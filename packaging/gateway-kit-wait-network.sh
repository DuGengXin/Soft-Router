#!/usr/bin/env bash
# Wait for configured physical interfaces to appear during early boot.
# A missing device is transient on USB/NIC firmware bring-up; do not mutate
# addresses or firewall state until both names are visible.
set -euo pipefail

timeout_seconds="${GATEWAY_KIT_NETWORK_WAIT_SECONDS:-60}"
if [[ -z "${LAN_IF:-}" || -z "${WAN_IF:-}" ]]; then
  exit 0
fi

for ((second = 0; second < timeout_seconds; second++)); do
  if ip link show dev "$LAN_IF" >/dev/null 2>&1 && ip link show dev "$WAN_IF" >/dev/null 2>&1; then
    exit 0
  fi
  sleep 1
done

echo "gateway-kit: timed out waiting for LAN=$LAN_IF WAN=$WAN_IF" >&2
exit 1
