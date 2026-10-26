//! Typed settings with defaults and validation.
//!
//! `None` means "use the hardware/model default". The UI is free to show the
//! resolved value without persisting it.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::errors::{AppError, Result};

pub const DEFAULT_SYSTEM_PROMPT: &str =
    "You are a helpful assistant. Answer clearly and concisely.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Language {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "pt-BR")]
    PtBr,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(default, deny_unknown_fields)]
#[ts(export)]
pub struct Settings {
    pub schema_version: u32,
    pub theme: Theme,
    pub language: Language,
    pub active_model_id: Option<String>,
    pub keep_model_loaded: bool,
    pub idle_unload_minutes: u32,
    pub threads: Option<u32>,
    pub context_size: Option<u32>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub presence_penalty: Option<f32>,
    pub system_prompt: String,
    pub show_reasoning: bool,
    pub telemetry_opt_in: bool,
    pub advanced_mode: bool,
    pub font_scale: f32,
    pub onboarding_complete: bool,
    /// Experimental models stay hidden unless this is explicitly enabled.
    pub include_experimental: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            theme: Theme::System,
            language: Language::En,
            active_model_id: None,
            keep_model_loaded: true,
            idle_unload_minutes: 30,
            threads: None,
            context_size: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            presence_penalty: None,
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            show_reasoning: false,
            telemetry_opt_in: false,
            advanced_mode: false,
            font_scale: 1.0,
            onboarding_complete: false,
            include_experimental: false,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err(AppError::InvalidInput {
                detail: format!("unsupported settings schema {}", self.schema_version),
            });
        }
        if let Some(threads) = self.threads {
            if !(1..=32).contains(&threads) {
                return Err(invalid("threads must be between 1 and 32"));
            }
        }
        if let Some(context) = self.context_size {
            if !(512..=32768).contains(&context) {
                return Err(invalid("context_size must be between 512 and 32768"));
            }
        }
        if let Some(max_tokens) = self.max_tokens {
            if !(1..=8192).contains(&max_tokens) {
                return Err(invalid("max_tokens must be between 1 and 8192"));
            }
        }
        if let Some(temperature) = self.temperature {
            if !(0.0..=2.0).contains(&temperature) {
                return Err(invalid("temperature must be between 0 and 2"));
            }
        }
        if let Some(top_p) = self.top_p {
            if !(0.0..=1.0).contains(&top_p) {
                return Err(invalid("top_p must be between 0 and 1"));
            }
        }
        if let Some(top_k) = self.top_k {
            if top_k > 500 {
                return Err(invalid("top_k must be 500 or less"));
            }
        }
        if let Some(penalty) = self.presence_penalty {
            if !(-2.0..=2.0).contains(&penalty) {
                return Err(invalid("presence_penalty must be between -2 and 2"));
            }
        }
        if self.idle_unload_minutes > 24 * 60 {
            return Err(invalid("idle_unload_minutes is too large"));
        }
        if !(0.8..=2.0).contains(&self.font_scale) {
            return Err(invalid("font_scale must be between 0.8 and 2.0"));
        }
        if self.system_prompt.len() > 4000 {
            return Err(invalid("system_prompt is too long"));
        }
        Ok(())
    }

    /// Apply a partial JSON patch. Unknown keys and invalid values are
    /// rejected; `null` resets an optional setting to its default.
    pub fn patched(&self, patch: &Value) -> Result<Self> {
        let base = serde_json::to_value(self)?;
        let patch = patch
            .as_object()
            .ok_or_else(|| invalid("settings patch must be an object"))?;
        let mut merged = base.as_object().cloned().unwrap_or_default();
        for (key, value) in patch {
            merged.insert(key.clone(), value.clone());
        }
        let next: Self = serde_json::from_value(Value::Object(merged))?;
        next.validate()?;
        Ok(next)
    }

    /// Resolve the effective thread count for a machine.
    pub fn resolved_threads(&self, hardware_threads: usize) -> usize {
        self.threads
            .map(|threads| threads as usize)
            .unwrap_or(hardware_threads)
            .clamp(1, hardware_threads.clamp(1, 8))
    }

    pub fn resolved_context(&self, default: u32) -> u32 {
        self.context_size.unwrap_or(default).clamp(512, 131_072)
    }

    pub fn resolved_max_tokens(&self, default: u32) -> u32 {
        self.max_tokens.unwrap_or(default).clamp(1, 8192)
    }
}

fn invalid(detail: impl Into<String>) -> AppError {
    AppError::InvalidInput {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_are_valid_and_non_thinking() {
        let settings = Settings::default();
        settings.validate().unwrap();
        assert!(!settings.show_reasoning);
        assert!(!settings.onboarding_complete);
        assert!(!settings.include_experimental);
    }

    #[test]
    fn patch_updates_and_keeps_defaults() {
        let settings = Settings::default();
        let next = settings
            .patched(&json!({"theme": "dark", "language": "pt-BR", "threads": 2}))
            .unwrap();
        assert_eq!(next.theme, Theme::Dark);
        assert_eq!(next.language, Language::PtBr);
        assert_eq!(next.threads, Some(2));
        assert_eq!(next.system_prompt, DEFAULT_SYSTEM_PROMPT);
    }

    #[test]
    fn patch_can_reset_optional_values_with_null() {
        let settings = Settings {
            threads: Some(4),
            ..Settings::default()
        };
        let next = settings.patched(&json!({"threads": null})).unwrap();
        assert_eq!(next.threads, None);
    }

    #[test]
    fn rejects_out_of_range_and_unknown_keys() {
        let settings = Settings::default();
        assert!(settings.patched(&json!({"threads": 99})).is_err());
        assert!(settings.patched(&json!({"temperature": 9.0})).is_err());
        assert!(settings.patched(&json!({"surprise": true})).is_err());
    }

    #[test]
    fn resolves_effective_values() {
        let settings = Settings {
            threads: Some(2),
            context_size: Some(2048),
            max_tokens: None,
            ..Settings::default()
        };
        assert_eq!(settings.resolved_threads(8), 2);
        assert_eq!(settings.resolved_context(4096), 2048);
        assert_eq!(settings.resolved_max_tokens(512), 512);
    }

    #[test]
    fn thread_override_never_exceeds_physical_cores() {
        let settings = Settings {
            threads: Some(32),
            ..Settings::default()
        };
        assert_eq!(settings.resolved_threads(4), 4);
        assert_eq!(settings.resolved_threads(16), 8);
        assert_eq!(settings.resolved_threads(0), 1);
    }
}
