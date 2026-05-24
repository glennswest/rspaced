//! Stage the RHCOS artifact set into the local cache, honoring online vs
//! offline mode, and verify each file against the published checksums.

use anyhow::{bail, Result};
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
                tracing::warn!(role = art.role, error = %e, "skipped (unavailable)");
                continue;
            }
        }

        if !sum_body.is_empty() {
            crate::verify::verify_against_sumfile(&local, &sum_body)?;
        }

        staged.push(StagedArtifact {
            role: art.role,
            in_image_name: art.in_image_name,
            source_name: name,
            local_path: local,
        });
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

/// Fetch the checksum file. Online mode pulls it from the mirror; offline mode
/// has no checksum source yet, so verification is skipped (returns "").
fn stage_sumfile(src: &SourceArgs, version: &str, cache_dir: &Path) -> Result<String> {
    let sum_path = cache_dir.join("sha256sum.txt");
    match src.mode {
        Mode::Online => {
            if !exists_nonempty(&sum_path)? {
                let url = crate::mirror::artifact_url(version, &src.arch, "sha256sum.txt");
                crate::mirror::download(&url, &sum_path)?;
            }
            Ok(fs::read_to_string(&sum_path)?)
        }
        Mode::Offline => {
            if exists_nonempty(&sum_path)? {
                Ok(fs::read_to_string(&sum_path)?)
            } else {
                tracing::warn!("offline mode: no sha256sum.txt available, skipping verification");
                Ok(String::new())
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
