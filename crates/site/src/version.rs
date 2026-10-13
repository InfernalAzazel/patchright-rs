//! The build's version identity, from the `SITE_VERSION` env that `build.rs`
//! injects. Single source for the `dev` vs release-snapshot distinction the
//! components branch on.

/// The snapshot identifier: `"dev"` for the main-HEAD build, or the release
/// version (e.g. `"0.14.0"`).
pub const SITE_VERSION: &str = env!("SITE_VERSION");

/// Whether this is the unreleased main-HEAD (`dev`) build.
pub fn is_dev() -> bool {
    SITE_VERSION == "dev"
}
