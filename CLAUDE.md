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
- [ ] Pull the full release payload (loop component images from `image-references`) + RHCOS/`machine-os-images`; dedup, parallelize
- [ ] Stack packed layer dirs into a local rspacefs via `rspacefs-core` `LayerFS`
- [ ] Wire `compose_rspaced` registry push/pull to `rspaced-oci` — unblocks offline mode
- [ ] Implement `iso` output: gather signed assets → embed local rspacefs → bootc wrapper via bootc-image-builder (boots an already-installed live system; same RHCOS kernel; PVC config, no ISO write-magic)
- [ ] Implement `pxe` output (kernel + initramfs + rootfs reference tree)
- [ ] Implement `raw`/`qcow2` decompression passthrough
- [ ] Flesh out `rspaced_agent/` — bootc image, live-pivot logic, rspacefs registry wiring, PVC config/push-back
- [ ] CI for the Rust build + both boot targets
- [ ] First tagged release `v0.1.0` once a minimal end-to-end build works

## Conventions

- Follow the cross-project rules in `/Volumes/minihome/gwest/CLAUDE.md` (commit early, push often, keep `CHANGELOG.md` current, no secrets, semver).
- Each subproject gets its own `README.md` and, when it has compiled code, its own version file.
