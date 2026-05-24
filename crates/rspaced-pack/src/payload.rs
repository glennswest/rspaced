//! Pull and pack a whole OpenShift release payload.
//!
//! Pulls the release image, reads its `image-references`, then pulls and packs
//! every component image (kernel/machine-os, installer, cli, all operators)
//! into a single shared store. Because layers are content-addressed by digest
//! under one `store_root`, layers shared across components are extracted once.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use rspaced_oci::{Client, Digest, Reference};

use crate::layer::{pack_image, PackedImage};
use crate::provenance::{ComponentProvenance, ReleaseProvenance};
use crate::release::ImageReferences;

/// One packed component image from the release payload.
pub struct PackedComponent {
    /// Component/tag name (e.g. `machine-os-images`, `cli`).
    pub name: String,
    /// Upstream image reference it was pulled from.
    pub image: String,
    /// Manifest digest it resolved to.
    pub manifest_digest: Digest,
    /// Layer directories, in `LayerFS` lower order.
    pub layer_dirs: Vec<std::path::PathBuf>,
}

/// Result of packing an entire release.
pub struct PackedRelease {
    /// The packed release image itself.
    pub release: PackedImage,
    /// The parsed `image-references` payload.
    pub references: ImageReferences,
    /// Successfully packed component images.
    pub components: Vec<PackedComponent>,
    /// Components that failed: `(name, error)`.
    pub failures: Vec<(String, String)>,
    /// Release-level provenance (also written to the store).
    pub provenance: ReleaseProvenance,
}

/// Options for [`pack_release`].
pub struct PackOptions {
    /// Platform architecture to resolve (e.g. `amd64`).
    pub arch: String,
    /// Platform OS to resolve (e.g. `linux`).
    pub os: String,
    /// Cap the number of components attempted (for testing). `None` = all.
    pub limit: Option<usize>,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            arch: "amd64".into(),
            os: "linux".into(),
            limit: None,
        }
    }
}

/// Pull and pack the release image and its entire component payload into
/// `store_root`. Continues past individual component failures, collecting them
/// in [`PackedRelease::failures`]. Idempotent: already-packed layers are reused.
pub fn pack_release(
    client: &Client,
    release_ref: &Reference,
    store_root: &Path,
    opts: &PackOptions,
) -> Result<PackedRelease> {
    let release = pack_image(client, release_ref, &opts.arch, &opts.os, store_root)
        .context("packing release image")?;

    let references = crate::release::find_image_references(&release.layer_dirs)?
        .ok_or_else(|| anyhow!("release-manifests/image-references not found in release image"))?;

    let total = references.components.len();
    tracing::info!(components = total, "discovered release payload");

    let limit = opts.limit.unwrap_or(usize::MAX);
    let mut seen: HashSet<String> = HashSet::new();
    let mut components = Vec::new();
    let mut failures = Vec::new();
    let mut component_provs: Vec<ComponentProvenance> = Vec::new();

    for (i, c) in references.components.iter().take(limit).enumerate() {
        // Dedup: several tags can point at the same image digest.
        if !seen.insert(c.image.clone()) {
            continue;
        }
        let reference = match Reference::parse(&c.image) {
            Ok(r) => r,
            Err(e) => {
                let err = format!("bad image ref: {e:#}");
                failures.push((c.name.clone(), err.clone()));
                component_provs.push(ComponentProvenance {
                    name: c.name.clone(),
                    image: c.image.clone(),
                    manifest_digest: None,
                    error: Some(err),
                });
                continue;
            }
        };
        tracing::info!(progress = format!("{}/{}", i + 1, total), component = %c.name, "packing component");
        match pack_image(client, &reference, &opts.arch, &opts.os, store_root) {
            Ok(p) => {
                component_provs.push(ComponentProvenance {
                    name: c.name.clone(),
                    image: c.image.clone(),
                    manifest_digest: Some(p.manifest_digest.to_string()),
                    error: None,
                });
                components.push(PackedComponent {
                    name: c.name.clone(),
                    image: c.image.clone(),
                    manifest_digest: p.manifest_digest,
                    layer_dirs: p.layer_dirs,
                });
            }
            Err(e) => {
                let err = format!("{e:#}");
                tracing::warn!(component = %c.name, error = %err, "component pack failed");
                failures.push((c.name.clone(), err.clone()));
                component_provs.push(ComponentProvenance {
                    name: c.name.clone(),
                    image: c.image.clone(),
                    manifest_digest: None,
                    error: Some(err),
                });
            }
        }
    }

    let provenance = ReleaseProvenance {
        release_image: format!(
            "{}/{}:{}",
            release_ref.registry, release_ref.repository, release_ref.reference
        ),
        arch: opts.arch.clone(),
        os: opts.os.clone(),
        release_manifest_digest: release.manifest_digest.to_string(),
        components: component_provs,
    };
    provenance.write(store_root)?;

    tracing::info!(
        ok = components.len(),
        failed = failures.len(),
        "release payload packed"
    );
    Ok(PackedRelease {
        release,
        references,
        components,
        failures,
        provenance,
    })
}
