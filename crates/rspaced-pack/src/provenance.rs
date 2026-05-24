//! Provenance records — the hash chain for every packed artifact.
//!
//! Provenance is rspaced's #1 feature: every byte must be verifiable by hash
//! before, during, and after each transform. These records capture that chain
//! so the whole store is independently re-verifiable, and the build-time
//! verity roots are exactly what the boot agent checks.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Per-layer hash chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerProvenance {
    /// OCI layer digest of the *compressed* blob — verified on download and
    /// the key under which the blob is preserved in the store.
    pub compressed_digest: String,
    /// sha256 of the *uncompressed* tar — verified on extraction and
    /// cross-checked against the image config's `rootfs.diff_ids`.
    /// `None` if the config did not provide a matching diff_id.
    pub diff_id: Option<String>,
    /// Whether `diff_id` matched the image config's declared diff_id.
    pub diff_id_verified: bool,
    /// Hex Merkle root (`rspacefs-verity`) over the extracted layer tree —
    /// the at-rest anchor re-checked at boot.
    pub verity_root: String,
    /// Extracted layer directory, relative to the store root.
    pub dir: String,
    /// Files + symlinks extracted.
    pub entries: u64,
    /// OCI whiteout markers preserved.
    pub whiteouts: u64,
}

/// Hash chain for one packed image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageProvenance {
    /// Source image reference.
    pub image: String,
    /// Platform architecture resolved.
    pub arch: String,
    /// Platform OS resolved.
    pub os: String,
    /// The image index digest, if the reference resolved through one.
    pub index_digest: Option<String>,
    /// The image manifest digest (the per-image provenance anchor).
    pub manifest_digest: String,
    /// The image config blob digest.
    pub config_digest: String,
    /// Per-layer chains, in manifest (base-first) order.
    pub layers: Vec<LayerProvenance>,
}

impl ImageProvenance {
    /// Write to `<store_root>/provenance/images/<manifest-hex>.json`.
    pub fn write(&self, store_root: &Path) -> Result<()> {
        let dir = store_root.join("provenance/images");
        fs::create_dir_all(&dir)?;
        let hex = self.manifest_digest.split_once(':').map_or(
            self.manifest_digest.as_str(),
            |(_, h)| h,
        );
        let path = dir.join(format!("{hex}.json"));
        let json = serde_json::to_vec_pretty(self)?;
        fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}

/// One component image's provenance summary in a release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentProvenance {
    /// Component/tag name.
    pub name: String,
    /// Upstream image reference.
    pub image: String,
    /// Resolved manifest digest (anchor), or `None` if the component failed.
    pub manifest_digest: Option<String>,
    /// Error string if packing this component failed.
    pub error: Option<String>,
}

/// Hash chain for a whole release payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseProvenance {
    /// Release image reference.
    pub release_image: String,
    /// Platform architecture.
    pub arch: String,
    /// Platform OS.
    pub os: String,
    /// Release image manifest digest (the release's identity).
    pub release_manifest_digest: String,
    /// Every component image referenced by the release.
    pub components: Vec<ComponentProvenance>,
}

impl ReleaseProvenance {
    /// Write to `<store_root>/provenance/release-<manifest-hex>.json`.
    pub fn write(&self, store_root: &Path) -> Result<()> {
        let dir = store_root.join("provenance");
        fs::create_dir_all(&dir)?;
        let hex = self
            .release_manifest_digest
            .split_once(':')
            .map_or(self.release_manifest_digest.as_str(), |(_, h)| h);
        let path = dir.join(format!("release-{hex}.json"));
        let json = serde_json::to_vec_pretty(self)?;
        fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}
