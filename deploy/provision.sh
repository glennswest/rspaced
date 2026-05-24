#!/usr/bin/env bash
# Provision rspaced.g8.lo: DNS A-record + Proxmox VM (root + persistent data
# disk) + wait for SSH. Idempotent; --force recreates the OS (data preserved).
#
# Usage: ./deploy/provision.sh [--force]
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_lib.sh
source "${HERE}/_lib.sh"

FORCE=0
for a in "$@"; do case "$a" in --force) FORCE=1 ;; *) echo "unknown arg: $a" >&2; exit 2 ;; esac; done

ssh-keygen -R "${VM_NAME}" >/dev/null 2>&1 || true
ssh-keygen -R "${VM_IP}"   >/dev/null 2>&1 || true

ensure_a_record "${VM_HOST}" "${VM_IP}"

if vm_exists && [[ ${FORCE} -ne 1 ]]; then
    echo "VM ${VMID} already exists; pass --force to recreate the OS"
    if ssh -o ConnectTimeout=3 -o BatchMode=yes "${VM_SSH}" 'echo ok' >/dev/null 2>&1; then
        echo "${VM_NAME}: already reachable. Skipping."
        exit 0
    fi
    echo "${VM_NAME}: exists but SSH unreachable; aborting (use --force)." >&2
    exit 1
fi

[[ ${FORCE} -eq 1 ]] && destroy_os
create_vm
start_vm
wait_for_ssh
echo "provision done: ${VM_NAME} @ ${VM_IP}"
