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

- `--mode online` (default) — fetch from `mirror.openshift.com`, verify
  against `sha256sum.txt`, cache locally.
- `--mode offline` — source only from `--registry` (no mirror access).
  Requires an explicit `--version`.

Typical flow: run `registry` once online to populate qregistry, then build
`iso`/`pxe` `--mode offline` from the registry — no redownloads, fully offline.

## Status

Functional: `latest`, online fetch + sha256 verify + local caching, and the
`files` output. Stubs pending implementation (see the repo work plan):
`registry` push/pull (unblocks offline mode), `iso`, `pxe`, `raw`, `qcow2`.

See [`ARTIFACTS.md`](./ARTIFACTS.md) for the artifact → image → registry map.
