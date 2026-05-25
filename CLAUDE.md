# rspaced

Holder repo for various implementations of our boot services.

## Subprojects

### `rspaced_agent/`
The bootc-based boot agent that replaces RHCOS in the OpenShift agent-based-installer flow. Boots live (ISO or PXE), pulls assets from rspacefs (the overlayfs replacement) split across kernel / openshift-system / user registries, self-installs onto a new drive, and pivots **without a reboot** on the **same kernel**. Config and mutable state ride in PVCs (config in, push-back out) rather than ISO-embedded "write magic". See [`rspaced_agent/DESIGN.md`](./rspaced_agent/DESIGN.md). PVC/rspacefs primitives are consumed from upstream crates, not authored here.

### `compose_rspaced/`
A single Rust CLI (clap-derive). Input is an RHCOS version (or `--series` to resolve latest) plus arch; output is one of: push to **qregistry**, a bootc **ISO**, a **PXE** tree, a **raw**/**qcow2** image, or raw **files**. Two source modes: **online** (pull from `mirror.openshift.com`, verify against `sha256sum.txt`, cache locally) and **offline** (pull only from qregistry). Normal pattern: populate qregistry online once, then build ISO/PXE offline with no redownloads. See [`compose_rspaced/ARTIFACTS.md`](./compose_rspaced/ARTIFACTS.md).

## Version

`0.1.0` — initial scaffold.

Version locations (keep in sync on every bump):
- This file (`## Version` heading above)
- `CHANGELOG.md` release heading
- Subproject version files (to be added as code lands)

## Work Plan

- [x] Scaffold repo (`CLAUDE.md`, `README.md`, `CHANGELOG.md`, `.gitignore`, subproject dirs)
- [x] Configure GitHub remote and push initial commit (`git@github.com:glennswest/rspaced.git`, public)
- [x] Scaffold `compose_rspaced/` Rust CLI — `latest` + per-output subcommands, mirror discovery, atomic download, sha256 verify, online/offline staging (`files` output functional)
- [x] Document artifact map (`ARTIFACTS.md`) and agent boot/pivot model (`DESIGN.md`)
- [x] Convert to cargo workspace; add `crates/rspaced-oci` (OCI pull client + image types ported from fastregistry, anonymous quay.io bearer auth, validated live)
- [x] Add `crates/rspaced-pack` — pull image → extract layers into rspacefs layer dirs (`LayerFS` order, whiteouts preserved) + release `image-references` discovery (validated live: 188 components)
- [x] Full release payload pull + provenance (`compose_rspaced release`): content-addressed OCI store + per-layer compressed-digest → diff_id → verity chain + provenance JSON (validated bounded, blocked only on root for 0000 files locally)
- [x] Provision `rspaced.g8.lo` build/run/control host (VMID 120 @ .160, Fedora 43, persistent /data); assets in `deploy/`
- [x] CI: `build.yml` on forcicd (fmt/clippy/test/build green); builds on cicd, executes on rspaced.g8.lo
- [ ] Deliver the built binary to rspaced.g8.lo (publish from CI: OCI image to local registry or release asset) so it runs without build tools
- [ ] Run the full "pull everything" on rspaced.g8.lo as root (reads 0000 files; data on /data) — the real provenance test
- [ ] `snotest` VM-control harness (instance qpve scripts for pve.g8.lo/snotest.g8.lo) + provision snotest.g8.lo (VMID 121 @ .161)
- [ ] CI test job (`self-hosted` runner) orchestrating across rspaced.g8.lo + snotest.g8.lo; agent-installer monitor in an LXC
- [ ] Image patch: make CoreOS boot on rspacefs ("rustfs") — `mount_program`/storage.conf + kernel args + boot-time ordering (before CRI-O); version-sweep ready
- [ ] Stack packed layer dirs into a local rspacefs via `rspacefs-core` `LayerFS`
- [ ] Wire `compose_rspaced` registry push/pull to `rspaced-oci` — unblocks offline mode
- [ ] Implement `iso` output: gather signed assets → embed local rspacefs → bootc wrapper via bootc-image-builder (boots an already-installed live system; same RHCOS kernel; PVC config, no ISO write-magic)
- [ ] Implement `pxe` output (kernel + initramfs + rootfs reference tree)
- [ ] Implement `raw`/`qcow2` decompression passthrough
- [ ] Flesh out `rspaced_agent/` — bootc image, live-pivot logic, rspacefs registry wiring, PVC config/push-back
- [ ] First tagged release `v0.1.0` once a minimal end-to-end build works

## Conventions

- Follow the cross-project rules in `/Volumes/minihome/gwest/CLAUDE.md` (commit early, push often, keep `CHANGELOG.md` current, no secrets, semver).
- Each subproject gets its own `README.md` and, when it has compiled code, its own version file.
