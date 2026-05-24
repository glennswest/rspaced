# compose_rspaced

A single Rust CLI that turns an RHCOS version into rspaced boot artifacts.

**Input:** an RHCOS version (or a `--series` to resolve the latest) plus an
architecture.
**Output (pick one per run):** push to a **qregistry**, a bootc **ISO**, a
**PXE** tree, a **raw**/**qcow2** image, or the raw **files**.

## Build

```sh
make            # debug build
make release    # optimized build
make install    # install to /usr/local/bin
make clippy     # lint (warnings = errors)
```

## Usage

```sh
# Latest RHCOS z-stream in a series
compose_rspaced latest --series 4.18 [--arch x86_64]

# Push the artifact set to qregistry (populates the offline cache)
compose_rspaced registry --version 4.18.30 --registry http://qregistry.gt.lo:5000

# Build outputs (each reuses the chosen RHCOS kernel)
compose_rspaced iso   --version 4.18.30 --out ./rspaced.iso
compose_rspaced pxe   --version 4.18.30 --out ./pxe/
compose_rspaced raw   --version 4.18.30 --out ./disk.raw
compose_rspaced qcow2 --version 4.18.30 --out ./disk.qcow2

# Inspect the raw RHCOS files
compose_rspaced files --version 4.18.30 --out ./files/
```

### Source modes

A central registry is **optional** — you can build an ISO/PXE with no registry
at all (rspacefs just local). Only the `registry` subcommand requires it.

- `--mode online` (default) — fetch from `mirror.openshift.com`, verify
  against `sha256sum.txt`, cache locally. Builds run from the local cache.
- `--mode offline` — build from the local cache / local rspacefs; no mirror
  access. Requires an explicit `--version`. If `--registry` is set, missing
  artifacts are pulled from it; otherwise everything must already be local.

Valid flows: (1) online once → `registry` push → later offline builds from it;
(2) online straight to an ISO with no registry; (3) fully offline from a local
cache / local rspacefs with no registry.

## Status

Functional: `latest`, online fetch + sha256 verify + local caching, and the
`files` output. Stubs pending implementation (see the repo work plan):
`registry` push/pull (unblocks offline mode), `iso`, `pxe`, `raw`, `qcow2`.

See [`ARTIFACTS.md`](./ARTIFACTS.md) for the artifact → image → registry map.
