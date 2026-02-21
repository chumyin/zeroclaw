pub mod feature_packs;
pub mod wizard;

// Re-exported for CLI and external use
#[allow(unused_imports)]
pub use wizard::{run_channels_repair_wizard, run_models_refresh, run_quick_setup, run_wizard};

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_reexport_exists<F>(_value: F) {}

    #[test]
    fn wizard_functions_are_reexported() {
        assert_reexport_exists(run_wizard);
        assert_reexport_exists(run_channels_repair_wizard);
        assert_reexport_exists(run_quick_setup);
        assert_reexport_exists(run_models_refresh);
        assert_reexport_exists(feature_pack_by_id);
        assert_reexport_exists(preset_by_id);
    }
}
