//! # rspaced-oci
//!
//! Minimal OCI Distribution v1.1 / Docker v2 **pull** client plus the image
//! media types it needs, for rspaced's compose pipeline.
//!
//! Ported from the Go [`fastregistry`](https://github.com/gwest/fastregistry)
//! project (`pkg/digest`, `pkg/oci`, `internal/sync/{registry,quay}.go`),
//! which hand-rolls the registry client over plain HTTP. This port keeps the
//! same wire behavior but adds the `WWW-Authenticate: Bearer` challenge →
//! token-exchange flow required for anonymous quay.io pulls of public
//! OpenShift release images (the Go source assumed a pre-supplied token).
//!
//! Scope: read-only pull (manifests, image indexes, blobs) with digest
//! verification. Packing pulled layers into rspacefs layer directories lives
//! in a separate crate.

mod client;
mod digest;
mod types;

pub use client::{Client, Reference};
pub use digest::Digest;
pub use types::{
    Descriptor, ImageConfig, Index, Manifest, Platform, media_type, MEDIA_TYPES_ACCEPT,
};
