//! The RHCOS install-media artifact set compose_rspaced knows how to handle.

/// One RHCOS install-media artifact.
pub struct Artifact {
    /// Logical role; drives OCI image naming and rspacefs registry placement.
    pub role: &'static str,
    /// Filename template; `{version}` and `{arch}` are substituted.
    pub template: &'static str,
    /// Basename used for the file inside a packaged OCI image.
    pub in_image_name: &'static str,
    /// Whether a bootable image cannot be assembled without this artifact. If a
    /// required artifact fails to stage or verify, staging fails outright rather
    /// than silently producing a partial set. Optional artifacts (alternate
    /// deployment formats the mirror may not publish for every release) are
    /// skipped with a warning when unavailable.
    pub required: bool,
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
///
/// `required` marks the boot trio (kernel + initramfs + rootfs) that a live
/// boot image cannot be assembled without. The remaining entries are alternate
/// deployment formats the mirror does not publish for every release, so they
/// are optional — missing ones are skipped with a warning.
pub const ARTIFACTS: &[Artifact] = &[
    Artifact {
        role: "kernel",
        template: "rhcos-{version}-{arch}-live-kernel-{arch}",
        in_image_name: "vmlinuz",
        required: true,
    },
    Artifact {
        role: "initramfs",
        template: "rhcos-{version}-{arch}-live-initramfs.{arch}.img",
        in_image_name: "initramfs.img",
        required: true,
    },
    Artifact {
        role: "rootfs",
        template: "rhcos-{version}-{arch}-live-rootfs.{arch}.img",
        in_image_name: "rootfs.img",
        required: true,
    },
    Artifact {
        role: "iso",
        template: "rhcos-{version}-{arch}-live.{arch}.iso",
        in_image_name: "live.iso",
        required: false,
    },
    Artifact {
        role: "metal",
        template: "rhcos-{version}-{arch}-metal.{arch}.raw.gz",
        in_image_name: "metal.raw.gz",
        required: false,
    },
    Artifact {
        role: "metal4k",
        template: "rhcos-{version}-{arch}-metal4k.{arch}.raw.gz",
        in_image_name: "metal4k.raw.gz",
        required: false,
    },
    Artifact {
        role: "qemu",
        template: "rhcos-{version}-{arch}-qemu.{arch}.qcow2.gz",
        in_image_name: "qemu.qcow2.gz",
        required: false,
    },
    Artifact {
        role: "vmware",
        template: "rhcos-{version}-{arch}-vmware.{arch}.ova",
        in_image_name: "vmware.ova",
        required: false,
    },
];

/// Look up an artifact by role. Used by the registry/output paths as they land.
#[allow(dead_code)]
pub fn by_role(role: &str) -> Option<&'static Artifact> {
    ARTIFACTS.iter().find(|a| a.role == role)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_set_is_the_boot_trio() {
        let required: Vec<&str> = ARTIFACTS
            .iter()
            .filter(|a| a.required)
            .map(|a| a.role)
            .collect();
        assert_eq!(required, ["kernel", "initramfs", "rootfs"]);
    }

    #[test]
    fn roles_are_unique() {
        let mut roles: Vec<&str> = ARTIFACTS.iter().map(|a| a.role).collect();
        let count = roles.len();
        roles.sort_unstable();
        roles.dedup();
        assert_eq!(roles.len(), count, "duplicate role in ARTIFACTS");
    }
}
