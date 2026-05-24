# Enhancement: Multi-Registry Boot with Per-Type Routing + Ephemeral-Then-Persistent PVCs

Author: rspacefs side, requested 2026-05-24.
Target: rspaced (both `rspaced_agent` and `compose_rspaced`).

## Why

A boot manifest today is implicitly single-registry: the bootc artifact,
the OCI images, and whatever PVC/data the node needs all come from the
same logical place. As fleet sizes grow this is wrong on three axes:

1. **Different content types live in different places.** Kernels and the
   bootc artifact are usually built once and signed; OCI images change
   frequently; PVC data is environment-specific (cluster seed,
   credentials, model weights). They have different lifecycles, different
   trust roots, different sizes. Pinning all three to one registry is
   either insecure or unscalable.
2. **Air-gap / edge scenarios.** A site mirror might hold OCI images but
   not the kernel; the kernel can come from a corporate registry, the
   images from a site mirror, PVC data from a tenant-local store.
3. **PVCs need a lifecycle.** Some PVCs are read-only and persistent
   (license bundle); some are read-write and persistent (database
   directory); some are RW and ephemeral (scratch tmpfs that needs the
   first-boot content but discards it on reboot). The boot agent has to
   know which is which.

## What

Two pieces:

### 1. Registry routing by content type

`rspaced_agent` (and `compose_rspaced` for ISO bake-in) consumes a
`registry-routes.toml` config that maps content types → registry
endpoints + credentials:

```toml
# /etc/rspaced/registry-routes.toml
default = "site-mirror"

[registries.site-mirror]
url    = "https://registry.site.example.com:5000"
auth   = "kubernetes:/run/secrets/site-mirror/auth"   # mounted secret
tls_ca = "/etc/pki/ca-trust/source/anchors/site-ca.crt"

[registries.bootc]
url    = "https://bootc.corp.example.com:5000"
auth   = "kubernetes:/run/secrets/bootc/auth"

[registries.tenant-pvc]
url    = "https://qregistry.cluster.local:5000"
auth   = "htpasswd-file:/etc/rspaced/qregistry.htpasswd"

[routes]
# By OCI artifact-type media type. Most-specific match wins.
"application/vnd.bootc.image.manifest.v1+json"           = "bootc"
"application/vnd.qregistry.pvc.v1+json"                  = "tenant-pvc"
"application/vnd.oci.image.manifest.v1+json"             = "site-mirror"  # implicit via default
```

When the agent needs to pull anything, it consults `routes` first
(longest media-type match), then falls back to `default`. Each registry
endpoint has independent auth + TLS — no shared trust required.

A `--print-routes` flag dumps the resolved routing table for debugging.

### 2. Ephemeral-then-persistent PVC lifecycle

When `rspaced_agent` encounters a PVC manifest with
`qregistry.pvc.lifecycle = "ephemeral-then-persistent"`, the lifecycle
runs in two stages:

#### Stage A — early boot (ephemeral)

- Pull the PVC manifest + data blob via the routed `tenant-pvc` registry.
- Stage the content into a **tmpfs-backed rspacefs upper layer** under
  `/run/rspaced/pvcs/<tenant>/<pvc-name>/`. Lower is an empty layer;
  upper is tmpfs. (Or, if the data is large enough to warrant it, an
  rspacefs lower from the pulled blob + tmpfs upper for any writes —
  see `rspacefs/CLAUDE.md` for the layer model.)
- Mount the result at the PVC's target mount-point. The pod / kubelet
  doesn't know it's tmpfs-backed.
- Record the staging state in `/var/lib/rspaced/state.json` so a
  controller / operator can see it.

The node is then bootable with NO disk write of the PVC content yet.
If boot fails partway, a reboot starts clean — the tmpfs is gone.

#### Stage B — promote to persistent

On request from the control plane (rspaced REST endpoint
`POST /v1/pvcs/<name>/promote`) OR on a state-machine trigger ("node
Ready for 5 min, no recent restarts, kubelet healthy"), the agent:

1. Pauses writes to the PVC mount (kubelet quiesce or via fsfreeze).
2. Snapshots the rspacefs upper into a persistent backing under
   `/var/lib/rspaced/pvcs/<tenant>/<pvc-name>/`. Reflinks where the
   underlying FS supports them (xfs/btrfs/bcachefs).
3. Remounts the rspacefs with the new persistent backing as a layer
   under the (now-frozen) tmpfs upper — or pivots entirely off tmpfs.
4. Resumes writes.
5. Records the transition in `state.json`.

After promotion, a reboot keeps the PVC contents; before promotion, a
reboot discards them. This gives a "soft acceptance" gate during cluster
bring-up: an init-rolled cluster can decide whether to commit to the
seed data only once the cluster's actually working.

#### Failure modes & rollback

- Promotion fails → leave the PVC in ephemeral state, alert via the
  control surface. Next reboot starts clean.
- Boot fails post-promotion → standard PVC recovery (fsck, etc.); not
  this enhancement's problem.
- An `ephemeral` (never-persist) PVC just stays in stage A forever; the
  promote endpoint refuses for that lifecycle value.

## What stays out of scope

- **Mutating a PVC's lifecycle** after it's been pushed to qregistry.
  Push with the right annotation; if you need to change it, push a new
  revision.
- **Cross-node PVC sharing.** Each node pulls its own copy. ReadWriteMany
  is out of scope; it's a registry, not a shared filesystem.
- **Snapshot policy** (when to take new snapshots of a promoted PVC). v0
  is one-time-only promotion.

## Acceptance

- [ ] `registry-routes.toml` is parsed at agent startup
- [ ] Pull requests are dispatched to the right registry per route
- [ ] `--print-routes` prints the resolved routing table
- [ ] An `ephemeral-then-persistent` PVC stages to tmpfs at boot
- [ ] `POST /v1/pvcs/<name>/promote` snapshots the tmpfs to disk and
  keeps the mount live
- [ ] Promoted PVCs survive reboot; un-promoted PVCs don't
- [ ] Failure in stage B leaves the system in stage A, no data loss

## Open questions

1. **Auth pluggability shape.** htpasswd, kube-secret-mounted, AWS-IRSA,
   client-cert — make this a small trait or a TOML enum?
2. **Promote trigger source.** Recommend an explicit REST call from a
   controller (cluster-admin decides), with an optional auto-promote
   timer for unattended deployments.
3. **rspacefs layer model for stage A.** Is "empty lower + tmpfs upper"
   enough, or do we always want a non-empty lower from the pulled blob?
   For small seed data the empty-lower / tmpfs-upper is fine; for >1GB
   data we want a real lower so we don't double-RAM the bytes.

## Cross-references

- `qregistry/enhancements/pvc-content-type.md` — the registry side
- `rspacefs/docs/openshift-integration.md` — the FUSE / mount_program
  surface this depends on
- `rspaced_agent` — the bootc agent that runs this
