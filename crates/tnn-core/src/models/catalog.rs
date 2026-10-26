//! Embedded model catalog and recommendation logic.
//!
//! The catalog is compiled into the binary. There is no remote manifest in v1:
//! a compromised CDN cannot silently add a model. Every downloadable entry must
//! carry a pinned HTTPS URL, a revision and a SHA-256.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::{AppError, Result};
use crate::hardware::HardwareProfile;

/// The catalog that ships with the app.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Catalog {
    pub schema_version: u32,
    pub catalog_version: String,
    pub models: Vec<ModelSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ModelTier {
    Fast,
    Balanced,
    Smart,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ValidationStatus {
    /// Passed the spike / validation gate; safe to offer.
    Validated,
    /// Announced in the plan but not yet converted/benchmarked. Never offered.
    Pending,
    /// Failed validation. Kept for diagnostics, never offered.
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Sampling {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    pub min_p: f32,
    pub presence_penalty: f32,
    pub repeat_penalty: f32,
    pub repeat_last_n: u32,
}

impl Default for Sampling {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.8,
            top_k: 20,
            min_p: 0.0,
            presence_penalty: 1.5,
            repeat_penalty: 1.0,
            repeat_last_n: 64,
        }
    }
}

/// One model in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModelSpec {
    pub id: String,
    pub display_name: String,
    pub tier: ModelTier,
    pub family: String,
    pub params_b: f64,
    pub quant: String,
    pub file_name: String,
    pub url: Option<String>,
    pub sha256: Option<String>,
    pub size_bytes: u64,
    pub license: String,
    pub license_url: String,
    pub model_card_url: String,
    pub revision: Option<String>,
    pub min_ram_bytes: u64,
    pub recommended_ram_bytes: u64,
    pub min_disk_bytes: u64,
    pub context_default: u32,
    pub context_max: u32,
    pub max_tokens_default: u32,
    pub sampling: Sampling,
    pub enable_thinking: bool,
    pub engine_min_tier: String,
    pub architecture: Option<String>,
    pub description: String,
    pub badge: Option<String>,
    pub experimental: bool,
    pub validation: ValidationStatus,
    pub unavailable_reason: Option<String>,
}

impl ModelSpec {
    pub fn is_downloadable(&self) -> bool {
        self.validation == ValidationStatus::Validated
            && self.url.is_some()
            && self.sha256.as_deref().is_some_and(is_sha256_hex)
    }

    pub fn required_disk_bytes(&self) -> u64 {
        self.size_bytes.saturating_add(512 * 1024 * 1024)
    }

    /// A card describes whether this model is usable on a given machine right
    /// now. `installed` is filled in by the app layer.
    pub fn card(
        &self,
        hardware: &HardwareProfile,
        recommended_id: Option<&str>,
        installed: bool,
    ) -> ModelCard {
        let fits_ram = hardware.fits(self.min_ram_bytes);
        let fits_disk = hardware.free_disk_bytes >= self.required_disk_bytes();
        let unavailable_reason = if !self.is_downloadable() {
            Some(
                self.unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "experimental_pending_validation".to_string()),
            )
        } else if !fits_ram {
            Some("needs_more_ram".to_string())
        } else if !fits_disk && !installed {
            Some("needs_more_disk".to_string())
        } else {
            None
        };

        ModelCard {
            model: self.clone(),
            installed,
            recommended: recommended_id == Some(self.id.as_str()),
            compatible: unavailable_reason.is_none() || installed,
            unavailable_reason,
        }
    }
}

/// A catalog entry decorated with machine-specific availability.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModelCard {
    pub model: ModelSpec,
    pub installed: bool,
    pub recommended: bool,
    pub compatible: bool,
    pub unavailable_reason: Option<String>,
}

impl Catalog {
    /// Parse and validate the embedded catalog.
    pub fn embedded() -> Result<Self> {
        let catalog: Catalog = serde_json::from_str(include_str!("../../assets/catalog.json"))?;
        catalog.validate()?;
        Ok(catalog)
    }

    /// Validate structural, security and licensing invariants.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err(AppError::InvalidInput {
                detail: format!("unsupported catalog schema {}", self.schema_version),
            });
        }
        if self.models.is_empty() {
            return Err(AppError::InvalidInput {
                detail: "catalog has no models".into(),
            });
        }

        let mut ids = HashSet::new();
        let mut files = HashSet::new();
        for model in &self.models {
            validate_model(model)?;
            if !ids.insert(model.id.clone()) {
                return Err(AppError::InvalidInput {
                    detail: format!("duplicate model id {}", model.id),
                });
            }
            if !files.insert(model.file_name.clone()) {
                return Err(AppError::InvalidInput {
                    detail: format!("duplicate model file {}", model.file_name),
                });
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&ModelSpec> {
        self.models.iter().find(|model| model.id == id)
    }

    /// Prefer models with recommended RAM headroom, then the largest that fits.
    /// Minimum mode stays at or below 2B parameters, including on a two-core
    /// machine with enough RAM for a larger model.
    pub fn recommended(&self, hardware: &HardwareProfile) -> Option<&ModelSpec> {
        let mut candidates: Vec<&ModelSpec> = self
            .models
            .iter()
            .filter(|model| model.is_downloadable() && !model.experimental)
            .filter(|model| hardware.total_ram_bytes >= model.min_ram_bytes)
            .filter(|model| !hardware.minimum_mode || model.params_b <= 2.0)
            .collect();
        candidates.sort_by(|a, b| {
            (hardware.total_ram_bytes >= a.recommended_ram_bytes)
                .cmp(&(hardware.total_ram_bytes >= b.recommended_ram_bytes))
                .then_with(|| a.params_b.total_cmp(&b.params_b))
        });
        candidates.last().copied()
    }

    pub fn recommended_id(&self, hardware: &HardwareProfile) -> Option<&str> {
        self.recommended(hardware).map(|model| model.id.as_str())
    }

    pub fn cards(&self, hardware: &HardwareProfile, installed: &HashSet<String>) -> Vec<ModelCard> {
        let recommended = self.recommended_id(hardware);
        self.models
            .iter()
            .map(|model| model.card(hardware, recommended, installed.contains(&model.id)))
            .collect()
    }

    /// Models the UI shows by default: validated, non-experimental entries.
    pub fn visible_models(&self) -> impl Iterator<Item = &ModelSpec> {
        self.models
            .iter()
            .filter(|model| !model.experimental && model.validation == ValidationStatus::Validated)
    }
}

fn validate_model(model: &ModelSpec) -> Result<()> {
    if model.id.is_empty()
        || !model
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::InvalidInput {
            detail: format!("invalid model id {:?}", model.id),
        });
    }
    if !model.file_name.ends_with(".gguf")
        || model.file_name.contains('/')
        || model.file_name.contains('\\')
        || model.file_name.contains("..")
    {
        return Err(AppError::InvalidInput {
            detail: format!("invalid model file name {:?}", model.file_name),
        });
    }
    if model.size_bytes == 0 {
        return Err(AppError::InvalidInput {
            detail: format!("model {} has zero size", model.id),
        });
    }
    if model.min_ram_bytes == 0 || model.recommended_ram_bytes < model.min_ram_bytes {
        return Err(AppError::InvalidInput {
            detail: format!("model {} has inconsistent RAM requirements", model.id),
        });
    }
    if model.context_default == 0
        || model.context_default > model.context_max
        || model.context_max > 4 * 1024 * 1024
    {
        return Err(AppError::InvalidInput {
            detail: format!("model {} has invalid context sizes", model.id),
        });
    }
    if model.max_tokens_default == 0 || model.max_tokens_default >= model.context_default {
        return Err(AppError::InvalidInput {
            detail: format!("model {} has invalid max tokens", model.id),
        });
    }
    let sampling = &model.sampling;
    if !(0.0..=2.0).contains(&sampling.temperature)
        || !(0.0..=1.0).contains(&sampling.top_p)
        || !(0.0..=1.0).contains(&sampling.min_p)
        || !(0.0..=2.0).contains(&sampling.presence_penalty)
    {
        return Err(AppError::InvalidInput {
            detail: format!("model {} has out-of-range sampling settings", model.id),
        });
    }
    if model.license.trim().is_empty() {
        return Err(AppError::InvalidInput {
            detail: format!("model {} has no license", model.id),
        });
    }

    if model.validation == ValidationStatus::Validated {
        let url = model.url.as_deref().ok_or_else(|| AppError::InvalidInput {
            detail: format!("validated model {} has no URL", model.id),
        })?;
        if !crate::models::verify::is_allowed_model_url(url) {
            return Err(AppError::DownloadBlocked {
                detail: format!("{url} is not an allowed model host"),
            });
        }
        let sha = model
            .sha256
            .as_deref()
            .ok_or_else(|| AppError::InvalidInput {
                detail: format!("validated model {} has no SHA-256", model.id),
            })?;
        if !is_sha256_hex(sha) {
            return Err(AppError::InvalidInput {
                detail: format!("validated model {} has an invalid SHA-256", model.id),
            });
        }
        if model.unavailable_reason.is_some() {
            return Err(AppError::InvalidInput {
                detail: format!(
                    "validated model {} cannot have unavailable_reason",
                    model.id
                ),
            });
        }
    }
    Ok(())
}

pub fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::{CpuFeatures, HardwareProfile};

    fn hardware(ram_gib: u64, cores: usize) -> HardwareProfile {
        HardwareProfile::from_parts(
            "Linux".into(),
            "x86_64".into(),
            "Test".into(),
            cores,
            cores * 2,
            ram_gib * 1024_u64.pow(3),
            ram_gib * 1024_u64.pow(3),
            200 * 1024_u64.pow(3),
            CpuFeatures {
                sse2: true,
                ssse3: true,
                sse42: true,
                ..Default::default()
            },
        )
    }

    #[test]
    fn embedded_catalog_is_valid() {
        let catalog = Catalog::embedded().unwrap();
        assert!(catalog.models.len() >= 4);
        let visible: Vec<_> = catalog.visible_models().collect();
        assert_eq!(visible.len(), 4);
        assert!(visible.iter().all(|model| model.family.starts_with("Qwen")));
    }

    #[test]
    fn four_gb_recommends_qwen_one_point_five_b() {
        let catalog = Catalog::embedded().unwrap();
        let hw = HardwareProfile::synthetic_minimum();
        assert_eq!(
            catalog.recommended_id(&hw),
            Some("qwen2-5-1-5b-instruct-q4-k-m")
        );
    }

    #[test]
    fn eight_gb_prefers_headroom_over_seven_b_minimum_ram() {
        let catalog = Catalog::embedded().unwrap();
        assert_eq!(
            catalog.recommended_id(&hardware(8, 4)),
            Some("qwen2-5-1-5b-instruct-q4-k-m")
        );
    }

    #[test]
    fn sixteen_gb_recommends_qwen_seven_b() {
        let catalog = Catalog::embedded().unwrap();
        assert_eq!(
            catalog.recommended_id(&hardware(16, 8)),
            Some("qwen2-5-7b-instruct-q4-k-m")
        );
    }

    #[test]
    fn minimum_mode_keeps_qwen_small_even_with_sixteen_gb() {
        let catalog = Catalog::embedded().unwrap();
        assert_eq!(
            catalog.recommended_id(&hardware(16, 2)),
            Some("qwen2-5-1-5b-instruct-q4-k-m")
        );
    }

    #[test]
    fn unvalidated_models_are_not_downloadable() {
        let catalog = Catalog::embedded().unwrap();
        for model in catalog
            .models
            .iter()
            .filter(|m| m.validation != ValidationStatus::Validated)
        {
            assert!(!model.is_downloadable(), "{} must stay hidden", model.id);
        }
    }

    #[test]
    fn experimental_models_never_become_default_recommendations() {
        let mut catalog = Catalog::embedded().unwrap();
        let model = catalog
            .models
            .iter_mut()
            .find(|m| m.id == "bitnet-2b4t")
            .unwrap();
        model.validation = ValidationStatus::Validated;
        model.params_b = 100.0;
        assert_ne!(
            catalog.recommended_id(&hardware(64, 8)),
            Some("bitnet-2b4t")
        );
    }

    #[test]
    fn cards_explain_incompatibility() {
        let catalog = Catalog::embedded().unwrap();
        let hw = HardwareProfile::synthetic_minimum();
        let cards = catalog.cards(&hw, &HashSet::new());
        let seven = cards
            .iter()
            .find(|c| c.model.id == "qwen2-5-7b-instruct-q4-k-m")
            .unwrap();
        assert!(!seven.compatible);
        assert_eq!(seven.unavailable_reason.as_deref(), Some("needs_more_ram"));
    }

    #[test]
    fn larger_qwen_models_respect_ram_limits_and_recommendations() {
        let catalog = Catalog::embedded().unwrap();
        for (ram, cores, expected, recommended) in [
            (4, 2, [true, false], "qwen2-5-1-5b-instruct-q4-k-m"),
            (6, 4, [true, false], "qwen2-5-1-5b-instruct-q4-k-m"),
            (8, 4, [true, true], "qwen2-5-1-5b-instruct-q4-k-m"),
            (16, 8, [true, true], "qwen2-5-7b-instruct-q4-k-m"),
        ] {
            let hw = hardware(ram, cores);
            let cards = catalog.cards(&hw, &HashSet::new());
            for (id, compatible) in ["qwen2-5-1-5b-instruct-q4-k-m", "qwen2-5-7b-instruct-q4-k-m"]
                .into_iter()
                .zip(expected)
            {
                let card = cards.iter().find(|card| card.model.id == id).unwrap();
                assert_eq!(card.compatible, compatible, "{id} on {ram} GiB");
                assert_eq!(
                    card.unavailable_reason.as_deref(),
                    if compatible {
                        None
                    } else {
                        Some("needs_more_ram")
                    },
                    "{id} on {ram} GiB"
                );
            }
            assert_eq!(catalog.recommended_id(&hw), Some(recommended));
        }
    }

    #[test]
    fn rejects_path_traversal_file_names() {
        let mut catalog = Catalog::embedded().unwrap();
        catalog.models[0].file_name = "../escape.gguf".into();
        assert!(catalog.validate().is_err());
    }
}
