# Changelog

## [Unreleased]

### 2026-05-24
- **chore:** Initial repo scaffold — `CLAUDE.md`, `README.md`, `CHANGELOG.md`, `.gitignore`, and subdirectories for `rspaced_agent` and `compose_rspaced`.
- **chore:** Created GitHub repo `glennswest/rspaced` (public) and pushed initial commit.
- **feat:** Scaffolded `compose_rspaced` as a single Rust CLI (clap-derive): `latest` + per-output subcommands (`registry`/`iso`/`pxe`/`raw`/`qcow2`/`files`). Implements RHCOS mirror version discovery, atomic download, sha256 verification, and online/offline staging into a local cache. `files` output is functional; `registry`/`iso`/`pxe`/`raw`/`qcow2` are scaffolded stubs. Makefile wraps cargo.
- **docs:** Added `compose_rspaced/ARTIFACTS.md` (artifact → role → image → rspacefs registry map; online/offline workflow) and `rspaced_agent/DESIGN.md` (live-boot → pull-from-rspacefs → self-install → pivot-without-reboot; registry split; PVC config/push-back).
- **feat:** Made the central registry optional. `offline` mode now builds from the local cache / local rspacefs without requiring `--registry`; a registry is only needed for the `registry` push subcommand. ISO/PXE builds can run with no central registry at all.
- **feat:** Converted rspaced to a cargo workspace and added `crates/rspaced-oci` — an OCI Distribution v2 pull client + image types ported from fastregistry (`pkg/digest`, `pkg/oci`, `internal/sync/{registry,quay}.go`). Includes the `WWW-Authenticate: Bearer` challenge → token-exchange flow for anonymous quay.io pulls (validated live against `quay.io/openshift-release-dev/ocp-release:4.18.30-x86_64`).
- **docs:** Added `enhancements/multi-registry-boot.md` (rspacefs-side spec): registry routing by content type + ephemeral-then-persistent PVC lifecycle, targeting `rspaced_agent` + `compose_rspaced`.
