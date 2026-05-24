//! Extract OCI image layers into directories that stack as `LayerFS` lowers.

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use rspaced_oci::{Client, Digest, Reference};

/// Counts from extracting a single layer tar.
#[derive(Debug, Default, Clone, Copy)]
pub struct LayerStats {
    /// Files + symlinks unpacked (auto-created directories are not counted).
    pub entries: u64,
    /// OCI whiteout entries seen (`.wh.*`), preserved as-is for `LayerFS`.
    pub whiteouts: u64,
}

/// A pulled image, packed to disk.
pub struct PackedImage {
    /// Digest the image manifest was fetched by.
    pub manifest_digest: Digest,
    /// Layer directories in `LayerFS` lower order: index 0 is the topmost
    /// (highest-priority) layer. Pass straight to `LayerFS::new(upper, dirs)`.
    pub layer_dirs: Vec<PathBuf>,
}

/// Extract one (optionally gzipped) OCI layer tar at `src` into `dest`.
///
/// Whiteout entries (`.wh.<name>`, `.wh..wh..opq`) are written verbatim — they
/// are not applied here; `LayerFS` interprets them when the layers are stacked.
/// Path traversal outside `dest` is refused by the tar reader.
pub fn extract_layer(src: &Path, dest: &Path) -> Result<LayerStats> {
    fs::create_dir_all(dest)?;
    let file = File::open(src).with_context(|| format!("open layer blob {}", src.display()))?;
    let is_gzip = {
        let mut head = [0u8; 2];
        let n = File::open(src)?.read(&mut head)?;
        n == 2 && head == [0x1f, 0x8b]
    };

    let reader: Box<dyn Read> = if is_gzip {
        Box::new(GzDecoder::new(BufReader::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };

    // Use Archive::unpack: it sets directory permissions only after every
    // entry is written, so a mode-0555 dir in the image (e.g. /root) can't
    // block files unpacked into it. It also handles symlinks/hardlinks and
    // refuses path-traversal entries. Whiteout markers (`.wh.*`) extract as
    // ordinary files, which is exactly what LayerFS expects.
    let mut archive = tar::Archive::new(reader);
    archive.set_preserve_permissions(true);
    archive.set_overwrite(true);
    archive.unpack(dest).context("unpacking layer tar")?;

    let mut stats = LayerStats::default();
    count_tree(dest, &mut stats)?;
    Ok(stats)
}

/// Walk the extracted tree counting files/symlinks and whiteout markers.
/// Directories are descended into but not counted; symlinks are not followed.
fn count_tree(dir: &Path, stats: &mut LayerStats) -> Result<()> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for entry in rd.flatten() {
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
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
    Ok(())
}

/// Pull `reference` (resolving an image index for `arch`/`os` if needed) and
/// extract every layer under `layers_root/<layer-hex>/`, returning the layer
/// directories in `LayerFS` lower order.
///
/// Idempotent: a layer dir carrying the `.rspaced-extracted` marker is reused,
/// so re-runs skip already-packed layers (the offline-cache property).
pub fn pack_image(
    client: &Client,
    reference: &Reference,
    arch: &str,
    os: &str,
    layers_root: &Path,
) -> Result<PackedImage> {
    let (manifest, manifest_digest) = client.resolve_image(reference, arch, os)?;
    fs::create_dir_all(layers_root)?;
    let tmp = layers_root.join(".tmp");
    fs::create_dir_all(&tmp)?;

    let mut dirs = Vec::with_capacity(manifest.layers.len());
    for layer in &manifest.layers {
        let hex = layer.digest.hex();
        let dir = layers_root.join(hex);
        let marker = dir.join(".rspaced-extracted");

        if !marker.exists() {
            // A dir without the marker is a stale/interrupted extraction;
            // remove it so we extract cleanly (avoids read-only-dir leftovers).
            if dir.exists() {
                fs::remove_dir_all(&dir)
                    .with_context(|| format!("clearing stale layer dir {}", dir.display()))?;
            }
            let blob = tmp.join(format!("{hex}.blob"));
            {
                let mut out =
                    File::create(&blob).with_context(|| format!("create {}", blob.display()))?;
                client
                    .pull_blob(reference, &layer.digest, &mut out)
                    .with_context(|| format!("pulling layer {}", layer.digest.short_hex()))?;
            }
            fs::create_dir_all(&dir)?;
            let stats = extract_layer(&blob, &dir)
                .with_context(|| format!("extracting layer {}", layer.digest.short_hex()))?;
            File::create(&marker)?;
            let _ = fs::remove_file(&blob);
            tracing::info!(
                layer = layer.digest.short_hex(),
                entries = stats.entries,
                whiteouts = stats.whiteouts,
                "packed layer"
            );
        } else {
            tracing::debug!(layer = layer.digest.short_hex(), "layer already packed");
        }
        dirs.push(dir);
    }

    // Manifest order is base-first; LayerFS lowers are top-down (index 0 =
    // highest priority), so reverse to put the topmost layer first.
    dirs.reverse();
    let _ = fs::remove_dir(&tmp);
    Ok(PackedImage {
        manifest_digest,
        layer_dirs: dirs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

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
        append("etc/.wh.obsolete", b""); // whiteout marker
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
    fn extracts_plain_tar_without_gzip() {
        let tmp = std::env::temp_dir().join(format!("rspaced-pack-plain-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let blob = tmp.join("layer.tar");
        {
            let f = File::create(&blob).unwrap();
            let mut tar = tar::Builder::new(f);
            let data = b"x";
            let mut h = tar::Header::new_gnu();
            h.set_size(1);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, "a.txt", &data[..]).unwrap();
            tar.into_inner().unwrap().flush().unwrap();
        }
        let dest = tmp.join("out");
        let stats = extract_layer(&blob, &dest).unwrap();
        assert_eq!(stats.entries, 1);
        assert!(dest.join("a.txt").exists());
        let _ = fs::remove_dir_all(&tmp);
    }
}
