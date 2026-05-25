#!/usr/bin/env bash
# Install/upgrade compose_rspaced on rspaced.g8.lo from an RPM.
# Procedure: CLEANUP BEFORE (remove any installed version), then INSTALL AFTER.
#
# Usage:
#   ./install-on-rspaced.sh <rpm-path-or-url>     # explicit RPM
#   ./install-on-rspaced.sh                       # fetch latest from GitHub release
#
# Run on rspaced.g8.lo (or via: ssh fedora@rspaced.g8.lo 'bash -s' < this).
set -euo pipefail

PKG=compose_rspaced
REPO=glennswest/rspaced
RPM="${1:-}"

if [[ -z "${RPM}" ]]; then
    echo "==> resolving latest ${PKG} rpm from github.com/${REPO} releases"
    RPM=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | python3 -c "import json,sys; [print(a['browser_download_url']) for a in json.load(sys.stdin).get('assets',[]) if a['name'].endswith('.rpm')]" \
        | head -1)
    [[ -n "${RPM}" ]] || { echo "no .rpm asset found in latest release" >&2; exit 1; }
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
