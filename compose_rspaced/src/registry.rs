//! qregistry client: pull artifacts from / push artifacts to a local OCI
//! registry (https://github.com/glennswest/qregistry).
//!
//! Per-artifact image naming convention:
//!   <registry>/rhcos-<role>:<version>-<arch>
//! with OCI labels:
//!   org.rspaced.artifact.role, org.rspaced.artifact.name,
//!   org.rspaced.rhcos.version, org.rspaced.rhcos.arch
//!
//! These per-artifact images are the offline cache and long-term source of
//! truth: `compose_rspaced registry` populates them online, and later ISO/PXE
//! builds run `--mode offline` to source everything from here.

use anyhow::{bail, Result};
use std::path::Path;

/// Image reference for one artifact role in a registry. Used once push/pull land.
#[allow(dead_code)]
pub fn image_ref(registry: &str, role: &str, version: &str, arch: &str) -> String {
    let base = registry.trim_end_matches('/');
    format!("{base}/rhcos-{role}:{version}-{arch}")
}

/// Pull one artifact's file out of its OCI image in the registry into `dest`.
pub fn pull_artifact(
    _registry: &str,
    _version: &str,
    _arch: &str,
    _file: &str,
    _dest: &Path,
) -> Result<()> {
    // TODO: resolve the rhcos-<role> image, fetch its single layer, and
    // extract the artifact blob to `dest`. Until implemented, offline mode
    // cannot source from the registry.
    bail!("registry pull not yet implemented")
}

/// Push one artifact file as a single-layer OCI image to the registry.
pub fn push_artifact(
    _registry: &str,
    _role: &str,
    _version: &str,
    _arch: &str,
    _in_image_name: &str,
    _src: &Path,
) -> Result<()> {
    // TODO: build a scratch image carrying `src` at /artifact/<in_image_name>,
    // apply the org.rspaced.* labels, and push to image_ref(...).
    bail!("registry push not yet implemented")
}
