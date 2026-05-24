//! The RHCOS install-media artifact set compose_rspaced knows how to handle.

/// One RHCOS install-media artifact.
pub struct Artifact {
    /// Logical role; drives OCI image naming and rspacefs registry placement.
    pub role: &'static str,
    /// Filename template; `{version}` and `{arch}` are substituted.
    pub template: &'static str,
    /// Basename used for the file inside a packaged OCI image.
    pub in_image_name: &'static str,
}

impl Artifact {
    /// Concrete mirror filename for a given version/arch.
    pub fn filename(&self, version: &str, arch: &str) -> String {
        self.template
            .replace("{version}", version)
            .replace("{arch}", arch)
    }
}

/// Authoritative ordered list. Producers iterate this so naming stays
/// consistent across the fetch / verify / package / emit stages.
///
/// The live kernel has no extension and a trailing `-{arch}`; every other
/// artifact uses `-{arch}.<ext>`.
pub const ARTIFACTS: &[Artifact] = &[
    Artifact {
        role: "kernel",
        template: "rhcos-{version}-{arch}-live-kernel-{arch}",
        in_image_name: "vmlinuz",
    },
    Artifact {
        role: "initramfs",
        template: "rhcos-{version}-{arch}-live-initramfs.{arch}.img",
        in_image_name: "initramfs.img",
    },
    Artifact {
        role: "rootfs",
        template: "rhcos-{version}-{arch}-live-rootfs.{arch}.img",
        in_image_name: "rootfs.img",
    },
    Artifact {
        role: "iso",
        template: "rhcos-{version}-{arch}-live.{arch}.iso",
        in_image_name: "live.iso",
    },
    Artifact {
        role: "metal",
        template: "rhcos-{version}-{arch}-metal.{arch}.raw.gz",
        in_image_name: "metal.raw.gz",
    },
    Artifact {
        role: "metal4k",
        template: "rhcos-{version}-{arch}-metal4k.{arch}.raw.gz",
        in_image_name: "metal4k.raw.gz",
    },
    Artifact {
        role: "qemu",
        template: "rhcos-{version}-{arch}-qemu.{arch}.qcow2.gz",
        in_image_name: "qemu.qcow2.gz",
    },
    Artifact {
        role: "vmware",
        template: "rhcos-{version}-{arch}-vmware.{arch}.ova",
        in_image_name: "vmware.ova",
    },
];

/// Look up an artifact by role.
pub fn by_role(role: &str) -> Option<&'static Artifact> {
    ARTIFACTS.iter().find(|a| a.role == role)
}
