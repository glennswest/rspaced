//! OpenShift release-payload discovery.
//!
//! An `ocp-release` image carries `release-manifests/image-references` — an
//! OpenShift ImageStream whose tags map a component name to the full image
//! reference (in the Red Hat registry) for that component. Reading it tells us
//! every image the release is built from: kernel/machine-os, installer, cli,
//! and all the operator payloads.
//!
//! Ported from fastregistry `internal/releases/extractor.go`
//! (`imageStream` / `findImageReferences`).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// One component → image reference mapping from `image-references`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentRef {
    /// Component/tag name, e.g. `machine-os-images`, `installer`, `cli`.
    pub name: String,
    /// Full upstream image reference, e.g.
    /// `quay.io/openshift-release-dev/ocp-v4.0-art-dev@sha256:…`.
    pub image: String,
}

/// The parsed `image-references` payload.
#[derive(Debug, Clone, Default)]
pub struct ImageReferences {
    /// All component references, in file order.
    pub components: Vec<ComponentRef>,
}

impl ImageReferences {
    /// Parse the raw `image-references` JSON (an OpenShift ImageStream).
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let stream: ImageStream =
            serde_json::from_slice(bytes).context("parsing image-references ImageStream")?;
        let components = stream
            .spec
            .tags
            .into_iter()
            .map(|t| ComponentRef {
                name: t.name,
                image: t.from.name,
            })
            .collect();
        Ok(Self { components })
    }

    /// Look up a component's image reference by name.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.components
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.image.as_str())
    }
}

#[derive(Deserialize)]
struct ImageStream {
    spec: ImageStreamSpec,
}

#[derive(Deserialize)]
struct ImageStreamSpec {
    #[serde(default)]
    tags: Vec<ImageStreamTag>,
}

#[derive(Deserialize)]
struct ImageStreamTag {
    name: String,
    from: ImageStreamFrom,
}

#[derive(Deserialize)]
struct ImageStreamFrom {
    name: String,
}

/// Find and parse `image-references` across the packed layer directories of a
/// release image. Checks the canonical `release-manifests/image-references`
/// path in each layer first, then falls back to a basename search.
pub fn find_image_references(layer_dirs: &[std::path::PathBuf]) -> Result<Option<ImageReferences>> {
    for dir in layer_dirs {
        let canonical = dir.join("release-manifests/image-references");
        if canonical.is_file() {
            let bytes = fs::read(&canonical)
                .with_context(|| format!("reading {}", canonical.display()))?;
            return Ok(Some(ImageReferences::parse(&bytes)?));
        }
    }
    for dir in layer_dirs {
        if let Some(found) = scan_for_image_references(dir)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// Shallow-walk a directory for any file basenamed `image-references`.
fn scan_for_image_references(dir: &Path) -> Result<Option<ImageReferences>> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            let ft = entry.file_type();
            if ft.map(|t| t.is_dir()).unwrap_or(false) {
                stack.push(path);
            } else if path.file_name().and_then(|n| n.to_str()) == Some("image-references") {
                let bytes = fs::read(&path)?;
                if let Ok(refs) = ImageReferences::parse(&bytes) {
                    return Ok(Some(refs));
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "kind": "ImageStream",
      "apiVersion": "image.openshift.io/v1",
      "spec": {
        "tags": [
          {"name": "machine-os-images", "from": {"kind": "DockerImage", "name": "quay.io/openshift-release-dev/ocp-v4.0-art-dev@sha256:aaa"}},
          {"name": "installer", "from": {"kind": "DockerImage", "name": "quay.io/openshift-release-dev/ocp-v4.0-art-dev@sha256:bbb"}},
          {"name": "cli", "from": {"kind": "DockerImage", "name": "quay.io/openshift-release-dev/ocp-v4.0-art-dev@sha256:ccc"}}
        ]
      }
    }"#;

    #[test]
    fn parses_image_stream() {
        let refs = ImageReferences::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(refs.components.len(), 3);
        assert_eq!(
            refs.get("machine-os-images"),
            Some("quay.io/openshift-release-dev/ocp-v4.0-art-dev@sha256:aaa")
        );
        assert_eq!(refs.get("missing"), None);
    }

    #[test]
    fn finds_in_canonical_path() {
        let tmp = std::env::temp_dir().join(format!("rspaced-rel-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let layer = tmp.join("layer0");
        fs::create_dir_all(layer.join("release-manifests")).unwrap();
        fs::write(layer.join("release-manifests/image-references"), SAMPLE).unwrap();

        let found = find_image_references(std::slice::from_ref(&layer))
            .unwrap()
            .unwrap();
        assert_eq!(found.components.len(), 3);
        let _ = fs::remove_dir_all(&tmp);
    }
}
