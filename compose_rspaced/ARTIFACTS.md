# RHCOS Artifacts

`compose_rspaced` consumes the RHCOS install-media set published on
`mirror.openshift.com` and re-homes it into rspaced's own registries and boot
formats.

## Mirror layout

```
https://mirror.openshift.com/pub/openshift-v4/<mirror-arch>/dependencies/rhcos/<x.y>/<x.y.z>/
```

- `<mirror-arch>` is an alias of the kernel-arch name:
  `x86_64 → amd64`, `aarch64 → arm64`. Other arches pass through unchanged.
- RHCOS lags OCP — e.g. latest 4.18 RHCOS is in the `4.18.x` line while the
  OCP installer may already be several z-streams ahead. `compose_rspaced
  latest --series 4.18` reports the newest RHCOS z-stream, not the OCP one.

## Artifact set

Source of truth is `src/artifacts.rs`. The live kernel has no extension and a
trailing `-<arch>`; everything else uses `-<arch>.<ext>`.

| Role | Mirror filename (`rhcos-<ver>-<arch>-…`) | In-image name | OCI image | rspacefs registry |
|---|---|---|---|---|
| `kernel` | `-live-kernel-<arch>` | `vmlinuz` | `rhcos-kernel:<ver>-<arch>` | kernel |
| `initramfs` | `-live-initramfs.<arch>.img` | `initramfs.img` | `rhcos-initramfs:<ver>-<arch>` | kernel |
| `rootfs` | `-live-rootfs.<arch>.img` | `rootfs.img` | `rhcos-rootfs:<ver>-<arch>` | openshift/system |
| `iso` | `-live.<arch>.iso` | `live.iso` | `rhcos-iso:<ver>-<arch>` | openshift/system |
| `metal` | `-metal.<arch>.raw.gz` | `metal.raw.gz` | `rhcos-metal:<ver>-<arch>` | openshift/system |
| `metal4k` | `-metal4k.<arch>.raw.gz` | `metal4k.raw.gz` | `rhcos-metal4k:<ver>-<arch>` | openshift/system |
| `qemu` | `-qemu.<arch>.qcow2.gz` | `qemu.qcow2.gz` | `rhcos-qemu:<ver>-<arch>` | openshift/system |
| `vmware` | `-vmware.<arch>.ova` | `vmware.ova` | `rhcos-vmware:<ver>-<arch>` | openshift/system |

Each artifact is packaged as a **separate single-layer OCI image** so the
rspacefs registries can hold and serve them independently. Image refs are
`<registry>/rhcos-<role>:<version>-<arch>` with labels:

- `org.rspaced.artifact.role`
- `org.rspaced.artifact.name`
- `org.rspaced.rhcos.version`
- `org.rspaced.rhcos.arch`

## Online vs offline

- **online** (default) — fetch artifacts and `sha256sum.txt` from the mirror,
  verify, cache locally. Run `compose_rspaced registry` once in this mode to
  populate qregistry.
- **offline** — source artifacts only from qregistry; never touch the mirror.
  Requires `--registry` and an explicit `--version` (cannot resolve "latest"
  without the mirror). Used for repeatable ISO/PXE builds on isolated hosts.

Normal workflow: populate qregistry online once, then build ISO/PXE `--mode
offline` from the registry — no redownloads, full offline capability.

## Verification

Online staging downloads the release `sha256sum.txt` and verifies every
fetched file against it before any artifact is packaged or emitted. Offline
staging verifies only if a cached `sha256sum.txt` is present.
