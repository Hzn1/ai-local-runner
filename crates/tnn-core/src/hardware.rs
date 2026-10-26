//! Hardware detection and capability tiering.
//!
//! Correctness rules encoded here:
//! * threads are physical cores, never logical CPUs;
//! * AVX2 is never required;
//! * 4 GB / 2-core machines are supported, but explicitly run in "Minimum
//!   mode" with a short context and short answers.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sysinfo::{Disks, System};
use ts_rs::TS;

use crate::errors::Result;

/// Coarse hardware tier used for model recommendations and defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HardwareTier {
    Minimum,
    Low,
    Balanced,
    High,
    Ultra,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CpuFeatures {
    pub sse2: bool,
    pub ssse3: bool,
    pub sse42: bool,
    pub avx: bool,
    pub avx2: bool,
    pub fma: bool,
    pub f16c: bool,
}

impl CpuFeatures {
    /// The best runtime-dispatch directory name this CPU can use. Never used to
    /// *require* a tier: the engine's own dispatcher remains authoritative.
    pub fn best_engine_tier(&self) -> &'static str {
        if self.avx2 {
            "haswell"
        } else if self.avx {
            "ivybridge"
        } else if self.sse42 {
            "sse42"
        } else if self.ssse3 {
            "ssse3"
        } else {
            "x64"
        }
    }
}

/// Everything the rest of the app needs to know about the host.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HardwareProfile {
    pub os: String,
    pub arch: String,
    pub cpu_brand: String,
    pub physical_cores: usize,
    pub logical_cores: usize,
    pub total_ram_bytes: u64,
    pub available_ram_bytes: u64,
    pub free_disk_bytes: u64,
    pub features: CpuFeatures,
    pub minimum_mode: bool,
    pub tier: HardwareTier,
    pub notes: Vec<String>,
}

impl HardwareProfile {
    /// Detect the live machine. `disk_probe` is a path on the volume that will
    /// hold models (normally the app-data directory).
    pub fn detect(disk_probe: &Path) -> Result<Self> {
        let mut system = System::new_all();
        system.refresh_all();

        let logical_cores = system.cpus().len().max(1);
        let physical_cores = physical_cores(&system).unwrap_or_else(|| (logical_cores / 2).max(1));
        let total_ram = system.total_memory();
        let available_ram = system.available_memory();

        let cpu_brand = system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .filter(|brand| !brand.is_empty())
            .unwrap_or_else(|| "Unknown CPU".to_string());

        let os = match System::long_os_version() {
            Some(version) => format!("{} ({})", System::name().unwrap_or_default(), version),
            None => std::env::consts::OS.to_string(),
        };

        let free_disk = free_disk_bytes(disk_probe);

        Ok(Self::from_parts(
            os,
            std::env::consts::ARCH.to_string(),
            cpu_brand,
            physical_cores,
            logical_cores,
            total_ram,
            available_ram,
            free_disk,
            detect_features(),
        ))
    }

    /// Deterministic constructor used by tests and onboarding previews.
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        os: String,
        arch: String,
        cpu_brand: String,
        physical_cores: usize,
        logical_cores: usize,
        total_ram_bytes: u64,
        available_ram_bytes: u64,
        free_disk_bytes: u64,
        features: CpuFeatures,
    ) -> Self {
        let gib = 1024_u64 * 1024 * 1024;
        let physical_cores = physical_cores.max(1);
        let minimum_mode = total_ram_bytes < 6 * gib || physical_cores <= 2;
        let tier = if minimum_mode || total_ram_bytes < 8 * gib {
            HardwareTier::Minimum
        } else if total_ram_bytes < 12 * gib {
            HardwareTier::Low
        } else if total_ram_bytes < 20 * gib {
            HardwareTier::Balanced
        } else if total_ram_bytes < 32 * gib {
            HardwareTier::High
        } else {
            HardwareTier::Ultra
        };

        let mut notes = Vec::new();
        if minimum_mode {
            notes.push("minimum_mode".to_string());
        }
        if !features.sse42 {
            notes.push("webview_may_require_sse42".to_string());
        }
        if !features.avx2 {
            notes.push("no_avx2_using_dispatch".to_string());
        }
        if free_disk_bytes < 3 * gib {
            notes.push("low_disk_space".to_string());
        }

        Self {
            os,
            arch,
            cpu_brand,
            physical_cores,
            logical_cores,
            total_ram_bytes,
            available_ram_bytes,
            free_disk_bytes,
            features,
            minimum_mode,
            tier,
            notes,
        }
    }

    /// Synthetic 4 GB / 2-core / no-AVX2 machine used for the minimum-mode gate.
    pub fn synthetic_minimum() -> Self {
        Self::from_parts(
            "Linux".into(),
            "x86_64".into(),
            "Intel Core i3-3217U".into(),
            2,
            4,
            4 * 1024_u64.pow(3),
            2 * 1024_u64.pow(3),
            20 * 1024_u64.pow(3),
            CpuFeatures {
                sse2: true,
                ssse3: true,
                sse42: true,
                avx: true,
                avx2: false,
                fma: false,
                f16c: true,
            },
        )
    }

    /// Threads = physical cores, capped at 8. Always at least one.
    pub fn default_threads(&self) -> usize {
        self.physical_cores.clamp(1, 8)
    }

    pub fn default_context(&self) -> u32 {
        match self.tier {
            HardwareTier::Minimum => 2048,
            HardwareTier::Low => 4096,
            HardwareTier::Balanced | HardwareTier::High | HardwareTier::Ultra => 8192,
        }
    }

    pub fn default_max_tokens(&self) -> u32 {
        match self.tier {
            HardwareTier::Minimum => 256,
            HardwareTier::Low => 512,
            HardwareTier::Balanced | HardwareTier::High | HardwareTier::Ultra => 1024,
        }
    }

    pub fn default_batch(&self) -> (u32, u32) {
        match self.tier {
            HardwareTier::Minimum => (256, 128),
            HardwareTier::Low => (512, 256),
            HardwareTier::Balanced | HardwareTier::High | HardwareTier::Ultra => (1024, 512),
        }
    }

    /// True when this machine can plausibly run a model needing `min_ram`.
    pub fn fits(&self, min_ram_bytes: u64) -> bool {
        self.total_ram_bytes >= min_ram_bytes
    }
}

fn physical_cores(system: &System) -> Option<usize> {
    // `physical_core_count` is an associated function in current sysinfo.
    let count = System::physical_core_count();
    if count.unwrap_or(0) > 0 {
        return count;
    }
    // Fallback for containers that do not expose topology.
    let logical = system.cpus().len();
    (logical > 0).then(|| (logical / 2).max(1))
}

fn free_disk_bytes(path: &Path) -> u64 {
    let disks = Disks::new_with_refreshed_list();
    let mut best: Option<(usize, u64)> = None;
    for disk in disks.list() {
        let mount = disk.mount_point();
        if path.starts_with(mount) {
            let depth = mount.components().count();
            if best.map(|(d, _)| depth > d).unwrap_or(true) {
                best = Some((depth, disk.available_space()));
            }
        }
    }
    best.map(|(_, free)| free).unwrap_or_else(|| {
        disks
            .list()
            .iter()
            .map(|d| d.available_space())
            .max()
            .unwrap_or(0)
    })
}

#[cfg(target_arch = "x86_64")]
fn detect_features() -> CpuFeatures {
    let cpuid = raw_cpuid::CpuId::new();
    let mut features = CpuFeatures::default();
    if let Some(info) = cpuid.get_feature_info() {
        features.sse2 = info.has_sse2();
        features.ssse3 = info.has_ssse3();
        features.sse42 = info.has_sse42();
        features.avx = info.has_avx();
        features.fma = info.has_fma();
        features.f16c = info.has_f16c();
    }
    if let Some(info) = cpuid.get_extended_feature_info() {
        features.avx2 = info.has_avx2();
    }
    features
}

#[cfg(not(target_arch = "x86_64"))]
fn detect_features() -> CpuFeatures {
    CpuFeatures {
        sse2: true,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_gb_two_core_is_minimum_mode() {
        let hw = HardwareProfile::synthetic_minimum();
        assert!(hw.minimum_mode);
        assert_eq!(hw.tier, HardwareTier::Minimum);
        assert_eq!(hw.default_threads(), 2);
        assert_eq!(hw.default_context(), 2048);
        assert_eq!(hw.default_max_tokens(), 256);
        assert_eq!(hw.default_batch(), (256, 128));
        assert_eq!(hw.features.best_engine_tier(), "ivybridge");
    }

    #[test]
    fn eight_gb_four_core_is_low_tier() {
        let hw = HardwareProfile::from_parts(
            "Linux".into(),
            "x86_64".into(),
            "Test".into(),
            4,
            8,
            8 * 1024_u64.pow(3),
            6 * 1024_u64.pow(3),
            50 * 1024_u64.pow(3),
            CpuFeatures {
                sse2: true,
                ssse3: true,
                sse42: true,
                ..Default::default()
            },
        );
        assert!(!hw.minimum_mode);
        assert_eq!(hw.tier, HardwareTier::Low);
        assert_eq!(hw.default_threads(), 4);
        assert_eq!(hw.default_context(), 4096);
    }

    #[test]
    fn thread_cap_is_eight() {
        let hw = HardwareProfile::from_parts(
            "Linux".into(),
            "x86_64".into(),
            "Test".into(),
            16,
            32,
            64 * 1024_u64.pow(3),
            60 * 1024_u64.pow(3),
            500 * 1024_u64.pow(3),
            CpuFeatures::default(),
        );
        assert_eq!(hw.default_threads(), 8);
        assert_eq!(hw.tier, HardwareTier::Ultra);
    }

    #[test]
    fn no_avx2_still_selects_a_real_tier() {
        let f = CpuFeatures {
            sse2: true,
            ssse3: true,
            sse42: true,
            avx: false,
            ..Default::default()
        };
        assert_eq!(f.best_engine_tier(), "sse42");
    }
}
