#!/usr/bin/env bash
# Remove Gateway-Kit units and data-plane objects. Leaves /etc/gateway-kit config.
set -euo pipefail
systemctl disable --now gateway-kit.service 2>/dev/null || true
systemctl disable --now gateway-kit-singbox.service 2>/dev/null || true
systemctl disable --now gateway-kit-dnsmasq.service 2>/dev/null || true
systemctl disable --now gateway-kit-forwarding.service 2>/dev/null || true
nft delete table inet gateway_kit 2>/dev/null || true
ip rule del fwmark 1 lookup 51820 2>/dev/null || true
ip route flush table 51820 2>/dev/null || true
rm -f /etc/sysctl.d/99-gateway-kit.conf
rm -f /etc/systemd/system/gateway-kit.service \
  /etc/systemd/system/gateway-kit-singbox.service \
  /etc/systemd/system/gateway-kit-dnsmasq.service \
  /etc/systemd/system/gateway-kit-forwarding.service \
  /usr/local/libexec/gateway-kit-forwarding.sh \
  /usr/local/libexec/gateway-kit-wait-network.sh \
  /usr/local/bin/gateway-kit
systemctl daemon-reload
echo "uninstalled product units, nft table gateway_kit, and policy table 51820."
echo "config in /etc/gateway-kit was left in place."
