//! Content-addressed OCI blob store.
//!
//! Preserves the original OCI artifacts — manifests, image indexes, configs,
//! and the *compressed* layer blobs — verbatim, keyed by digest under
//! `<root>/blobs/sha256/<hex>`. This is the faithful, digest-verifiable form
//! that OpenShift consumes (it pulls by digest), kept alongside the extracted
//! layer directories used for the rspacefs mount.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rspaced_oci::Digest;

/// A content-addressed blob store rooted at `<root>/blobs/sha256/`.
pub struct BlobStore {
    sha256_dir: PathBuf,
}

impl BlobStore {
    /// Open (creating if needed) the blob store under `store_root`.
    pub fn open(store_root: &Path) -> Result<Self> {
        let sha256_dir = store_root.join("blobs/sha256");
        fs::create_dir_all(&sha256_dir)
            .with_context(|| format!("creating blob store {}", sha256_dir.display()))?;
        Ok(Self { sha256_dir })
    }

    /// On-disk path for a digest's blob (whether or not it exists yet).
    pub fn path_for(&self, digest: &Digest) -> PathBuf {
        self.sha256_dir.join(digest.hex())
    }

    /// True if the blob is already present.
    pub fn has(&self, digest: &Digest) -> bool {
        self.path_for(digest).exists()
    }

    /// Store `bytes` under `expected` after verifying they hash to it.
    /// Idempotent: a no-op if already present (the name is the hash).
    pub fn put_bytes(&self, expected: &Digest, bytes: &[u8]) -> Result<()> {
        let actual = Digest::from_bytes(bytes);
        if &actual != expected {
            bail!(
                "blob digest mismatch: expected {}, got {}",
                expected,
                actual
            );
        }
        let path = self.path_for(expected);
        if path.exists() {
            return Ok(());
        }
        write_atomic(&path, bytes)
    }
}

/// Write `bytes` to `path` via a `.part` temp + rename.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("part");
    {
        let mut f = File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        f.write_all(bytes)?;
        f.flush()?;
    }
    fs::rename(&tmp, path).with_context(|| format!("rename into {}", path.display()))?;
    Ok(())
}
