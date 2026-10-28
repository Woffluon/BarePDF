use barepdf_core::MemoryBudget;

#[derive(Debug, Clone, PartialEq)]
pub struct SystemHardwareProfile {
    pub total_ram_mb: u64,
    pub primary_screen_dpi: f32,
}

impl SystemHardwareProfile {
    #[must_use]
    pub fn detect() -> Self {
        let total_ram_mb = detect_system_ram_mb().unwrap_or(4096);
        Self {
            total_ram_mb,
            primary_screen_dpi: 96.0,
        }
    }
}

impl Default for SystemHardwareProfile {
    fn default() -> Self {
        Self::detect()
    }
}

fn detect_system_ram_mb() -> Option<u64> {
    if let Ok(val) = std::env::var("BAREPDF_TOTAL_RAM_MB") {
        if let Ok(mb) = val.trim().parse::<u64>() {
            return Some(mb);
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if let Some(rest) = line.strip_prefix("MemTotal:") {
                    let kb_str = rest.trim().trim_end_matches("kB").trim();
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        return Some(kb / 1024);
                    }
                }
            }
        }
    }

    None
}

#[must_use]
pub fn calculate_adaptive_memory_budget(profile: &SystemHardwareProfile) -> MemoryBudget {
    let final_mb = if profile.total_ram_mb <= 1024 {
        64
    } else if profile.total_ram_mb <= 2048 {
        128
    } else if profile.total_ram_mb <= 4096 {
        256
    } else if profile.total_ram_mb <= 8192 {
        384
    } else {
        1024
    };

    MemoryBudget::new(final_mb * 1024 * 1024)
}

#[must_use]
pub fn calculate_page_image_budget(is_smartboard: bool) -> usize {
    if is_smartboard {
        64 * 1024 * 1024
    } else {
        32 * 1024 * 1024
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_memory_budget() {
        let budget = calculate_adaptive_memory_budget(&SystemHardwareProfile {
            total_ram_mb: 1024,
            primary_screen_dpi: 96.0,
        });
        assert_eq!(budget.get(), 64 * 1024 * 1024);

        let budget = calculate_adaptive_memory_budget(&SystemHardwareProfile {
            total_ram_mb: 2048,
            primary_screen_dpi: 96.0,
        });
        assert_eq!(budget.get(), 128 * 1024 * 1024);

        let budget = calculate_adaptive_memory_budget(&SystemHardwareProfile {
            total_ram_mb: 4096,
            primary_screen_dpi: 96.0,
        });
        assert_eq!(budget.get(), 256 * 1024 * 1024);

        let budget = calculate_adaptive_memory_budget(&SystemHardwareProfile {
            total_ram_mb: 8192,
            primary_screen_dpi: 96.0,
        });
        assert_eq!(budget.get(), 384 * 1024 * 1024);

        let budget = calculate_adaptive_memory_budget(&SystemHardwareProfile {
            total_ram_mb: 16384,
            primary_screen_dpi: 192.0,
        });
        assert_eq!(budget.get(), 1024 * 1024 * 1024);
    }

    #[test]
    fn test_calculate_page_image_budget() {
        assert_eq!(calculate_page_image_budget(false), 32 * 1024 * 1024);
        assert_eq!(calculate_page_image_budget(true), 64 * 1024 * 1024);
    }

    #[test]
    fn test_system_hardware_profile_detect() {
        let profile = SystemHardwareProfile::detect();
        assert!(profile.total_ram_mb > 0);
        assert_eq!(profile.primary_screen_dpi, 96.0);

        let default_profile = SystemHardwareProfile::default();
        assert_eq!(profile, default_profile);
    }
}
