//! Locate and emit the CoreOS live ISO from a packed store.
//!
//! `machine-os-images` ships `coreos/coreos-<arch>.iso` (the RHCOS live ISO —
//! kernel + init + live rootfs, with podman) alongside `coreos-<arch>.iso.sha256`.
//! For the first bootc-assembly milestone we emit that verified ISO straight
//! from the packed store: the bootable "format", from content we already pulled
//! and provenance-checked. (rspacefs/storage customization layers on next.)
//!
//! A store is content-addressed and shared, so it can legitimately hold several
//! releases and several architectures at once. Selection is therefore anchored
//! in provenance rather than in a directory scan: pick the matching
//! `provenance/release-*.json`, follow its `machine-os-images` component to
//! that image's recorded layer dirs, and take the ISO from there. An ambiguous
//! or unmatched selection is an error — this never picks arbitrarily, because
//! emitting an unidentified ISO is exactly the failure mode worth preventing.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest as _, Sha256};

use crate::provenance::{ImageProvenance, ReleaseProvenance};

/// Selects which release's ISO to emit from a store that may hold several.
///
/// An empty selector is valid and means "there must be exactly one candidate".
#[derive(Debug, Default, Clone)]
pub struct IsoSelector {
    /// Match releases whose image reference contains this substring (e.g.
    /// `4.18.30`), or whose release manifest digest starts with it.
    pub release: Option<String>,
    /// Match releases of this architecture. Accepts either the OCI platform
    /// name (`amd64`) or the kernel/CoreOS name (`x86_64`).
    pub arch: Option<String>,
}

/// The emitted CoreOS ISO.
#[derive(Debug, Clone)]
pub struct CoreosIso {
    /// Where it was written.
    pub out: PathBuf,
    /// Source path inside the store.
    pub source: PathBuf,
    /// sha256 (verified against the `.sha256` sidecar if present).
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
    /// The release this ISO was resolved from, when the store recorded one.
    pub release: Option<String>,
}

/// One ISO the store could emit, with the release it came from.
struct Candidate {
    iso: PathBuf,
    release: Option<String>,
}

/// Resolve the CoreOS ISO selected by `sel`, verify it against its `.sha256`
/// sidecar (when present), and copy it to `out`.
///
/// Errors if the selection matches no ISO or more than one; narrow with
/// [`IsoSelector::release`] / [`IsoSelector::arch`] in that case.
pub fn extract_coreos_iso(store_root: &Path, out: &Path, sel: &IsoSelector) -> Result<CoreosIso> {
    let mut candidates = find_candidates(store_root, sel)?;

    if candidates.is_empty() {
        return Err(no_candidates_error(store_root, sel));
    }
    if candidates.len() > 1 {
        let list = candidates
            .iter()
            .map(|c| {
                format!(
                    "\n  {} [{}]",
                    c.iso.display(),
                    c.release.as_deref().unwrap_or("unknown release")
                )
            })
            .collect::<String>();
        bail!(
            "ambiguous ISO selection in {}: {} candidates match. \
             Narrow with --release <version> and/or --arch <arch>:{}",
            store_root.display(),
            candidates.len(),
            list
        );
    }

    let chosen = candidates.remove(0);
    let iso = chosen.iso;
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
        release: chosen.release,
    })
}

/// Collect every ISO in the store matching `sel`, deduplicated by path.
///
/// Prefers the provenance-anchored path (release → `machine-os-images` → that
/// image's layers). Falls back to scanning `extracted/` only when the store has
/// no release provenance at all (e.g. images packed individually).
fn find_candidates(store_root: &Path, sel: &IsoSelector) -> Result<Vec<Candidate>> {
    let releases = load_releases(store_root)?;
    let mut found: Vec<Candidate> = Vec::new();

    if releases.is_empty() {
        for iso in scan_extracted(store_root)? {
            if arch_matches_filename(&iso, sel.arch.as_deref()) {
                found.push(Candidate { iso, release: None });
            }
        }
    } else {
        for rel in &releases {
            if !release_matches(rel, sel) {
                continue;
            }
            let label = format!("{} ({})", rel.release_image, rel.arch);
            for iso in isos_for_release(store_root, rel)? {
                found.push(Candidate {
                    iso,
                    release: Some(label.clone()),
                });
            }
        }
    }

    // Several releases can share one machine-os image, yielding the same path.
    found.sort_by(|a, b| a.iso.cmp(&b.iso));
    found.dedup_by(|a, b| a.iso == b.iso);
    Ok(found)
}

/// Build a diagnostic for "nothing matched", listing what the store does hold.
fn no_candidates_error(store_root: &Path, sel: &IsoSelector) -> anyhow::Error {
    let releases = load_releases(store_root).unwrap_or_default();
    if releases.is_empty() {
        return anyhow!(
            "no coreos-*.iso found under {}/extracted (store has no release provenance either; \
             populate it with `compose_rspaced release`)",
            store_root.display()
        );
    }
    let available = releases
        .iter()
        .map(|r| format!("\n  {} ({})", r.release_image, r.arch))
        .collect::<String>();
    anyhow!(
        "no coreos ISO in {} matched release={:?} arch={:?}. Releases in this store:{}",
        store_root.display(),
        sel.release.as_deref().unwrap_or("<any>"),
        sel.arch.as_deref().unwrap_or("<any>"),
        available
    )
}

/// Read every `provenance/release-*.json` in the store.
fn load_releases(store_root: &Path) -> Result<Vec<ReleaseProvenance>> {
    let dir = store_root.join("provenance");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("release-") || !name.ends_with(".json") {
            continue;
        }
        let body = fs::read(e.path()).with_context(|| format!("reading {}", e.path().display()))?;
        let rel: ReleaseProvenance = serde_json::from_slice(&body)
            .with_context(|| format!("parsing {}", e.path().display()))?;
        out.push(rel);
    }
    // Stable order so errors and selection are deterministic.
    out.sort_by(|a, b| a.release_image.cmp(&b.release_image));
    Ok(out)
}

/// Does this release satisfy the selector?
fn release_matches(rel: &ReleaseProvenance, sel: &IsoSelector) -> bool {
    if let Some(want) = &sel.arch {
        if oci_arch(want) != oci_arch(&rel.arch) {
            return false;
        }
    }
    if let Some(want) = &sel.release {
        let hex = digest_hex(&rel.release_manifest_digest);
        if !rel.release_image.contains(want.as_str()) && !hex.starts_with(want.as_str()) {
            return false;
        }
    }
    true
}

/// ISOs reachable from this release's machine-os component(s).
fn isos_for_release(store_root: &Path, rel: &ReleaseProvenance) -> Result<Vec<PathBuf>> {
    let want = format!("coreos-{}.iso", coreos_arch(&rel.arch));
    let mut found = Vec::new();

    for c in &rel.components {
        if !c.name.contains("machine-os") {
            continue;
        }
        let Some(digest) = &c.manifest_digest else {
            continue;
        };
        let Some(prov) = load_image_provenance(store_root, digest)? else {
            continue;
        };
        // Layers are recorded base-first; the topmost occurrence wins in
        // LayerFS order, so search from the top down.
        for layer in prov.layers.iter().rev() {
            let p = store_root.join(&layer.dir).join("coreos").join(&want);
            if p.is_file() {
                found.push(p);
                break;
            }
        }
    }
    Ok(found)
}

/// Load `provenance/images/<manifest-hex>.json`, if it exists.
fn load_image_provenance(
    store_root: &Path,
    manifest_digest: &str,
) -> Result<Option<ImageProvenance>> {
    let path = store_root
        .join("provenance/images")
        .join(format!("{}.json", digest_hex(manifest_digest)));
    if !path.is_file() {
        return Ok(None);
    }
    let body = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let prov: ImageProvenance =
        serde_json::from_slice(&body).with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(prov))
}

/// Every `extracted/*/coreos/coreos-*.iso` in the store.
fn scan_extracted(store_root: &Path) -> Result<Vec<PathBuf>> {
    let extracted = store_root.join("extracted");
    let Ok(layers) = fs::read_dir(&extracted) else {
        bail!("no extracted/ dir in store {}", store_root.display());
    };
    let mut found = Vec::new();
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
                    found.push(f.path());
                }
            }
        }
    }
    Ok(found)
}

/// Filename-level arch filter for the provenance-less fallback path.
fn arch_matches_filename(iso: &Path, arch: Option<&str>) -> bool {
    let Some(arch) = arch else {
        return true;
    };
    let want = format!("coreos-{}.iso", coreos_arch(oci_arch(arch)));
    iso.file_name().is_some_and(|n| n == want.as_str())
}

/// Hex portion of an `algo:hex` digest (or the whole string if unprefixed).
fn digest_hex(digest: &str) -> &str {
    digest.split_once(':').map_or(digest, |(_, h)| h)
}

/// CoreOS/kernel arch name for an OCI platform arch.
fn coreos_arch(arch: &str) -> &str {
    match arch {
        "amd64" => "x86_64",
        "arm64" => "aarch64",
        other => other,
    }
}

/// Normalize either spelling to the OCI platform name, so `x86_64` and `amd64`
/// select the same release.
fn oci_arch(arch: &str) -> &str {
    match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::{ComponentProvenance, LayerProvenance};
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    struct TmpDir(PathBuf);

    impl TmpDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("rspaced-coreos-{}-{}", std::process::id(), n));
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

    /// Write a release + its machine-os image provenance + the ISO itself.
    fn seed_release(store: &Path, version: &str, arch: &str, layer_hex: &str, body: &[u8]) {
        let kernel_arch = coreos_arch(arch);
        let layer_dir = store.join("extracted").join(layer_hex).join("coreos");
        fs::create_dir_all(&layer_dir).unwrap();
        fs::write(layer_dir.join(format!("coreos-{kernel_arch}.iso")), body).unwrap();

        // Digests must be unique per (version, arch): provenance files are keyed
        // by digest hex, and real per-arch release images differ too.
        let manifest_digest = format!("sha256:img{version}-{arch}");
        ImageProvenance {
            image: format!("machine-os-images:{version}"),
            arch: arch.into(),
            os: "linux".into(),
            index_digest: None,
            manifest_digest: manifest_digest.clone(),
            config_digest: "sha256:cfg".into(),
            layers: vec![LayerProvenance {
                compressed_digest: format!("sha256:{layer_hex}"),
                diff_id: None,
                diff_id_verified: false,
                verity_root: "deadbeef".into(),
                dir: format!("extracted/{layer_hex}"),
                entries: 1,
                whiteouts: 0,
            }],
        }
        .write(store)
        .unwrap();

        ReleaseProvenance {
            release_image: format!(
                "quay.io/openshift-release-dev/ocp-release:{version}-{kernel_arch}"
            ),
            arch: arch.into(),
            os: "linux".into(),
            release_manifest_digest: format!("sha256:rel{version}-{arch}"),
            components: vec![ComponentProvenance {
                name: "machine-os-images".into(),
                image: format!("machine-os-images:{version}"),
                manifest_digest: Some(manifest_digest),
                error: None,
            }],
        }
        .write(store)
        .unwrap();
    }

    #[test]
    fn single_release_resolves_through_provenance() {
        let tmp = TmpDir::new();
        let store = tmp.0.join("store");
        seed_release(&store, "4.18.30", "amd64", "aaa111", b"iso-bytes");

        let out = tmp.0.join("out.iso");
        let iso = extract_coreos_iso(&store, &out, &IsoSelector::default()).unwrap();

        assert_eq!(fs::read(&iso.out).unwrap(), b"iso-bytes");
        assert!(iso.release.unwrap().contains("4.18.30"));
        assert!(iso.source.to_string_lossy().contains("aaa111"));
    }

    #[test]
    fn multiple_releases_are_ambiguous_without_a_selector() {
        let tmp = TmpDir::new();
        let store = tmp.0.join("store");
        seed_release(&store, "4.18.30", "amd64", "aaa111", b"old");
        seed_release(&store, "4.19.02", "amd64", "bbb222", b"new");

        let out = tmp.0.join("out.iso");
        let err = extract_coreos_iso(&store, &out, &IsoSelector::default()).unwrap_err();
        assert!(
            err.to_string().contains("ambiguous"),
            "unexpected error: {err}"
        );
        // Nothing should have been emitted on an ambiguous selection.
        assert!(!out.exists());
    }

    #[test]
    fn release_selector_disambiguates() {
        let tmp = TmpDir::new();
        let store = tmp.0.join("store");
        seed_release(&store, "4.18.30", "amd64", "aaa111", b"old");
        seed_release(&store, "4.19.02", "amd64", "bbb222", b"new");

        let out = tmp.0.join("out.iso");
        let sel = IsoSelector {
            release: Some("4.19.02".into()),
            arch: None,
        };
        let iso = extract_coreos_iso(&store, &out, &sel).unwrap();
        assert_eq!(fs::read(&iso.out).unwrap(), b"new");
    }

    #[test]
    fn arch_selector_disambiguates_and_accepts_either_spelling() {
        let tmp = TmpDir::new();
        let store = tmp.0.join("store");
        seed_release(&store, "4.18.30", "amd64", "aaa111", b"intel");
        seed_release(&store, "4.18.30", "arm64", "bbb222", b"arm");

        let out = tmp.0.join("out.iso");
        // Kernel-arch spelling must select the amd64 release.
        let sel = IsoSelector {
            release: None,
            arch: Some("x86_64".into()),
        };
        let iso = extract_coreos_iso(&store, &out, &sel).unwrap();
        assert_eq!(fs::read(&iso.out).unwrap(), b"intel");

        // And the OCI spelling selects the same one.
        let out2 = tmp.0.join("out2.iso");
        let sel2 = IsoSelector {
            release: None,
            arch: Some("amd64".into()),
        };
        let iso2 = extract_coreos_iso(&store, &out2, &sel2).unwrap();
        assert_eq!(fs::read(&iso2.out).unwrap(), b"intel");
    }

    #[test]
    fn unmatched_selection_lists_available_releases() {
        let tmp = TmpDir::new();
        let store = tmp.0.join("store");
        seed_release(&store, "4.18.30", "amd64", "aaa111", b"iso");

        let out = tmp.0.join("out.iso");
        let sel = IsoSelector {
            release: Some("9.99.99".into()),
            arch: None,
        };
        let err = extract_coreos_iso(&store, &out, &sel).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no coreos ISO"), "unexpected error: {msg}");
        assert!(
            msg.contains("4.18.30"),
            "should list what is available: {msg}"
        );
    }

    #[test]
    fn sidecar_mismatch_is_rejected() {
        let tmp = TmpDir::new();
        let store = tmp.0.join("store");
        seed_release(&store, "4.18.30", "amd64", "aaa111", b"iso-bytes");
        // Sidecar claims a digest the ISO does not have.
        fs::write(
            store.join("extracted/aaa111/coreos/coreos-x86_64.iso.sha256"),
            format!("{}  coreos-x86_64.iso\n", "0".repeat(64)),
        )
        .unwrap();

        let out = tmp.0.join("out.iso");
        let err = extract_coreos_iso(&store, &out, &IsoSelector::default()).unwrap_err();
        assert!(
            err.to_string().contains("sha256 mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn falls_back_to_scan_when_store_has_no_release_provenance() {
        let tmp = TmpDir::new();
        let store = tmp.0.join("store");
        let dir = store.join("extracted/ccc333/coreos");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("coreos-x86_64.iso"), b"scanned").unwrap();

        let out = tmp.0.join("out.iso");
        let iso = extract_coreos_iso(&store, &out, &IsoSelector::default()).unwrap();
        assert_eq!(fs::read(&iso.out).unwrap(), b"scanned");
        assert!(iso.release.is_none());
    }

    #[test]
    fn arch_spellings_normalize() {
        assert_eq!(coreos_arch("amd64"), "x86_64");
        assert_eq!(coreos_arch("arm64"), "aarch64");
        assert_eq!(coreos_arch("riscv64"), "riscv64");
        assert_eq!(oci_arch("x86_64"), "amd64");
        assert_eq!(oci_arch("amd64"), "amd64");
        assert_eq!(digest_hex("sha256:abc"), "abc");
        assert_eq!(digest_hex("abc"), "abc");
    }
}
