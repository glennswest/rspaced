#!/usr/bin/env bash
# Build the rspaced_agent hello-world bootc image and a bootable artifact.
# Runs on rspaced.g8.lo (podman present). Uses the rhel-coreos image from our
# packed store as the bootc base, so the RHCOS kernel comes from our content.
#
# Usage: ./build.sh            # qcow2 (default)
#        TYPE=iso ./build.sh   # bootable ISO
#
# NEVER uses coreos-installer.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"

STORE="${STORE:-/data/store}"
AUTH="${AUTH:-/data/pull-secret.json}"
TAG="${TAG:-localhost/rspaced-agent:hello}"
OUTDIR="${OUTDIR:-/data/agent-out}"
TYPE="${TYPE:-qcow2}"
BIB="${BIB:-quay.io/centos-bootc/bootc-image-builder:latest}"

# rhel-coreos bootc base, resolved from the release provenance.
BASE="${BASE:-$(python3 -c "
import json,glob
d=json.load(open(glob.glob('${STORE}/provenance/release-*.json')[0]))
print(next(c['image'] for c in d['components'] if c['name']=='rhel-coreos'))
")}"
echo "==> bootc base (rhel-coreos): ${BASE}"

echo "==> pull base (authed)"
sudo podman pull --authfile "${AUTH}" "${BASE}"

echo "==> build ${TAG}"
sudo podman build --build-arg BASE="${BASE}" -t "${TAG}" "${HERE}"

echo "==> bootc-image-builder --type ${TYPE}"
mkdir -p "${OUTDIR}"
sudo podman run --rm --privileged \
  --security-opt label=type:unconfined_t \
  -v "${OUTDIR}":/output \
  -v /var/lib/containers/storage:/var/lib/containers/storage \
  "${BIB}" build --type "${TYPE}" --local "${TAG}"

echo "==> done; artifact under ${OUTDIR}"
find "${OUTDIR}" -type f \( -name '*.qcow2' -o -name '*.iso' \) -printf '%p  %s bytes\n' 2>/dev/null || true
