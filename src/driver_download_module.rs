//! Assemble the Playwright driver at runtime, into this machine's user cache.
//!
//! `build.rs` assembles the driver on the *build* machine and records that
//! machine's cache path, which is the wrong machine for anyone who receives a
//! built binary: the recorded path names the builder, and the user cache is
//! empty on the machine that runs the binary, so every lookup in
//! [`super::driver`] misses. This module is the step that puts a driver on
//! *this* machine, at the path the resolution chain already probes —
//! [`crate::driver_urls::cached_driver_dir`].
//!
//! The assembly itself is [`crate::driver_assembly::assemble_driver`], the one
//! implementation the build script and the cli binary share. There is no
//! second downloader here to keep in step.

use std::path::{Path, PathBuf};

/// Assemble the driver into this machine's user cache when it is not already
/// there. Returns the directory that now holds it.
///
/// `Ok(None)` when the download is disabled for this process, or when the build
/// recorded no version/platform to name the destination with; the caller then
/// reports the resolution failure it already had.
pub(crate) fn assemble_into_user_cache(
    env: impl Fn(&str) -> Option<String>,
) -> std::io::Result<Option<PathBuf>> {
    assemble_into_user_cache_with(
        env,
        baked_version_and_platform(),
        dirs::cache_dir(),
        crate::driver_assembly::assemble_driver,
    )
}

/// The version and platform this build recorded.
///
/// These name the destination directory, so they must be the values
/// `super::driver::try_user_cache_driver` probes with, or an assembled driver
/// would land where the lookup does not look. `build.rs` emits both on every
/// path, including a skipped or failed download.
fn baked_version_and_platform() -> Option<(&'static str, &'static str)> {
    let version = option_env!("PLAYWRIGHT_DRIVER_VERSION").filter(|v| !v.is_empty())?;
    let platform = option_env!("PLAYWRIGHT_DRIVER_PLATFORM").filter(|p| !p.is_empty())?;
    Some((version, platform))
}

/// [`assemble_into_user_cache`] with every input injected, so the decision is
/// testable without mutating the process environment, touching the real cache,
/// or downloading.
///
/// `env` supplies the process environment; `recorded` stands in for the
/// compile-time version/platform; `cache_root` is the user cache root (`None`
/// where no home directory resolves); `assemble` performs the download.
pub(crate) fn assemble_into_user_cache_with(
    env: impl Fn(&str) -> Option<String>,
    recorded: Option<(&str, &str)>,
    cache_root: Option<PathBuf>,
    assemble: impl Fn(&Path, &str, &str) -> std::io::Result<()>,
) -> std::io::Result<Option<PathBuf>> {
    // Exported-but-empty counts as unset, the convention `build.rs` and
    // `super::driver` both use.
    let env = |name: &str| env(name).filter(|value| !value.is_empty());
    if env("PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD").is_some() {
        return Ok(None);
    }
    let (Some((version, platform)), Some(cache_root)) = (recorded, cache_root) else {
        return Ok(None);
    };

    let driver_dir = crate::driver_urls::cached_driver_dir(&cache_root, version, platform);
    if crate::driver_assembly::driver_complete(&driver_dir, platform) {
        return Ok(Some(driver_dir));
    }
    assemble(&driver_dir, version, platform)?;
    Ok(Some(driver_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::path::{Path, PathBuf};

    /// A driver directory in the shape `driver_complete` accepts.
    fn fake_driver(driver_dir: &Path, platform: &str) {
        let node = driver_dir.join(crate::driver_urls::node_exe_name(platform));
        let cli = driver_dir.join("package").join("cli.js");
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(&node, b"").unwrap();
        std::fs::write(&cli, b"").unwrap();
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn skip_knob(name: &str) -> Option<String> {
        (name == "PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD").then(|| "1".to_string())
    }

    #[test]
    fn the_production_entry_point_skips_a_skipped_download() {
        // Also keeps `assemble_into_user_cache` exercised: it is the only
        // caller of the injected form below, and nothing else reaches it until
        // `ensure_driver` does.
        assert!(assemble_into_user_cache(skip_knob).unwrap().is_none());
    }

    #[test]
    fn a_skipped_download_is_skipped_at_runtime_too() {
        let calls = Cell::new(0);
        let temp = tempfile::tempdir().unwrap();

        let result = assemble_into_user_cache_with(
            skip_knob,
            Some(("1.0.0", "linux")),
            Some(temp.path().into()),
            |_, _, _| {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .unwrap();

        assert!(result.is_none());
        assert_eq!(calls.get(), 0, "the knob must not download");
    }

    #[test]
    fn an_empty_skip_value_counts_as_unset() {
        let env = |name: &str| (name == "PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD").then(String::new);
        let temp = tempfile::tempdir().unwrap();

        let result = assemble_into_user_cache_with(
            env,
            Some(("1.0.0", "linux")),
            Some(temp.path().into()),
            |dir, _, platform| {
                fake_driver(dir, platform);
                Ok(())
            },
        )
        .unwrap();

        assert!(result.is_some(), "an exported-but-empty knob is not a skip");
    }

    #[test]
    fn a_missing_cache_root_or_record_skips_without_downloading() {
        // `Fn` and `Copy`: captures a `&Cell`, so it can be handed to both
        // calls without borrowing mutably.
        let calls = Cell::new(0);
        let assemble = |_: &Path, _: &str, _: &str| {
            calls.set(calls.get() + 1);
            Ok(())
        };
        let temp = tempfile::tempdir().unwrap();

        assert!(
            assemble_into_user_cache_with(no_env, Some(("1.0.0", "linux")), None, assemble)
                .unwrap()
                .is_none(),
            "no cache root means no destination"
        );
        assert!(
            assemble_into_user_cache_with(no_env, None, Some(temp.path().into()), assemble)
                .unwrap()
                .is_none(),
            "no recorded version/platform means no destination"
        );
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn the_destination_is_the_directory_the_resolution_chain_probes() {
        let temp = tempfile::tempdir().unwrap();
        let seen: Cell<Option<PathBuf>> = Cell::new(None);

        let result = assemble_into_user_cache_with(
            no_env,
            Some(("1.63.0", "mac-arm64")),
            Some(temp.path().into()),
            |dir, _, platform| {
                seen.set(Some(dir.to_path_buf()));
                fake_driver(dir, platform);
                Ok(())
            },
        )
        .unwrap()
        .unwrap();

        // The contract the whole scheme rests on: the assembler writes where
        // `try_user_cache_driver` looks, or the next launch finds nothing.
        let expected = crate::driver_urls::cached_driver_dir(temp.path(), "1.63.0", "mac-arm64");
        assert_eq!(result, expected);
        assert_eq!(seen.take().unwrap(), expected);
    }

    #[test]
    fn an_existing_driver_is_reused_instead_of_downloaded() {
        let temp = tempfile::tempdir().unwrap();
        let existing = crate::driver_urls::cached_driver_dir(temp.path(), "1.63.0", "linux");
        fake_driver(&existing, "linux");
        let calls = Cell::new(0);

        let result = assemble_into_user_cache_with(
            no_env,
            Some(("1.63.0", "linux")),
            Some(temp.path().into()),
            |_, _, _| {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .unwrap()
        .unwrap();

        assert_eq!(result, existing);
        assert_eq!(calls.get(), 0, "a driver already on disk is not re-fetched");
    }

    #[test]
    fn a_failed_assembly_carries_the_reason_out() {
        let temp = tempfile::tempdir().unwrap();
        let error = assemble_into_user_cache_with(
            no_env,
            Some(("1.63.0", "linux")),
            Some(temp.path().into()),
            |_, _, _| {
                Err(std::io::Error::other(
                    "download of https://example.invalid failed",
                ))
            },
        )
        .unwrap_err();

        assert!(
            error.to_string().contains("https://example.invalid"),
            "the failing URL is the diagnosis: {error}"
        );
    }
}
