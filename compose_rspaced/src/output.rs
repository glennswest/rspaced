//! Output emitters. `files` is functional; the rest are scaffolded stubs
//! pending the bootc/registry build work in the project plan.

use anyhow::{anyhow, bail, Result};
use std::fs;
use std::path::Path;

use crate::cli::SourceArgs;
use crate::stage::Staged;

/// Write the raw RHCOS files into `out` (a directory). Functional MVP.
pub fn files(_src: &SourceArgs, staged: &Staged, out: &Path) -> Result<()> {
    fs::create_dir_all(out)?;
    for a in &staged.artifacts {
        let dest = out.join(&a.source_name);
        fs::copy(&a.local_path, &dest)?;
        tracing::info!(role = a.role, dest = %dest.display(), "wrote artifact");
    }
    tracing::info!(out = %out.display(), "files written");
    Ok(())
}

/// Push every staged artifact to the qregistry. Stub.
pub fn registry(src: &SourceArgs, staged: &Staged) -> Result<()> {
    let reg = src
        .registry
        .as_ref()
        .ok_or_else(|| anyhow!("--registry is required for the `registry` subcommand"))?;

    for a in &staged.artifacts {
        crate::registry::push_artifact(
            reg,
            a.role,
            &staged.version,
            &staged.arch,
            a.in_image_name,
            &a.local_path,
        )?;
    }
    Ok(())
}

/// Build a bootc-based live ISO. Stub.
pub fn iso(_src: &SourceArgs, _staged: &Staged, _out: &Path) -> Result<()> {
    // TODO: assemble a bootc image reusing the staged kernel/initramfs/rootfs
    // (same kernel as the chosen RHCOS version) and convert it to a live ISO.
    bail!("`iso` output not yet implemented")
}

/// Build a PXE-bootable tree. Stub.
pub fn pxe(_src: &SourceArgs, _staged: &Staged, _out: &Path) -> Result<()> {
    // TODO: emit kernel + initramfs + a rootfs container reference suitable
    // for netboot, into `out`.
    bail!("`pxe` output not yet implemented")
}

/// Emit the metal raw disk image (decompressed). Stub.
pub fn raw(_src: &SourceArgs, _staged: &Staged, _out: &Path) -> Result<()> {
    // TODO: decompress the staged `metal` artifact to `out`.
    // (`files` already writes the .gz as-is in the meantime.)
    bail!("`raw` output not yet implemented")
}

/// Emit the QEMU qcow2 image (decompressed). Stub.
pub fn qcow2(_src: &SourceArgs, _staged: &Staged, _out: &Path) -> Result<()> {
    // TODO: decompress the staged `qemu` artifact to `out`.
    bail!("`qcow2` output not yet implemented")
}
