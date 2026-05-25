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

## Scope: coreos-installer is the bar; start there

The bar (user, 2026-05-24) is **coreos-installer** — the disk-write step, not
the whole assisted-installer/agent stack. As invoked by assisted-installer
(`../agentinstall/upstream/assisted-installer/src/ops/ops.go`):

```
coreos-installer install --insecure -i <ignition> [--copy-network] [--append-karg …] <device>
```

i.e. coreos-installer: resolve the target **device**, write the OS image to it,
embed the **ignition** config, copy **network**, append **kernel args**.

**rspaced_agent v1 = the coreos-installer-equivalent**, where it differs:
1. **find the disk** — resolve the target device.
2. **partition it**.
3. **instance the rspacefs repos** on it (kernel / openshift-system / user / data).
4. **clone the bundled content** onto it, live during boot — this replaces
   "write the RHCOS metal image".
5. **config via PVC** (we can pass data now) — replaces `-i <ignition>`.
   Network/kargs applied as needed.
6. **same kernel, no reboot** (pivot only if media is read-only).

**Deferred** (per user — "some of the agent stuff we can defer"): the
assisted-installer-*agent* validations (disk_speed, connectivity,
domain_resolution, free_addresses, ntp, container_image_availability,
apivip/vips, tang) and the post-boot controller (CSR approval,
wait-for-operators). Add these later; not the first target.
