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

## rspaced_agent build order (user, 2026-05-25)

The agent is a **bootc app** — it *is* the boot logic and the **GRUB
replacement**. We write our own; **never use `coreos-installer`** (we may
**port its Rust code** for disk/partition handling, but never invoke it — see
[[feedback-no-coreos-installer]]).

1. **Hello world** — a bootc app that boots and prints to **console and/or
   serial**. Just prove the bootc path + console output. Console/serial kargs
   come from the bootc image's `kargs.d`, NOT coreos-installer.
2. **Kernel from rspacefs** — kernel + initramfs are images inside rspacefs
   (kernel registry); the bootc app brings them up, wired to rspacefs.
3. **State check** — *am I on boot media, or already resident on disk?*
4. **If boot media:** determine the target disk (hardcoded default now; a
   **YAML** later, like the assisted agent — cribbing it is worthwhile) →
   **format** → **set up rspacefs** for the types (kernel/openshift-system/
   user/data) → **copy the bootc app in** (replaces GRUB, so a **reboot just
   works without reformatting**) → **pivot** and continue the normal SNO process.
5. **Agent (assisted-installer-agent) is optional** — bare-metal install must
   work without it; its diagnostics + progress reporting are a nice-to-have to
   copy in later.

Console/kargs/config are set the bootc-native way (image `kargs.d`, PVC for
config) — never by hacking a stock ISO.

## Boot model (LOCKED, user 2026-05-25): OpenShift's kernel+initramfs + composefs flow

Do **not** build a bootc/OS image and do **not** bake or load a separate
kernel (no bootc-image-builder, no coreos-installer, no stub+kexec, no UEFI
rspacefs — all rejected). Instead:

- **Boot the exact RHCOS kernel + initramfs OpenShift uses** — the ones from the
  payload we already pulled into rspacefs (machine-os-images / rhel-coreos).
  That's the same kernel as the release, so the kernel invariant holds for free:
  the kernel we boot *is* the kernel the system runs on. No second kernel.
- **Our agent runs inside that initramfs** (we manipulate the initramfs to add
  it) and does **formatting, rspacefs setup, and the transition/pivot** — all on
  that one kernel.
- **Follow the composefs boot flow** (rspacefs is our analog of composefs):
  - composefs: in the initramfs, `mount.composefs IMAGE TARGET -o basedir=…,digest=<fs-verity>` mounts a content-addressed root (EROFS metadata + content basedir) as overlayfs, **validating the fs-verity digest pinned on the kernel cmdline** (chain of trust), with `upperdir`/`workdir` for the writable layer; then `switch_root`.
  - rspaced: the agent mounts the **rspacefs** root (LayerFS + verity, digest pinned on the cmdline) the same way and `switch_root`s in. This keeps provenance unbroken right through the pivot (the verity root we recorded at pack time is checked at mount).
  - Crib `composefs` (`mount.composefs`, the boot flow) and `composefs-rs` (Rust) for the repo/mount logic.

**Therefore the boot media is just: RHCOS `vmlinuz` + a (modified) `initramfs`
carrying the agent.** One agent works for every release; the kernel/initramfs
are content-addressed images in rspacefs, so updating either is just a new image.

Build/test path (to confirm): take `vmlinuz` + `initramfs` from the packed
payload, inject the agent into the initramfs, boot snotest off them with
`console=ttyS0` on the cmdline, agent prints hello → then grows into
find-disk/format/rspacefs-setup/mount-root(composefs-flow)/switch_root.
