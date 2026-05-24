# rspaced

Holder repo for various implementations of our boot services.

## Subprojects

### `rspaced_agent/`
A bootc-based implementation of the boot agent. Writes `rspacefs` as the repo, and packages a chosen version of kernel (and supporting userspace) derived from RHCOS. Acts as a drop-in replacement for the RHCOS format / RHCOS boot used by the OpenShift agent-based installer (ABI).

### `compose_rspaced/`
The build tool. Given an existing OCP online repo as input, it produces one of two artifacts:

1. An **rspaced-based ISO** built with bootc.
2. A **signed bootc artifact** that behaves like the agent-based installer (consumes the same input config surface) but whose internals are signed bootc images rather than RHCOS.

Both targets are part of the move to a new bootc-based, signed boot pipeline.

## Version

`0.1.0` — initial scaffold.

Version locations (keep in sync on every bump):
- This file (`## Version` heading above)
- `CHANGELOG.md` release heading
- Subproject version files (to be added as code lands)

## Work Plan

- [x] Scaffold repo (`CLAUDE.md`, `README.md`, `CHANGELOG.md`, `.gitignore`, subproject dirs)
- [ ] Configure GitHub remote and push initial commit
- [ ] Flesh out `rspaced_agent/` — bootc `Containerfile`, `rspacefs` writer, kernel packaging
- [ ] Flesh out `compose_rspaced/` — input-repo ingestion, ISO build path, signed-bootc build path
- [ ] CI for both build targets
- [ ] First tagged release `v0.1.0` once a minimal end-to-end build works

## Conventions

- Follow the cross-project rules in `/Volumes/minihome/gwest/CLAUDE.md` (commit early, push often, keep `CHANGELOG.md` current, no secrets, semver).
- Each subproject gets its own `README.md` and, when it has compiled code, its own version file.
