//! Stage the RHCOS artifact set into the local cache, honoring online vs
//! offline mode, and verify each file against the published checksums.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::artifacts::ARTIFACTS;
use crate::cli::{Mode, SourceArgs};

/// A staged artifact: its role plus the local cache path of the raw file.
pub struct StagedArtifact {
    pub role: &'static str,
    pub in_image_name: &'static str,
    pub source_name: String,
    pub local_path: PathBuf,
}

/// The full staged set for one version/arch.
pub struct Staged {
    pub version: String,
    pub arch: String,
    pub artifacts: Vec<StagedArtifact>,
}

/// Resolve the concrete version to operate on.
pub fn resolve_version(src: &SourceArgs) -> Result<String> {
    if let Some(v) = &src.version {
        return Ok(v.clone());
    }
    match src.mode {
        Mode::Online => crate::mirror::find_latest(&src.series, &src.arch),
        Mode::Offline => bail!(
            "offline mode requires an explicit --version \
             (cannot resolve 'latest' without the mirror)"
        ),
    }
}

/// Ensure all available artifacts are present in the local cache and verified.
pub fn stage(src: &SourceArgs) -> Result<Staged> {
    let version = resolve_version(src)?;
    let cache_dir = src.cache.join(format!("{}-{}", version, src.arch));
    fs::create_dir_all(&cache_dir)?;

    let sum_body = stage_sumfile(src, &version, &cache_dir)?;

    let mut staged = Vec::new();
    for art in ARTIFACTS {
        let name = art.filename(&version, &src.arch);
        let local = cache_dir.join(&name);

        if !exists_nonempty(&local)? {
            if let Err(e) = fetch_one(src, &version, &name, &local) {
                if art.required {
                    return Err(e).with_context(|| {
                        format!(
                            "required artifact '{}' ({name}) could not be staged for \
                             {version} ({}); a bootable image cannot be assembled without it",
                            art.role, src.arch
                        )
                    });
                }
                tracing::warn!(role = art.role, error = %e, "skipped optional artifact (unavailable)");
                continue;
            }
        }

        // Verify unless verification was explicitly disabled (offline mode with
        // no checksum source and --insecure-skip-verify). A required artifact
        // that fails verification aborts the whole stage via `?`.
        if let Some(body) = &sum_body {
            crate::verify::verify_against_sumfile(&local, body)?;
        }

        staged.push(StagedArtifact {
            role: art.role,
            in_image_name: art.in_image_name,
            source_name: name,
            local_path: local,
        });
    }

    // Invariant: every required role must have made it into the staged set.
    let missing: Vec<&str> = ARTIFACTS
        .iter()
        .filter(|a| a.required && !staged.iter().any(|s| s.role == a.role))
        .map(|a| a.role)
        .collect();
    if !missing.is_empty() {
        bail!(
            "required artifacts missing after staging {version} ({}): {}",
            src.arch,
            missing.join(", ")
        );
    }

    if staged.is_empty() {
        bail!("no artifacts could be staged for {version} ({})", src.arch);
    }

    tracing::info!(
        version = %version,
        arch = %src.arch,
        count = staged.len(),
        "staged artifacts"
    );
    Ok(Staged {
        version,
        arch: src.arch.clone(),
        artifacts: staged,
    })
}

/// Fetch the checksum file used to verify staged artifacts.
///
/// Returns `Some(body)` to verify every staged artifact against, or `None` only
/// when verification was explicitly disabled with `--insecure-skip-verify`.
/// Online mode always pulls the mirror's `sha256sum.txt`. Offline mode uses the
/// cached copy (an online run leaves one behind); if none is present it fails
/// closed rather than building from unverified media, unless the operator opts
/// out explicitly.
fn stage_sumfile(src: &SourceArgs, version: &str, cache_dir: &Path) -> Result<Option<String>> {
    let sum_path = cache_dir.join("sha256sum.txt");
    match src.mode {
        Mode::Online => {
            if !exists_nonempty(&sum_path)? {
                let url = crate::mirror::artifact_url(version, &src.arch, "sha256sum.txt");
                crate::mirror::download(&url, &sum_path)?;
            }
            Ok(Some(fs::read_to_string(&sum_path)?))
        }
        Mode::Offline => {
            if exists_nonempty(&sum_path)? {
                Ok(Some(fs::read_to_string(&sum_path)?))
            } else if src.insecure_skip_verify {
                tracing::warn!(
                    "offline mode: no sha256sum.txt in cache and --insecure-skip-verify \
                     set; proceeding WITHOUT integrity verification"
                );
                Ok(None)
            } else {
                bail!(
                    "offline mode: no sha256sum.txt in cache ({}); cannot verify artifact \
                     integrity. Populate the cache with an online run first, or pass \
                     --insecure-skip-verify to build from unverified media (unsafe).",
                    sum_path.display()
                )
            }
        }
    }
}

fn fetch_one(src: &SourceArgs, version: &str, name: &str, local: &Path) -> Result<()> {
    match src.mode {
        Mode::Online => {
            let url = crate::mirror::artifact_url(version, &src.arch, name);
            crate::mirror::download(&url, local)
        }
        Mode::Offline => match src.registry.as_ref() {
            Some(reg) => crate::registry::pull_artifact(reg, version, &src.arch, name, local),
            None => bail!(
                "{name} not present in local cache and no --registry set \
                 (offline mode builds from the local cache / local rspacefs)"
            ),
        },
    }
}

fn exists_nonempty(path: &Path) -> Result<bool> {
    Ok(path.exists() && fs::metadata(path)?.len() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A unique, empty temp directory that removes itself on drop.
    struct TmpDir(PathBuf);

    impl TmpDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "compose_rspaced-test-{}-{}",
                std::process::id(),
                n
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TmpDir(dir)
        }
    }

    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn offline_args(cache: PathBuf, insecure_skip_verify: bool) -> SourceArgs {
        SourceArgs {
            version: Some("4.18.30".into()),
            series: "4.18".into(),
            arch: "x86_64".into(),
            mode: Mode::Offline,
            registry: None,
            cache,
            insecure_skip_verify,
        }
    }

    #[test]
    fn offline_without_checksum_fails_closed() {
        let tmp = TmpDir::new();
        let src = offline_args(tmp.0.clone(), false);
        // No sha256sum.txt in the cache dir -> must error, not skip.
        let err = stage_sumfile(&src, "4.18.30", &tmp.0).unwrap_err();
        assert!(
            err.to_string().contains("no sha256sum.txt"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn offline_insecure_skip_verify_returns_none() {
        let tmp = TmpDir::new();
        let src = offline_args(tmp.0.clone(), true);
        // Opt-out is honored: verification is disabled (None), not an error.
        let body = stage_sumfile(&src, "4.18.30", &tmp.0).unwrap();
        assert!(body.is_none());
    }

    #[test]
    fn offline_with_cached_checksum_returns_body() {
        let tmp = TmpDir::new();
        fs::write(tmp.0.join("sha256sum.txt"), "abc123  somefile\n").unwrap();
        let src = offline_args(tmp.0.clone(), false);
        let body = stage_sumfile(&src, "4.18.30", &tmp.0).unwrap();
        assert_eq!(body.as_deref(), Some("abc123  somefile\n"));
    }
}
