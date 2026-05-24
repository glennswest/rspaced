# Changelog

## [Unreleased]

### 2026-05-24
- **chore:** Initial repo scaffold — `CLAUDE.md`, `README.md`, `CHANGELOG.md`, `.gitignore`, and subdirectories for `rspaced_agent` and `compose_rspaced`.
- **chore:** Created GitHub repo `glennswest/rspaced` (public) and pushed initial commit.
- **feat:** Scaffolded `compose_rspaced` as a single Rust CLI (clap-derive): `latest` + per-output subcommands (`registry`/`iso`/`pxe`/`raw`/`qcow2`/`files`). Implements RHCOS mirror version discovery, atomic download, sha256 verification, and online/offline staging into a local cache. `files` output is functional; `registry`/`iso`/`pxe`/`raw`/`qcow2` are scaffolded stubs. Makefile wraps cargo.
- **docs:** Added `compose_rspaced/ARTIFACTS.md` (artifact → role → image → rspacefs registry map; online/offline workflow) and `rspaced_agent/DESIGN.md` (live-boot → pull-from-rspacefs → self-install → pivot-without-reboot; registry split; PVC config/push-back).
- **feat:** Made the central registry optional. `offline` mode now builds from the local cache / local rspacefs without requiring `--registry`; a registry is only needed for the `registry` push subcommand. ISO/PXE builds can run with no central registry at all.
