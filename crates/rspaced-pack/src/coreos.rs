//! Locate and emit the CoreOS live ISO from a packed store.
//!
//! `machine-os-images` ships `coreos/coreos-<arch>.iso` (the RHCOS live ISO —
//! kernel + init + live rootfs, with podman) alongside `coreos-<arch>.iso.sha256`.
//! For the first bootc-assembly milestone we emit that verified ISO straight
//! from the packed store: the bootable "format", from content we already pulled
//! and provenance-checked. (rspacefs/storage customization layers on next.)

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest as _, Sha256};

/// The emitted CoreOS ISO.
pub struct CoreosIso {
    /// Where it was written.
    pub out: PathBuf,
    /// Source path inside the store.
    pub source: PathBuf,
    /// sha256 (verified against the `.sha256` sidecar if present).
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
}

/// Find `coreos/coreos-*.iso` under `store_root/extracted/*/`, verify it against
/// its `.sha256` sidecar (when present), and copy it to `out`.
pub fn extract_coreos_iso(store_root: &Path, out: &Path) -> Result<CoreosIso> {
    let iso = find_coreos_iso(store_root)?.ok_or_else(|| {
        anyhow!(
            "no coreos-*.iso found under {}/extracted",
            store_root.display()
        )
    })?;

    let sha256 = sha256_file(&iso)?;

    // Verify against the published sidecar if it's there (provenance).
    let sidecar = PathBuf::from(format!("{}.sha256", iso.display()));
    if sidecar.is_file() {
        let want = fs::read_to_string(&sidecar)?
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        if !want.is_empty() && !sha256.eq_ignore_ascii_case(&want) {
            bail!(
                "coreos ISO sha256 mismatch: have {sha256}, sidecar says {want} ({})",
                iso.display()
            );
        }
    }

    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&iso, out)
        .with_context(|| format!("copying {} -> {}", iso.display(), out.display()))?;
    let size = fs::metadata(out)?.len();

    Ok(CoreosIso {
        out: out.to_path_buf(),
        source: iso,
        sha256,
        size,
    })
}

fn find_coreos_iso(store_root: &Path) -> Result<Option<PathBuf>> {
    let extracted = store_root.join("extracted");
    let Ok(layers) = fs::read_dir(&extracted) else {
        bail!("no extracted/ dir in store {}", store_root.display());
    };
    for layer in layers.flatten() {
        let coreos = layer.path().join("coreos");
        if !coreos.is_dir() {
            continue;
        }
        if let Ok(files) = fs::read_dir(&coreos) {
            for f in files.flatten() {
                let name = f.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("coreos-") && name.ends_with(".iso") {
                    return Ok(Some(f.path()));
                }
            }
        }
    }
    Ok(None)
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}
