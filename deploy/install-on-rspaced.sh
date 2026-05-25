#!/usr/bin/env bash
# Install/upgrade compose_rspaced on rspaced.g8.lo from an RPM.
# Procedure: CLEANUP BEFORE (remove any installed version), then INSTALL AFTER.
#
# Usage:
#   ./install-on-rspaced.sh <rpm-path-or-url>     # explicit RPM
#   ./install-on-rspaced.sh                       # fetch latest from the local
#                                                 # cicd (Forgejo) release
#
# Run on rspaced.g8.lo (or via: ssh fedora@rspaced.g8.lo 'bash -s' < this).
set -euo pipefail

PKG=compose_rspaced
# Local cicd (Forgejo on forcicd.g8.lo) — NOT github.
FORGEJO="${FORGEJO:-http://forcicd.g8.lo:3000/api/v1}"
REPO="${REPO:-ci/rspaced}"
RPM="${1:-}"

if [[ -z "${RPM}" ]]; then
    echo "==> resolving newest ${PKG} rpm from local cicd ${FORGEJO}/repos/${REPO}"
    # Use the releases list (newest first); /releases/latest skips prereleases.
    RPM=$(curl -fsSL "${FORGEJO}/repos/${REPO}/releases?limit=10" \
        | python3 -c "
import json,sys
for r in json.load(sys.stdin):
    for a in r.get('assets', []):
        if a['name'].endswith('.rpm'):
            print(a['browser_download_url']); sys.exit(0)
")
    [[ -n "${RPM}" ]] || { echo "no .rpm asset found in local-cicd releases" >&2; exit 1; }
fi
echo "==> rpm: ${RPM}"

# CLEANUP BEFORE — remove any previously installed version for a clean install.
echo "==> cleanup: removing existing ${PKG} (if any)"
sudo dnf remove -y "${PKG}" 2>/dev/null || true

# INSTALL AFTER.
echo "==> install: ${RPM}"
sudo dnf install -y "${RPM}"

echo "==> installed:"
command -v "${PKG}" && "${PKG}" --version
