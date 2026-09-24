use barepdf_core::MemoryBudget;

pub struct SystemHardwareProfile {
    pub total_ram_mb: u64,
    pub primary_screen_dpi: f32,
}

#[must_use]
pub fn calculate_adaptive_memory_budget(profile: &SystemHardwareProfile) -> MemoryBudget {
    let final_mb = if profile.total_ram_mb <= 4096 {
        256
    } else if profile.total_ram_mb <= 8192 {
        384
    } else {
        1024
    };

    MemoryBudget::new(final_mb * 1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_memory_budget() {
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
}
