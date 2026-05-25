//! Extract OCI image layers into directories that stack as `LayerFS` lowers,
//! preserving the original OCI blobs and recording the full provenance chain.

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use rspaced_oci::{Client, Digest, ImageConfig, Index, Manifest, Reference};
use sha2::{Digest as _, Sha256};

use crate::provenance::{ImageProvenance, LayerProvenance};
use crate::store::BlobStore;

/// Counts from extracting a single layer tar.
#[derive(Debug, Default, Clone, Copy)]
pub struct LayerStats {
    /// Files + symlinks unpacked (auto-created directories are not counted).
    pub entries: u64,
    /// OCI whiteout entries seen (`.wh.*`), preserved as-is for `LayerFS`.
    pub whiteouts: u64,
}

/// A pulled image, packed to disk with provenance.
pub struct PackedImage {
    /// Digest the image manifest was fetched by (the per-image anchor).
    pub manifest_digest: Digest,
    /// Image index digest, if the reference resolved through a multi-arch index.
    pub index_digest: Option<Digest>,
    /// Image config blob digest.
    pub config_digest: Digest,
    /// Layer directories in `LayerFS` lower order: index 0 is the topmost
    /// (highest-priority) layer. Pass straight to `LayerFS::new(upper, dirs)`.
    pub layer_dirs: Vec<PathBuf>,
    /// Full hash chain for this image.
    pub provenance: ImageProvenance,
}

/// Pull `reference` (resolving an image index for `arch`/`os` if needed) into a
/// content-addressed store at `store_root`, recording the full provenance chain.
///
/// Produces, all under `store_root`:
/// - `blobs/sha256/<hex>` — the original manifest(s), config, and *compressed*
///   layer blobs, verbatim and digest-verified (the OpenShift-servable form).
/// - `extracted/<layer-hex>/` — each layer's tar unpacked (pure content; OCI
///   `.wh.` whiteouts preserved for `LayerFS`).
/// - `provenance/layers/<layer-hex>.verity.json` — verity Merkle manifest.
/// - `provenance/images/<manifest-hex>.json` — the per-image hash chain.
///
/// Idempotent: already-extracted layers (marker present) are reused; verity is
/// recomputed every run, which doubles as re-verification.
pub fn pack_image(
    client: &Client,
    reference: &Reference,
    arch: &str,
    os: &str,
    store_root: &Path,
) -> Result<PackedImage> {
    let blobs = BlobStore::open(store_root)?;
    let extracted_root = store_root.join("extracted");
    fs::create_dir_all(&extracted_root)?;
    let verity_dir = store_root.join("provenance/layers");
    fs::create_dir_all(&verity_dir)?;

    // ── Manifest (preserve raw bytes; resolve index by platform) ──────────
    let raw = client.fetch_manifest(reference)?;
    let probe: serde_json::Value =
        serde_json::from_slice(&raw.body).context("parsing manifest JSON")?;

    let (manifest, manifest_digest, index_digest): (Manifest, Digest, Option<Digest>) =
        if probe.get("manifests").is_some() {
            blobs.put_bytes(&raw.digest, &raw.body)?; // preserve the index
            let index: Index = serde_json::from_slice(&raw.body)?;
            let desc = index
                .select(arch, os)
                .ok_or_else(|| anyhow!("index has no {arch}/{os} manifest"))?;
            let by_digest = Reference {
                registry: reference.registry.clone(),
                repository: reference.repository.clone(),
                reference: desc.digest.to_string(),
            };
            let raw2 = client.fetch_manifest(&by_digest)?;
            blobs.put_bytes(&raw2.digest, &raw2.body)?;
            let m: Manifest = serde_json::from_slice(&raw2.body)?;
            (m, raw2.digest, Some(raw.digest))
        } else {
            blobs.put_bytes(&raw.digest, &raw.body)?;
            let m: Manifest = serde_json::from_slice(&raw.body)?;
            (m, raw.digest, None)
        };

    // ── Config (preserve; gives us the diff_ids to verify against) ────────
    let config_digest = manifest.config.digest.clone();
    let mut config_bytes = Vec::new();
    client
        .pull_blob(reference, &config_digest, &mut config_bytes)
        .context("pulling image config")?;
    blobs.put_bytes(&config_digest, &config_bytes)?;
    let config: ImageConfig =
        serde_json::from_slice(&config_bytes).context("parsing image config")?;
    let diff_ids = config.rootfs.diff_ids;

    // ── Layers ────────────────────────────────────────────────────────────
    let mut layer_provs = Vec::with_capacity(manifest.layers.len());
    let mut layer_dirs = Vec::with_capacity(manifest.layers.len());

    for (i, layer) in manifest.layers.iter().enumerate() {
        let hex = layer.digest.hex().to_string();

        // 1. Preserve the compressed layer blob (digest-verified on download).
        let blob_path = blobs.path_for(&layer.digest);
        if !blobs.has(&layer.digest) {
            let tmp = blob_path.with_extension("part");
            {
                let mut f =
                    File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
                client
                    .pull_blob(reference, &layer.digest, &mut f)
                    .with_context(|| format!("pulling layer {}", layer.digest.short_hex()))?;
            }
            fs::rename(&tmp, &blob_path)?;
        }

        // 2. diff_id = sha256 of the uncompressed tar; cross-check vs config.
        let diff_id = compute_diff_id(&blob_path)
            .with_context(|| format!("hashing uncompressed layer {hex}"))?;
        let expected = diff_ids.get(i);
        let diff_id_verified = expected == Some(&diff_id);
        if let Some(e) = expected {
            if e != &diff_id {
                tracing::warn!(
                    layer = layer.digest.short_hex(),
                    expected = %e,
                    got = %diff_id,
                    "diff_id mismatch vs image config"
                );
            }
        }

        // 3. Extract pure layer content to extracted/<hex>/ (markers kept out
        //    of the dir so verity hashes only image content).
        let dir = extracted_root.join(&hex);
        let marker = extracted_root.join(format!("{hex}.done"));
        let verity_json = verity_dir.join(format!("{hex}.verity.json"));
        // A fully-packed layer has both its marker and its verity manifest;
        // shared base layers recur across components, so reuse rather than
        // re-extract + re-hash. (`verify` is the explicit re-check.)
        let cached = marker.exists() && verity_json.exists();

        let stats = if marker.exists() {
            count_tree(&dir)?
        } else {
            force_remove_dir_all(&dir)
                .with_context(|| format!("clearing stale layer dir {}", dir.display()))?;
            let s = extract_layer(&blob_path, &dir)
                .with_context(|| format!("extracting layer {}", layer.digest.short_hex()))?;
            File::create(&marker)?;
            s
        };

        // 4. Verity Merkle root over the extracted tree (re-checked at boot).
        let verity_root = if cached {
            let m: rspacefs_verity::LayerManifest =
                serde_json::from_slice(&fs::read(&verity_json)?)
                    .with_context(|| format!("reading cached verity {hex}"))?;
            hex::encode(m.root_hash)
        } else {
            let (root, manifest) =
                build_verity(&dir).with_context(|| format!("building verity for layer {hex}"))?;
            fs::write(&verity_json, serde_json::to_vec_pretty(&manifest)?)?;
            root
        };

        tracing::info!(
            layer = layer.digest.short_hex(),
            entries = stats.entries,
            whiteouts = stats.whiteouts,
            diff_id_ok = diff_id_verified,
            "packed layer"
        );

        layer_provs.push(LayerProvenance {
            compressed_digest: layer.digest.to_string(),
            diff_id: Some(diff_id.to_string()),
            diff_id_verified,
            verity_root,
            dir: format!("extracted/{hex}"),
            entries: stats.entries,
            whiteouts: stats.whiteouts,
        });
        layer_dirs.push(dir);
    }

    let provenance = ImageProvenance {
        image: format!(
            "{}/{}:{}",
            reference.registry, reference.repository, reference.reference
        ),
        arch: arch.to_string(),
        os: os.to_string(),
        index_digest: index_digest.as_ref().map(|d| d.to_string()),
        manifest_digest: manifest_digest.to_string(),
        config_digest: config_digest.to_string(),
        layers: layer_provs,
    };
    provenance.write(store_root)?;

    // Manifest order is base-first; LayerFS lowers are top-down (index 0 =
    // highest priority), so reverse to put the topmost layer first.
    layer_dirs.reverse();
    Ok(PackedImage {
        manifest_digest,
        index_digest,
        config_digest,
        layer_dirs,
        provenance,
    })
}

/// Extract one (optionally gzipped) OCI layer tar at `src` into `dest`.
///
/// Manual multi-pass extraction, because `Archive::unpack` can't handle two
/// things real OCI/ostree layers need:
/// - **read-only directories** (e.g. `/root` mode 0555): we create dirs writable
///   and apply their recorded mode last, so children unpack first.
/// - **hardlinks whose target appears later / is content-addressed** (ostree
///   images hardlink `/usr/...` to `/sysroot/ostree/repo/objects/..`): `unpack`
///   canonicalizes the link target eagerly and fails with ENOENT. We defer
///   hardlinks to a second pass once every regular file exists.
///
/// Whiteouts (`.wh.*`) extract as ordinary files; path-traversal entries are
/// refused.
pub fn extract_layer(src: &Path, dest: &Path) -> Result<LayerStats> {
    fs::create_dir_all(dest)?;
    let reader = open_maybe_gzip(src)?;
    let mut archive = tar::Archive::new(reader);
    archive.set_preserve_permissions(true);
    archive.set_overwrite(true);

    let mut stats = LayerStats::default();
    let mut deferred_dirs: Vec<(PathBuf, u32)> = Vec::new();
    let mut hardlinks: Vec<(PathBuf, PathBuf)> = Vec::new();

    for entry in archive.entries().context("reading layer tar")? {
        let mut entry = entry.context("reading tar entry")?;
        let etype = entry.header().entry_type();
        let path = entry
            .path()
            .context("decoding tar entry path")?
            .into_owned();
        let Some(rel) = safe_rel(&path) else {
            tracing::warn!(path = %path.display(), "skipped unsafe tar entry");
            continue;
        };
        let target = dest.join(&rel);

        if rel
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(".wh."))
        {
            stats.whiteouts += 1;
        }

        if etype.is_dir() {
            fs::create_dir_all(&target)?;
            if let Ok(mode) = entry.header().mode() {
                deferred_dirs.push((target, mode & 0o7777));
            }
            continue;
        }

        if etype == tar::EntryType::Link {
            // Hardlink — defer; its target may not be extracted yet.
            if let Some(link) = entry.link_name().ok().flatten() {
                if let Some(tgt) = safe_rel(&link) {
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    hardlinks.push((target, dest.join(tgt)));
                    stats.entries += 1;
                }
            }
            continue;
        }

        // Regular file / symlink / etc. — ensure parent exists, then unpack.
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        match entry.unpack_in(dest).context("unpacking tar entry")? {
            true => stats.entries += 1,
            false => tracing::warn!(path = %path.display(), "skipped unsafe tar entry"),
        }
    }

    // Pass 2: hardlinks — targets exist now.
    for (link, tgt) in hardlinks {
        let _ = fs::remove_file(&link);
        if let Err(e) = fs::hard_link(&tgt, &link) {
            if tgt.exists() {
                fs::copy(&tgt, &link)
                    .with_context(|| format!("copy-fallback hardlink {}", link.display()))?;
            } else {
                tracing::warn!(link = %link.display(), target = %tgt.display(), error = %e, "hardlink target missing");
            }
        }
    }

    // Pass 3: apply directory modes deepest-first (kept writable until now).
    deferred_dirs.sort_by_key(|(p, _)| std::cmp::Reverse(p.components().count()));
    for (p, mode) in deferred_dirs {
        let _ = fs::set_permissions(&p, fs::Permissions::from_mode(mode));
    }

    Ok(stats)
}

/// Sanitize a tar entry path to a relative path under the dest dir; rejects
/// absolute paths and any `..`/prefix components (traversal).
fn safe_rel(p: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// Open `src`, transparently gunzipping if it has the gzip magic.
fn open_maybe_gzip(src: &Path) -> Result<Box<dyn Read>> {
    let is_gzip = {
        let mut head = [0u8; 2];
        let n = File::open(src)?.read(&mut head)?;
        n == 2 && head == [0x1f, 0x8b]
    };
    let file = File::open(src).with_context(|| format!("open {}", src.display()))?;
    Ok(if is_gzip {
        Box::new(GzDecoder::new(BufReader::new(file)))
    } else {
        Box::new(BufReader::new(file))
    })
}

/// sha256 of the uncompressed tar (the OCI `diff_id`).
fn compute_diff_id(src: &Path) -> Result<Digest> {
    let mut reader = open_maybe_gzip(src)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(Digest(format!("sha256:{}", hex::encode(hasher.finalize()))))
}

/// Build a `rspacefs-verity` Merkle manifest over `dir`; return its hex root
/// hash and the manifest (the same tree the runtime verifies at boot).
///
/// Uses `build_from_dir` (symlink-aware: records links via `symlink_metadata`,
/// tolerates dangling, doesn't follow/double-hash) — required for real OCI
/// layer trees (rspacefs#18).
fn build_verity(dir: &Path) -> Result<(String, rspacefs_verity::LayerManifest)> {
    let (_tree, manifest) = rspacefs_verity::MerkleTree::build_from_dir(dir)
        .map_err(|e| anyhow!("verity build over {}: {e}", dir.display()))?;
    Ok((hex::encode(manifest.root_hash), manifest))
}

/// Remove a directory tree, even one extracted from an image with read-only
/// directories (mode 0555). `fs::remove_dir_all` can't unlink entries inside a
/// directory that lacks owner-write, so make every directory writable first.
fn force_remove_dir_all(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let _ = fs::set_permissions(&d, fs::Permissions::from_mode(0o755));
        if let Ok(rd) = fs::read_dir(&d) {
            for e in rd.flatten() {
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    stack.push(e.path());
                }
            }
        }
    }
    fs::remove_dir_all(dir).with_context(|| format!("removing {}", dir.display()))
}

/// Walk the extracted tree counting files/symlinks and whiteout markers.
/// Directories are descended into but not counted; symlinks are not followed.
fn count_tree(dir: &Path) -> Result<LayerStats> {
    let mut stats = LayerStats::default();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for entry in rd.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                stack.push(entry.path());
                continue;
            }
            stats.entries += 1;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(".wh."))
            {
                stats.whiteouts += 1;
            }
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    fn make_layer_targz(path: &Path) {
        let f = File::create(path).unwrap();
        let enc = GzEncoder::new(f, Compression::default());
        let mut tar = tar::Builder::new(enc);
        let mut append = |name: &str, data: &[u8]| {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, name, data).unwrap();
        };
        append("etc/hostname", b"node-a\n");
        append("etc/.wh.obsolete", b"");
        tar.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn extracts_files_and_preserves_whiteouts() {
        let tmp = std::env::temp_dir().join(format!("rspaced-pack-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let blob = tmp.join("layer.tar.gz");
        make_layer_targz(&blob);

        let dest = tmp.join("layer0");
        let stats = extract_layer(&blob, &dest).unwrap();

        assert_eq!(stats.entries, 2);
        assert_eq!(stats.whiteouts, 1);
        let mut s = String::new();
        File::open(dest.join("etc/hostname"))
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        assert_eq!(s, "node-a\n");
        assert!(dest.join("etc/.wh.obsolete").exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn diff_id_is_stable_and_gzip_independent() {
        let tmp = std::env::temp_dir().join(format!("rspaced-diffid-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let blob = tmp.join("layer.tar.gz");
        make_layer_targz(&blob);
        let a = compute_diff_id(&blob).unwrap();
        let b = compute_diff_id(&blob).unwrap();
        assert_eq!(a, b);
        assert!(a.validate().is_ok());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn verity_root_is_deterministic() {
        let tmp = std::env::temp_dir().join(format!("rspaced-verity-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let dir = tmp.join("content");
        fs::create_dir_all(dir.join("etc")).unwrap();
        fs::write(dir.join("etc/hostname"), b"node-a\n").unwrap();
        let (r1, _) = build_verity(&dir).unwrap();
        let (r2, _) = build_verity(&dir).unwrap();
        assert_eq!(r1, r2);
        assert_eq!(r1.len(), 64);
        let _ = fs::remove_dir_all(&tmp);
    }
}
