//! # rspaced-pack
//!
//! Pull OCI images (via [`rspaced_oci`]) and **pack** their layers into
//! on-disk directories suitable for use as `rspacefs-core` `LayerFS` lower
//! layers, plus OpenShift release-payload discovery.
//!
//! Two halves:
//! - [`layer`] — turn an OCI image's gzipped tar layers into per-layer
//!   directories (OCI `.wh.` whiteouts preserved verbatim, since `LayerFS`
//!   interprets them).
//! - [`release`] — read `release-manifests/image-references` out of an
//!   OpenShift release image to enumerate every component image (kernel,
//!   installer, operators, …) by its Red Hat registry reference.
//!
//! The actual layered filesystem is created by `rspacefs-core` (consumed by
//! the runtime overlayfs replacement and qregistry); this crate only produces
//! the layer directories it stacks.

pub mod layer;
pub mod payload;
pub mod provenance;
pub mod release;
pub mod store;
pub mod verify;

pub use layer::{extract_layer, pack_image, LayerStats, PackedImage};
pub use payload::{pack_release, PackOptions, PackedComponent, PackedRelease};
pub use provenance::{ImageProvenance, LayerProvenance, ReleaseProvenance};
pub use release::{find_image_references, ComponentRef, ImageReferences};
pub use verify::{verify_store, VerifyReport};
