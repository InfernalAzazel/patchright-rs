// Driver assembly: download the `playwright-core` npm tarball plus a pinned
// Node.js binary and lay them out exactly like the discontinued prebuilt zip
// (`<dir>/node[.exe]` + `<dir>/package/cli.js`), so the runtime launch path
// is unchanged (see ADR 0006).
//
// `include!`d by `build.rs` and the cli binary — the two places that acquire
// a driver. Effectful (ureq / zip / flate2 / tar); the pure mapping lives in
// `driver_urls.rs`, included below, and is unit-tested from the lib suite.

include!("driver_urls.rs");

#[cfg(not(any(feature = "aws-lc", feature = "ring")))]
compile_error!("enable either the `aws-lc` or `ring` feature to select a TLS crypto backend");

/// Builds the crypto provider used by the driver's HTTP client.
///
/// `aws-lc` wins when both features are present so `--all-features` and
/// dependency feature unification remain usable. Consumers that need to keep
/// `ring` out of their dependency graph must disable default features before
/// enabling `aws-lc`.
#[cfg(feature = "aws-lc")]
fn tls_crypto_provider() -> rustls::crypto::CryptoProvider {
    rustls::crypto::aws_lc_rs::default_provider()
}

#[cfg(all(not(feature = "aws-lc"), feature = "ring"))]
fn tls_crypto_provider() -> rustls::crypto::CryptoProvider {
    rustls::crypto::ring::default_provider()
}

/// Downloads and assembles the driver into `driver_dir` for a Playwright
/// `platform` identifier (e.g. `mac-arm64`).
///
/// Assembles into a sibling temp directory and renames into place, so an
/// interrupted run can't leave a half-populated `driver_dir` that a later
/// check mistakes for a complete driver, and a concurrent run that finishes
/// first wins cleanly. Errors name the URL or archive entry that failed, so
/// an upstream move is a one-line diagnosis.
#[allow(dead_code)]
pub(crate) fn assemble_driver(
    driver_dir: &std::path::Path,
    driver_version: &str,
    platform: &str,
    report: &dyn Fn(DriverProgress),
) -> std::io::Result<()> {
    use std::io;

    let triple = node_triple(platform).ok_or_else(|| {
        io::Error::other(format!("unsupported driver platform: {platform}"))
    })?;

    let parent = driver_dir
        .parent()
        .ok_or_else(|| io::Error::other("driver dir has no parent"))?;
    std::fs::create_dir_all(parent)?;

    // Temp dir next to the target so the final rename stays on one filesystem.
    let tmp = parent.join(format!(
        ".{}.partial-{}",
        driver_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("driver"),
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;

    let result = (|| {
        let core = http_get(&npm_core_url(driver_version), DriverArtifact::Core, report)?;
        report_safely(
            report,
            DriverProgress::Extracting {
                artifact: DriverArtifact::Core,
            },
        );
        extract_npm_package(&core, &tmp)?;

        let node = http_get(
            &node_archive_url(NODE_VERSION, triple),
            DriverArtifact::Node,
            report,
        )?;
        report_safely(
            report,
            DriverProgress::Extracting {
                artifact: DriverArtifact::Node,
            },
        );
        extract_node_binary(&node, NODE_VERSION, triple, &tmp)?;

        // The layout contract the runtime launch path depends on.
        for required in [
            tmp.join(node_exe_name(platform)),
            tmp.join("package").join("cli.js"),
        ] {
            if !required.exists() {
                return Err(io::Error::other(format!(
                    "assembled driver is missing {}",
                    required.display()
                )));
            }
        }
        Ok(())
    })();

    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }

    // A concurrent run may have completed first; its result is equally good.
    // Checked before the rename, and again if the rename fails, because the
    // other run can land between the two: renaming onto a populated
    // directory fails (ENOTEMPTY, or ERROR_ALREADY_EXISTS on Windows) and
    // must not be reported as a broken download when a whole driver now
    // sits at the destination.
    if driver_complete(driver_dir, platform) {
        let _ = std::fs::remove_dir_all(&tmp);
        report_safely(
            report,
            DriverProgress::Assembled {
                dir: driver_dir.to_path_buf(),
            },
        );
        return Ok(());
    }
    match std::fs::rename(&tmp, driver_dir) {
        Ok(()) => {
            report_safely(
                report,
                DriverProgress::Assembled {
                    dir: driver_dir.to_path_buf(),
                },
            );
            Ok(())
        }
        Err(_) if driver_complete(driver_dir, platform) => {
            let _ = std::fs::remove_dir_all(&tmp);
            report_safely(
                report,
                DriverProgress::Assembled {
                    dir: driver_dir.to_path_buf(),
                },
            );
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Give up on establishing the connection (including DNS and the TLS
/// handshake). ureq's default is `None`, i.e. wait forever.
#[allow(dead_code)]
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Give up on the response headers.
#[allow(dead_code)]
const RESPONSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Backstop for reading the whole body.
///
/// This is ureq's *total* budget for the body and it does not restart per read,
/// so it cannot be a stall detector: 122 MB on a 1 Mbps line needs 16 minutes,
/// and a tighter value would kill a slow-but-working download. It exists so a
/// hung transfer eventually fails instead of hanging the first launch forever.
#[allow(dead_code)]
const BODY_BUDGET: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Call a downstream reporter without letting its panic escape.
///
/// The reporter runs on the `spawn_blocking` thread; a panic there surfaces as
/// a `JoinError`, which `ensure_driver` reads as "the assembly task did not
/// finish" and reports a *successful* assembly as a failure. `AssertUnwindSafe`
/// is deliberate: the closure's state is dropped and never observed again.
#[allow(dead_code)]
fn report_safely(report: &dyn Fn(DriverProgress), progress: DriverProgress) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| report(progress)));
}

/// How many bytes are read (and reported) at a time.
#[allow(dead_code)]
const DOWNLOAD_CHUNK: usize = 64 * 1024;

/// Read `reader` to the end, handing the running byte count to `on_progress`
/// after each chunk.
#[allow(dead_code)]
fn read_streaming(
    reader: &mut impl std::io::Read,
    on_progress: &mut impl FnMut(u64),
) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; DOWNLOAD_CHUNK];
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read]);
        on_progress(out.len() as u64);
    }
    Ok(out)
}

/// Whether `driver_dir` holds the two files the runtime launches with.
#[allow(dead_code)]
pub(crate) fn driver_complete(driver_dir: &std::path::Path, platform: &str) -> bool {
    driver_dir.join(node_exe_name(platform)).exists()
        && driver_dir.join("package").join("cli.js").exists()
}

/// GET `url` into memory, reporting progress as the body arrives.
///
/// The artifacts are tens of MB, so the body is read in chunks (see
/// [`read_streaming`]) and every chunk is handed to `report`. The bytes are
/// still buffered whole because unpacking needs them: the Windows Node dist is
/// a zip and needs `Seek`. Errors carry the URL.
///
/// The timeouts are not optional: ureq's defaults are all `None` (wait
/// forever), which turns a stalled transfer into a first launch that never
/// finishes and never fails. See [`BODY_BUDGET`] for why the body bound is
/// loose rather than a stall detector.
#[allow(dead_code)]
fn http_get(
    url: &str,
    artifact: DriverArtifact,
    report: &dyn Fn(DriverProgress),
) -> std::io::Result<Vec<u8>> {
    use std::io;
    use std::sync::Arc;

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::Rustls)
                .unversioned_rustls_crypto_provider(Arc::new(tls_crypto_provider()))
                .build(),
        )
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_response(Some(RESPONSE_TIMEOUT))
        .timeout_recv_body(Some(BODY_BUDGET))
        .build()
        .into();

    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| io::Error::other(format!("download of {url} failed: {e}")))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(io::Error::other(format!(
            "download of {url} failed with status {status}"
        )));
    }

    let total = response.body_mut().content_length();
    report_safely(
        report,
        DriverProgress::Downloading {
            artifact,
            done: 0,
            total,
        },
    );

    let body = response.body_mut();
    let mut on_chunk = |done: u64| {
        report_safely(
            report,
            DriverProgress::Downloading {
                artifact,
                done,
                total,
            },
        );
    };
    let bytes = read_streaming(&mut body.as_reader(), &mut on_chunk)
        .map_err(|e| io::Error::other(format!("reading {url} failed: {e}")))?;
    Ok(bytes)
}

/// Extracts the npm tarball's `package/` subtree into `dest/package/`.
#[allow(dead_code)]
fn extract_npm_package(tgz: &[u8], dest: &std::path::Path) -> std::io::Result<()> {
    use std::io;

    let gz = flate2::read::GzDecoder::new(tgz);
    let mut archive = tar::Archive::new(gz);
    let mut entries = 0usize;
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.starts_with("package") {
            // unpack_in guards against path traversal outside `dest`.
            entry.unpack_in(dest)?;
            entries += 1;
        }
    }
    if entries == 0 {
        return Err(io::Error::other(
            "npm tarball contained no package/ entries",
        ));
    }
    Ok(())
}

/// Extracts the `node` executable (plus Node's LICENSE) out of the Node dist
/// archive into `dest/node[.exe]`, matching the old bundle layout.
#[allow(dead_code)]
fn extract_node_binary(
    archive_bytes: &[u8],
    node_version: &str,
    triple: &str,
    dest: &std::path::Path,
) -> std::io::Result<()> {
    use std::io;
    use std::io::Read;

    let exe_inner = node_archive_exe_path(node_version, triple);
    let license_inner = format!("node-v{node_version}-{triple}/LICENSE");

    let mut found_exe = false;
    if triple.starts_with("win-") {
        let cursor = io::Cursor::new(archive_bytes.to_vec());
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| io::Error::other(format!("Node archive is not a valid zip: {e}")))?;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| io::Error::other(format!("zip read failed: {e}")))?;
            let name = file.name().to_string();
            let out = if name == exe_inner {
                found_exe = true;
                dest.join("node.exe")
            } else if name == license_inner {
                dest.join("LICENSE")
            } else {
                continue;
            };
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            std::fs::write(&out, bytes)?;
        }
    } else {
        let gz = flate2::read::GzDecoder::new(archive_bytes);
        let mut archive = tar::Archive::new(gz);
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            let out = if path == std::path::Path::new(&exe_inner) {
                found_exe = true;
                dest.join("node")
            } else if path == std::path::Path::new(&license_inner) {
                dest.join("LICENSE")
            } else {
                continue;
            };
            entry.unpack(&out)?;
            #[cfg(unix)]
            if out.ends_with("node") {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&out)?.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&out, perms)?;
            }
        }
    }

    if !found_exe {
        return Err(io::Error::other(format!(
            "Node archive did not contain {exe_inner}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod progress_tests {
    use super::*;

    #[test]
    fn streaming_read_reports_each_chunk_and_keeps_the_bytes_intact() {
        let payload: Vec<u8> = (0..(DOWNLOAD_CHUNK * 2 + 7))
            .map(|i| (i % 251) as u8)
            .collect();
        let mut reported = Vec::new();
        let mut reader = std::io::Cursor::new(payload.clone());

        let read = read_streaming(&mut reader, &mut |done| reported.push(done)).unwrap();

        assert_eq!(read, payload, "the bytes must survive the chunked read");
        assert!(
            reported.len() >= 3,
            "must report more than once, otherwise it is not streaming: {reported:?}"
        );
        assert_eq!(reported.last(), Some(&(payload.len() as u64)));
        assert!(
            reported.windows(2).all(|pair| pair[0] <= pair[1]),
            "the running total must not go backwards: {reported:?}"
        );
    }

    #[test]
    fn streaming_read_of_an_empty_body_reports_nothing() {
        let mut reported = Vec::new();
        let mut reader = std::io::Cursor::new(Vec::<u8>::new());

        let read = read_streaming(&mut reader, &mut |done| reported.push(done)).unwrap();

        assert!(read.is_empty());
        assert!(reported.is_empty(), "an empty body has no progress to report");
    }
}

#[cfg(test)]
mod report_tests {
    use super::*;

    #[test]
    fn a_panicking_report_does_not_escape() {
        // A caller's panic must not become a `JoinError` in `ensure_driver`,
        // which would report a *successful* assembly as a failed one.
        report_safely(&|_| panic!("caller bug"), DriverProgress::Extracting {
            artifact: DriverArtifact::Core,
        });
    }

    #[test]
    fn a_report_sees_every_step_it_is_given() {
        use std::sync::Mutex;
        let seen = Mutex::new(Vec::new());
        let report = |progress: DriverProgress| seen.lock().unwrap().push(progress);

        report_safely(&report, DriverProgress::Assembled {
            dir: std::path::PathBuf::from("/c/driver"),
        });
        report_safely(&report, DriverProgress::Extracting {
            artifact: DriverArtifact::Node,
        });

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert!(matches!(seen[0], DriverProgress::Assembled { .. }));
    }
}
