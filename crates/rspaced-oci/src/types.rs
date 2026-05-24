//! OCI / Docker image media types and manifest structures.
//!
//! Ported from fastregistry `pkg/oci/types.go`.

use serde::{Deserialize, Serialize};

use crate::digest::Digest;

/// Well-known media-type strings.
pub mod media_type {
    // OCI
    /// OCI image manifest.
    pub const MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
    /// OCI image index (multi-arch).
    pub const INDEX: &str = "application/vnd.oci.image.index.v1+json";
    /// OCI image config.
    pub const IMAGE_CONFIG: &str = "application/vnd.oci.image.config.v1+json";
    /// OCI gzipped layer.
    pub const LAYER_GZIP: &str = "application/vnd.oci.image.layer.v1.tar+gzip";
    /// OCI zstd layer.
    pub const LAYER_ZSTD: &str = "application/vnd.oci.image.layer.v1.tar+zstd";

    // Docker (for compatibility)
    /// Docker v2 manifest.
    pub const DOCKER_MANIFEST: &str = "application/vnd.docker.distribution.manifest.v2+json";
    /// Docker v2 manifest list.
    pub const DOCKER_MANIFEST_LIST: &str =
        "application/vnd.docker.distribution.manifest.list.v2+json";
    /// Docker image config.
    pub const DOCKER_IMAGE_CONFIG: &str = "application/vnd.docker.container.image.v1+json";
    /// Docker gzipped layer.
    pub const DOCKER_LAYER_GZIP: &str = "application/vnd.docker.image.rootfs.diff.tar.gzip";
}

/// `Accept` header value covering both OCI and Docker manifest + index types.
pub const MEDIA_TYPES_ACCEPT: &str = "application/vnd.oci.image.manifest.v1+json, \
application/vnd.oci.image.index.v1+json, \
application/vnd.docker.distribution.manifest.v2+json, \
application/vnd.docker.distribution.manifest.list.v2+json";

/// A content-addressable blob reference.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Descriptor {
    /// Blob media type.
    #[serde(rename = "mediaType")]
    pub media_type: String,
    /// Blob digest.
    pub digest: Digest,
    /// Blob size in bytes.
    pub size: i64,
    /// Optional alternate download URLs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,
    /// Optional annotations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<std::collections::BTreeMap<String, String>>,
    /// Platform (present on index entries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
}

/// Target platform of an image or index entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Platform {
    /// CPU architecture (e.g. `amd64`, `arm64`).
    pub architecture: String,
    /// OS (e.g. `linux`).
    pub os: String,
    /// Optional OS version.
    #[serde(rename = "os.version", default, skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    /// Optional architecture variant (e.g. `v8`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

/// An OCI / Docker image manifest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    /// Schema version (2).
    #[serde(rename = "schemaVersion")]
    pub schema_version: i32,
    /// Manifest media type.
    #[serde(rename = "mediaType", default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// Image config descriptor.
    pub config: Descriptor,
    /// Ordered layer descriptors (lowest first).
    pub layers: Vec<Descriptor>,
    /// Optional annotations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<std::collections::BTreeMap<String, String>>,
}

/// An OCI image index / Docker manifest list (multi-arch).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Index {
    /// Schema version (2).
    #[serde(rename = "schemaVersion")]
    pub schema_version: i32,
    /// Index media type.
    #[serde(rename = "mediaType", default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// Per-platform manifest descriptors.
    pub manifests: Vec<Descriptor>,
    /// Optional annotations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<std::collections::BTreeMap<String, String>>,
}

/// The image configuration blob (`mediaType` image config).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageConfig {
    /// Architecture.
    pub architecture: String,
    /// OS.
    pub os: String,
    /// Root filesystem (diff IDs).
    pub rootfs: RootFs,
    /// Optional layer history.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<History>,
}

/// Root filesystem section of an image config.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RootFs {
    /// Always `layers`.
    #[serde(rename = "type")]
    pub fs_type: String,
    /// Uncompressed layer diff IDs.
    pub diff_ids: Vec<Digest>,
}

/// A single layer-history entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct History {
    /// Command that created the layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Whether this history entry produced no filesystem layer.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub empty_layer: bool,
}

impl Index {
    /// Pick the manifest descriptor for a given arch + os (e.g. `amd64`/`linux`).
    pub fn select(&self, arch: &str, os: &str) -> Option<&Descriptor> {
        self.manifests.iter().find(|d| {
            d.platform
                .as_ref()
                .is_some_and(|p| p.architecture == arch && p.os == os)
        })
    }
}
