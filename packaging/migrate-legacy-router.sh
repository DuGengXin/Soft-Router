#!/usr/bin/env bash
# Disable and archive the pre-Rust Python Router installation.
# This script never flushes the global nftables ruleset and never touches
# Docker tables or the netplan file that owns the physical NIC addresses.
set -euo pipefail

if [[ "${1:-}" != "--yes" ]]; then
  echo "refusing migration without explicit --yes" >&2
  echo "usage: sudo $0 --yes" >&2
  exit 2
fi
if [[ "${EUID}" -ne 0 ]]; then
  echo "run as root" >&2
  exit 1
fi

stamp="$(date +%Y%m%d-%H%M%S)"
archive="/var/backups/gateway-kit/legacy-router-${stamp}"
install -d -m 700 "$archive"

copy_if_present() {
  local path="$1"
  if [[ -e "$path" ]]; then
    cp --parents -a "$path" "$archive"
  fi
}

for path in \
  /root/work/soft-router \
  /etc/systemd/system/gateway-firewall.service \
  /etc/systemd/system/sing-box.service \
  /etc/systemd/system/dnsmasq.service.d/gateway-kit-override.conf \
  /etc/sing-box \
  /etc/dnsmasq.d/gateway.conf \
  /etc/nftables/gateway-router.conf \
  /etc/sysctl.d/99-gateway.conf \
  /etc/sysctl.d/99-gateway-forward.conf \
  /etc/sysctl.d/99-gateway-socket.conf; do
  copy_if_present "$path"
done

systemctl disable --now gateway-firewall.service sing-box.service dnsmasq.service \
  >/dev/null 2>&1 || true

# Remove only the old, named resources. Docker's ip/nft tables are untouched.
nft delete table inet router >/dev/null 2>&1 || true
while ip rule del fwmark 1 lookup 100 >/dev/null 2>&1; do :; done
ip route flush table 100 >/dev/null 2>&1 || true

rm -f \
  /etc/systemd/system/gateway-firewall.service \
  /etc/systemd/system/sing-box.service \
  /etc/dnsmasq.d/gateway.conf \
  /etc/nftables/gateway-router.conf \
  /etc/sysctl.d/99-gateway.conf \
  /etc/sysctl.d/99-gateway-forward.conf \
  /etc/sysctl.d/99-gateway-socket.conf
rm -f /etc/systemd/system/dnsmasq.service.d/gateway-kit-override.conf
rmdir /etc/systemd/system/dnsmasq.service.d 2>/dev/null || true
rm -rf /etc/sing-box
rm -rf /root/work/soft-router

systemctl daemon-reload
sysctl --system >/dev/null 2>&1 || true

chmod -R go-rwx "$archive"
echo "legacy Router archived at $archive"
echo "netplan and Docker resources were preserved"
