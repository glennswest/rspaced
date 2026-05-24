//! Upstream OpenShift mirror access: version discovery and downloads.

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;

const MIRROR_BASE: &str = "https://mirror.openshift.com/pub/openshift-v4";

/// Translate a kernel-arch name to the alias used in mirror paths.
pub fn mirror_arch(arch: &str) -> &str {
    match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

fn major_minor(version_or_series: &str) -> String {
    version_or_series
        .split('.')
        .take(2)
        .collect::<Vec<_>>()
        .join(".")
}

/// Directory URL holding the z-stream releases for a series.
fn series_url(series: &str, arch: &str) -> String {
    let march = mirror_arch(arch);
    let mver = major_minor(series);
    format!("{MIRROR_BASE}/{march}/dependencies/rhcos/{mver}/")
}

/// Full URL to a single artifact file for a specific version.
pub fn artifact_url(version: &str, arch: &str, file: &str) -> String {
    let march = mirror_arch(arch);
    let mver = major_minor(version);
    format!("{MIRROR_BASE}/{march}/dependencies/rhcos/{mver}/{version}/{file}")
}

/// Highest z-stream version published for the given series.
pub fn find_latest(series: &str, arch: &str) -> Result<String> {
    let url = series_url(series, arch);
    tracing::info!(%url, "querying mirror for RHCOS versions");

    let body = reqwest::blocking::get(&url)
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?
        .text()?;

    let re = regex::Regex::new(r#"href="(\d+\.\d+\.\d+)/""#).unwrap();
    let mut versions: Vec<(u64, u64, u64, String)> = re
        .captures_iter(&body)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .filter_map(|s| {
            let p: Vec<u64> = s.split('.').filter_map(|x| x.parse().ok()).collect();
            (p.len() == 3).then(|| (p[0], p[1], p[2], s))
        })
        .collect();

    versions.sort();
    versions
        .pop()
        .map(|(_, _, _, s)| s)
        .ok_or_else(|| anyhow!("no z-stream versions found under {url}"))
}

/// Download `url` to `dest` atomically (via a `.part` temp file).
pub fn download(url: &str, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    tracing::info!(%url, dest = %dest.display(), "downloading");

    let mut resp = reqwest::blocking::get(url)
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?;

    let tmp = dest.with_extension("part");
    {
        let mut out =
            fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        std::io::copy(&mut resp, &mut out)?;
        out.flush()?;
    }
    fs::rename(&tmp, dest)
        .with_context(|| format!("rename {} -> {}", tmp.display(), dest.display()))?;
    Ok(())
}
