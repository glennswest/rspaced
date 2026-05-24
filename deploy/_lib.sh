#!/usr/bin/env bash
# Common helpers for rspaced provisioning. Sourced via `source _lib.sh`.
# Instanced from forcicd's recipe; adds a persistent data disk.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG="${REPO_ROOT}/proxmox.env"
[[ -f "${CONFIG}" ]] || { echo "missing ${CONFIG}" >&2; exit 2; }
# shellcheck source=/dev/null
source "${CONFIG}"

VM_SSH="fedora@${VM_NAME}"

# ----- DNS (MicroDNS) --------------------------------------------

dns_record_id() {
    curl --silent --max-time 5 \
        "${MICRODNS_URL}/zones/${G8_ZONE_ID}/records?limit=500" \
        | python3 -c "
import json, sys
for r in json.load(sys.stdin):
    if r['name'] == '$1' and r['data'].get('type') == 'A':
        print(r['id']); break
"
}

ensure_a_record() {
    local name="$1" ip="$2" existing
    existing=$(dns_record_id "${name}")
    if [[ -n "${existing}" ]]; then
        curl --silent --max-time 5 -X PUT \
            "${MICRODNS_URL}/zones/${G8_ZONE_ID}/records/${existing}" \
            -H 'Content-Type: application/json' \
            -d "{\"data\":{\"type\":\"A\",\"data\":\"${ip}\"},\"ttl\":300}" >/dev/null
        echo "DNS: ${name}.g8.lo -> ${ip} (updated)"
    else
        curl --silent --max-time 5 -X POST \
            "${MICRODNS_URL}/zones/${G8_ZONE_ID}/records" \
            -H 'Content-Type: application/json' \
            -d "{\"name\":\"${name}\",\"ttl\":300,\"data\":{\"type\":\"A\",\"data\":\"${ip}\"},\"enabled\":true}" >/dev/null
        echo "DNS: ${name}.g8.lo -> ${ip} (created)"
    fi
}

# ----- Proxmox VM lifecycle (over SSH to PVE_HOST) ---------------

vm_exists() { ssh "${PVE_HOST}" "qm status ${VMID} >/dev/null 2>&1"; }

# Destroy the VM but PRESERVE the data disk (scsi1): detach it first, and do
# NOT pass --destroy-unreferenced-disks, so /data survives an OS wipe.
destroy_os() {
    if vm_exists; then
        echo "VM ${VMID}: stopping + destroying OS (data disk preserved)"
        ssh "${PVE_HOST}" "set -e
            qm stop ${VMID} --skiplock 1 --timeout 30 2>/dev/null || true
            sleep 2
            qm set ${VMID} --delete scsi1 2>/dev/null || true   # unreference data disk
            qm destroy ${VMID} --purge"
    fi
}

create_vm() {
    if vm_exists; then
        echo "VM ${VMID} already exists; use destroy_os first" >&2
        return 1
    fi
    scp -q \
        "${REPO_ROOT}/cloud-init/rspaced-user-data.yaml" \
        "${REPO_ROOT}/cloud-init/rspaced-network-config.yaml" \
        "${PVE_HOST}:${PVE_SNIPPETS}/"
    ssh "${PVE_HOST}" "set -e
        qm create ${VMID} --name ${VM_NAME} --memory ${VM_MEMORY_MB} --cores ${VM_CORES} --sockets 1 \
            --cpu host --machine q35 --bios ovmf --ostype l26 \
            --net0 virtio,bridge=${PVE_BRIDGE} --agent enabled=1 \
            --scsihw virtio-scsi-single --serial0 socket --vga serial0
        qm set ${VMID} --efidisk0 ${PVE_STORAGE}:0,efitype=4m,pre-enrolled-keys=0,size=4M
        qm importdisk ${VMID} ${PVE_IMG} ${PVE_STORAGE} --format raw
        qm set ${VMID} --scsi0 ${PVE_STORAGE}:vm-${VMID}-disk-1,discard=on,iothread=1,ssd=1
        qm resize ${VMID} scsi0 ${VM_DISK_GB}G
        qm set ${VMID} --scsi1 ${PVE_STORAGE}:${VM_DATA_GB},discard=on,iothread=1,ssd=1
        qm set ${VMID} --ide2 ${PVE_STORAGE}:cloudinit
        qm set ${VMID} --cicustom \"user=local:snippets/rspaced-user-data.yaml,network=local:snippets/rspaced-network-config.yaml\"
        qm set ${VMID} --ipconfig0 ip=${VM_IP}/24,gw=${VM_GW}
        qm set ${VMID} --boot order=scsi0"
    echo "VM ${VMID} (${VM_NAME} @ ${VM_IP}) created — root ${VM_DISK_GB}G + data ${VM_DATA_GB}G"
}

start_vm() { ssh "${PVE_HOST}" "qm start ${VMID}"; echo "VM ${VMID} started"; }

wait_for_ssh() {
    local deadline="${1:-600}" start; start=$(date +%s)
    while true; do
        (( $(date +%s) - start > deadline )) && { echo "${VM_NAME}: no SSH in ${deadline}s" >&2; return 1; }
        if ssh -o ConnectTimeout=3 -o BatchMode=yes -o StrictHostKeyChecking=accept-new \
            "${VM_SSH}" 'echo ok' >/dev/null 2>&1; then
            echo "${VM_NAME}: SSH ready after $(($(date +%s) - start))s"; return 0
        fi
        sleep 5
    done
}
