# rspaced_agent — Design

`rspaced_agent` is the bootc-based runtime that replaces RHCOS in the
OpenShift agent-based-installer flow. It consumes the artifacts and images
produced by [`compose_rspaced`](../compose_rspaced/).

## Goal

Boot once, compose the system from registries + PVCs, and pivot into the
installed system **without a reboot** — using the **same kernel** throughout.
This eliminates both the reboot cycle and the ISO-embedded "write magic" the
old agent flow used to inject state.

## Boot / pivot flow

1. **Live boot** — a bootc live ISO or PXE tree (built by `compose_rspaced`)
   brings up a minimal live environment on the chosen RHCOS kernel.
2. **Pull assets** — the agent pulls what it needs from rspacefs, the
   overlayfs replacement. Assets are split across registries (below).
3. **Self-install** — the agent lays the system down onto a new drive.
4. **Pivot without reboot** — switch root into the installed system in place.
   Because the live kernel and the installed kernel are the same RHCOS kernel,
   no kexec/reboot is required to match kernel and userspace.

## rspacefs registry split

rspacefs presents distinct filesystems, each backing a registry role:

- **kernel registry** — kernel + initramfs (the boot core).
- **openshift/system registry** — OS / system images (rootfs, metal, etc.).
- **user registry** — user/application images.
- **data (PVC)** — see below.

Long-term options: a single full image bundling every required container, or a
pull-through model that fetches on demand. Both are served through the same
registry split.

## PVC / data path

PVC support is provided by rspacefs and its PVC library crate (already in
flight upstream — rspaced is a *consumer*, not the author; see project memory
and the scope rule). Relevant properties rspaced_agent relies on:

- PVCs can be containers; **empty PVCs are supported** and can be created from
  boot.
- The PVC carries config **in** (replacing ISO-embedded ignition/config) and
  provides the **write/push-back** path **out**.

This is why the old ISO "write magic" is no longer needed: configuration and
mutable state live in PVCs composed at boot, not baked into the media.

## Relationship to compose_rspaced

`compose_rspaced` builds and hosts the inputs (per-artifact OCI images, live
ISO, PXE tree) in the registries; `rspaced_agent` is the runtime that consumes
them at boot. The "same kernel" constraint here is the reason `compose_rspaced`
always reuses the chosen RHCOS kernel rather than building its own.

## Scope: at least what the agent-based installer does (if not more)

The bar (set by the user): rspaced_agent must do **at least** what the
OpenShift agent-based installer does in the RHCOS live image, and then the
rspacefs/no-reboot extensions. Reference source lives at
`../agentinstall/upstream/{assisted-installer,assisted-installer-agent,assisted-service}`
— the components that run in the live image today. Map of their
responsibilities → rspaced_agent:

| agent-based installer (live image) | rspaced_agent |
|---|---|
| **inventory** — discover disks, CPU, mem, NICs (`assisted-installer-agent/src/inventory`) | **find the disk** + hardware discovery |
| **validations** — disk_speed, connectivity, domain_resolution, free_addresses, ntp, container_image_availability, apivip/vips, tang | same preflight checks (at least) |
| **next_step_runner** — execute steps the service hands it | local orchestration of the boot/install steps |
| **install** — `coreos-installer` writes RHCOS to disk: Format/partition, `--copy-network`, embed ignition (`assisted-installer/src/ops/ops.go`) | **partition** the disk + **instance rspacefs repos** on it + **clone the bundled content live during boot** (rspacefs replaces the coreos-installer disk write) |
| **controller** — approve CSRs, wait-for-operators, finalize (`assisted_installer_controller`) | bring the node up / finalize; CSR approval + operator readiness as needed |
| reboot into installed system | **no reboot** — same kernel, pivot only if media is read-only |

"If not more": the rspacefs delivery layer, PVC config/push-back (no
ISO-embedded ignition write-magic), signed assets, and same-kernel in-place
pivot are beyond what the stock installer does.
