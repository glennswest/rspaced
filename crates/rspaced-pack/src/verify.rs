//! Verify a packed store against its recorded provenance.
//!
//! Provenance is rspaced's #1 feature, so the store must be independently
//! re-verifiable at rest: every preserved blob must still hash to its digest,
//! and every extracted layer must still produce its recorded verity root. This
//! is the same check the boot agent performs before trusting the content.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest as _, Sha256};

use crate::provenance::ImageProvenance;

/// Outcome of verifying a store.
#[derive(Default)]
pub struct VerifyReport {
    /// Image provenance docs checked.
    pub images: usize,
    /// Blobs whose sha256 matched their digest.
    pub blobs_ok: usize,
    /// Blobs missing or with a digest mismatch.
    pub blobs_fail: usize,
    /// Layers whose rebuilt verity root matched the recorded one.
    pub verity_ok: usize,
    /// Layers whose verity root mismatched or could not be rebuilt.
    pub verity_fail: usize,
    /// Human-readable failure descriptions.
    pub failures: Vec<String>,
}

impl VerifyReport {
    /// True if nothing failed.
    pub fn ok(&self) -> bool {
        self.blobs_fail == 0 && self.verity_fail == 0
    }
}

/// Verify every image provenance doc under `store_root/provenance/images/`:
/// each manifest/config/layer blob hashes to its digest, and each extracted
/// layer rebuilds to its recorded verity root.
pub fn verify_store(store_root: &Path) -> Result<VerifyReport> {
    let mut rep = VerifyReport::default();
    let prov_dir = store_root.join("provenance/images");
    let entries =
        fs::read_dir(&prov_dir).with_context(|| format!("reading {}", prov_dir.display()))?;

    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(&path)?;
        let prov: ImageProvenance = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing {}", path.display()))?;
        rep.images += 1;

        verify_blob(store_root, &prov.manifest_digest, &mut rep);
        verify_blob(store_root, &prov.config_digest, &mut rep);
        for l in &prov.layers {
            verify_blob(store_root, &l.compressed_digest, &mut rep);
            verify_verity(store_root, &l.dir, &l.verity_root, &mut rep);
        }
    }
    Ok(rep)
}

fn blob_path(store_root: &Path, digest: &str) -> PathBuf {
    let hex = digest.split_once(':').map(|(_, h)| h).unwrap_or(digest);
    store_root.join("blobs/sha256").join(hex)
}

fn verify_blob(store_root: &Path, digest: &str, rep: &mut VerifyReport) {
    let expected = digest.split_once(':').map(|(_, h)| h).unwrap_or(digest);
    match sha256_file(&blob_path(store_root, digest)) {
        Ok(actual) if actual.eq_ignore_ascii_case(expected) => rep.blobs_ok += 1,
        Ok(actual) => {
            rep.blobs_fail += 1;
            rep.failures
                .push(format!("blob {digest}: have {actual}, want {expected}"));
        }
        Err(e) => {
            rep.blobs_fail += 1;
            rep.failures.push(format!("blob {digest}: {e:#}"));
        }
    }
}

fn verify_verity(store_root: &Path, dir_rel: &str, expected: &str, rep: &mut VerifyReport) {
    let dir = store_root.join(dir_rel);
    match rspacefs_verity::MerkleTree::build_from_dir(&dir) {
        Ok((_t, m)) => {
            let root = hex::encode(m.root_hash);
            if root == expected {
                rep.verity_ok += 1;
            } else {
                rep.verity_fail += 1;
                rep.failures
                    .push(format!("verity {dir_rel}: have {root}, want {expected}"));
            }
        }
        Err(e) => {
            rep.verity_fail += 1;
            rep.failures.push(format!("verity {dir_rel}: {e}"));
        }
    }
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
