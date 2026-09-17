//! Compile-time gate for development and test-harness environment hooks.
//!
//! Several environment variables let the automated test harness redirect
//! executable paths, config locations, PID files, and destructive uninstall
//! targets. They are honored only when debug assertions are enabled, so release
//! binaries always use their production defaults and ignore these overrides.
//!
//! Debug builds (including `cargo test` and the Docker integration tests) keep
//! the hooks so fixtures and stubs can drive the binaries.
//!
//! `DEKU_CONFIG_DIR` and `DEKU_DASHBOARD_HOST` are intentionally not gated:
//! they are part of the installer and CLI contract, not test-only hooks.

/// Whether development and test-harness environment overrides are honored.
pub fn enabled() -> bool {
    cfg!(debug_assertions)
}
