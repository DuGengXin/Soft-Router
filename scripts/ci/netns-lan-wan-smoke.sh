#!/usr/bin/env bash
# Dual-veth + WG/tproxy smoke.
# LAN ICMP to WAN peer (nft masquerade). LAN TCP to TEST-NET dest via WG+sing-box.
# After WG down: WAN ICMP still works, TEST-NET TCP fails.
# Does not prove a physical NIC reboot or VPS/Xray.
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "skip: Linux netns only"
  exit 0
fi
if [[ "${EUID}" -ne 0 ]]; then
  echo "re-exec with sudo"
  exec sudo --preserve-env=PATH "$0" "$@"
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="${1:-$ROOT/target/debug/gateway-kit}"
if [[ ! -x "$BIN" ]]; then
  echo "missing binary: $BIN (build gateway-app first)" >&2
  exit 1
fi
command -v wg >/dev/null || {
  echo "missing wg (apt-get install wireguard-tools)" >&2
  exit 1
}
modprobe wireguard 2>/dev/null || true

NS_GW=gk-ci-gw
NS_WAN=gk-ci-wan
NS_LAN=gk-ci-lan
NS_VPS=gk-ci-vps
WORKDIR="$(mktemp -d /tmp/gk-netns.XXXXXX)"
HOST_DEF="$(ip route show default || true)"
SB_PID=""
VPS_PID=""

SINGBOX_VER="${SINGBOX_VER:-1.13.18}"

install_singbox() {
  if command -v sing-box >/dev/null 2>&1; then
    command -v sing-box
    return
  fi
  local arch
  case "$(uname -m)" in
    x86_64) arch=amd64 ;;
    aarch64 | arm64) arch=arm64 ;;
    *)
      echo "unsupported arch for sing-box download: $(uname -m)" >&2
      exit 1
      ;;
  esac
  local tgz="$WORKDIR/sing-box.tgz"
  curl -fsSL -o "$tgz" \
    "https://github.com/SagerNet/sing-box/releases/download/v${SINGBOX_VER}/sing-box-${SINGBOX_VER}-linux-${arch}.tar.gz"
  tar -xzf "$tgz" -C "$WORKDIR"
  local found
  found="$(find "$WORKDIR" -type f -name sing-box | head -n1)"
  install -m 0755 "$found" "$WORKDIR/sing-box"
  echo "$WORKDIR/sing-box"
}

cleanup() {
  if [[ -n "$SB_PID" ]]; then kill "$SB_PID" 2>/dev/null || true; fi
  if [[ -n "$VPS_PID" ]]; then kill "$VPS_PID" 2>/dev/null || true; fi
  ip netns exec "$NS_GW" env -C "$WORKDIR" "$WORKDIR/gateway-kit" --local disable --confirm >/dev/null 2>&1 || true
  rm -f /etc/sysctl.d/99-gateway-kit.conf
  ip netns del "$NS_GW" 2>/dev/null || true
  ip netns del "$NS_WAN" 2>/dev/null || true
  ip netns del "$NS_LAN" 2>/dev/null || true
  ip netns del "$NS_VPS" 2>/dev/null || true
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

ip netns del "$NS_GW" 2>/dev/null || true
ip netns del "$NS_WAN" 2>/dev/null || true
ip netns del "$NS_LAN" 2>/dev/null || true
ip netns del "$NS_VPS" 2>/dev/null || true
ip netns add "$NS_GW"
ip netns add "$NS_WAN"
ip netns add "$NS_LAN"
ip netns add "$NS_VPS"

ip link add gk-wan type veth peer name gk-wan-p
ip link add gk-lan type veth peer name gk-lan-p
ip link add gk-ugw type veth peer name gk-uvps
ip link set gk-wan netns "$NS_GW"
ip link set gk-lan netns "$NS_GW"
ip link set gk-ugw netns "$NS_GW"
ip link set gk-wan-p netns "$NS_WAN"
ip link set gk-lan-p netns "$NS_LAN"
ip link set gk-uvps netns "$NS_VPS"

ip netns exec "$NS_WAN" ip link set lo up
ip netns exec "$NS_WAN" ip link set gk-wan-p up
ip netns exec "$NS_WAN" ip addr add 192.168.40.1/24 dev gk-wan-p

ip netns exec "$NS_LAN" ip link set lo up
ip netns exec "$NS_LAN" ip link set gk-lan-p up
ip netns exec "$NS_LAN" ip addr add 192.168.50.10/24 dev gk-lan-p
ip netns exec "$NS_LAN" ip route add default via 192.168.50.1

ip netns exec "$NS_GW" ip link set lo up
ip netns exec "$NS_GW" ip link set gk-wan up
ip netns exec "$NS_GW" ip link set gk-lan up
ip netns exec "$NS_GW" ip link set gk-ugw up
ip netns exec "$NS_GW" ip addr add 10.99.0.2/24 dev gk-ugw

ip netns exec "$NS_VPS" ip link set lo up
ip netns exec "$NS_VPS" ip link set gk-uvps up
ip netns exec "$NS_VPS" ip addr add 10.99.0.1/24 dev gk-uvps
ip netns exec "$NS_VPS" ip link add dummy0 type dummy
ip netns exec "$NS_VPS" ip addr add 203.0.113.1/32 dev dummy0
ip netns exec "$NS_VPS" ip link set dummy0 up

GW_PRIV="$(wg genkey)"
VPS_PRIV="$(wg genkey)"
GW_PUB="$(printf '%s' "$GW_PRIV" | wg pubkey)"
VPS_PUB="$(printf '%s' "$VPS_PRIV" | wg pubkey)"

umask 077
ip netns exec "$NS_VPS" ip link add wg-vps type wireguard
printf '%s' "$VPS_PRIV" >"$WORKDIR/vps.key"
ip netns exec "$NS_VPS" wg set wg-vps listen-port 51820 private-key "$WORKDIR/vps.key" \
  peer "$GW_PUB" allowed-ips 10.66.0.2/32,192.168.50.0/24 endpoint 10.99.0.2:51820
ip netns exec "$NS_VPS" ip addr add 10.66.0.1/32 dev wg-vps
ip netns exec "$NS_VPS" ip link set wg-vps up
ip netns exec "$NS_VPS" ip route add 10.66.0.2/32 dev wg-vps
ip netns exec "$NS_VPS" ip route add 192.168.50.0/24 dev wg-vps

ip netns exec "$NS_VPS" python3 - <<'PY' &
import socket
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("203.0.113.1", 8080))
s.listen(5)
while True:
    conn, _ = s.accept()
    conn.send(b"wg-ok")
    conn.close()
PY
VPS_PID=$!

install -m 0755 "$BIN" "$WORKDIR/gateway-kit"
SB_BIN="$(install_singbox)"

cat >"$WORKDIR/config.toml" <<EOF
mode = "gateway"

[wan]
interface = "gk-wan"
address = "192.168.40.2/24"
gateway = "192.168.40.1"
dns = ["192.168.40.1"]

[lan]
interface = "gk-lan"
address = "192.168.50.1/24"

[dhcp]
enabled = false

[firewall]
enabled = true
table_name = "gateway_kit"
ssh_port = 22

[wireguard]
enabled = true
interface = "wg0"
address = "10.66.0.2/32"
listen_port = 51820
peer_endpoint = "10.99.0.1:51820"
peer_allowed_ips = "10.66.0.1/32,203.0.113.1/32"

[routing]
china_direct = false
extra_direct_cidrs = ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"]
policy_table_id = 51820

[ui]
bind = "127.0.0.1"
port = 7676
EOF
cat >"$WORKDIR/secrets.toml" <<EOF
wireguard_private_key = "$GW_PRIV"
wireguard_peer_public_key = "$VPS_PUB"
ui_lan_token = ""
EOF
chmod 600 "$WORKDIR/secrets.toml"

ip netns exec "$NS_GW" env -C "$WORKDIR" "$WORKDIR/gateway-kit" --local apply --confirm

if ! grep -Fq 'udp dport 53 dnat ip to 192.168.50.1:53' "$WORKDIR/generated/nftables.conf" \
  || ! grep -Fq 'tcp dport 53 dnat ip to 192.168.50.1:53' "$WORKDIR/generated/nftables.conf"; then
  echo "LAN external DNS interception rules missing" >&2
  exit 1
fi

AFTER_DEF="$(ip route show default || true)"
if [[ "$HOST_DEF" != "$AFTER_DEF" ]]; then
  echo "host default route changed" >&2
  echo "before: $HOST_DEF" >&2
  echo "after:  $AFTER_DEF" >&2
  exit 1
fi

if ! ip netns exec "$NS_LAN" ping -c 3 -W 2 192.168.40.1; then
  echo "LAN client could not reach WAN peer (nft forward/masquerade)" >&2
  exit 1
fi

ip netns exec "$NS_GW" env -C "$WORKDIR/generated" PATH="$(dirname "$SB_BIN"):$PATH" "$SB_BIN" run -c "$WORKDIR/generated/sing-box.json" &
SB_PID=$!
sleep 2

ip netns exec "$NS_LAN" python3 - <<'PY'
import socket
s = socket.create_connection(("203.0.113.1", 8080), timeout=5)
data = s.recv(16)
s.close()
if data != b"wg-ok":
    raise SystemExit(f"unexpected payload {data!r}")
print("lan-tcp-via-wg ok")
PY

echo "unplug WG: LAN to WAN must still work; TEST-NET TCP must fail"
ip netns exec "$NS_GW" wg-quick down "$WORKDIR/generated/wg0.conf" 2>/dev/null || \
  ip netns exec "$NS_GW" ip link del wg0 2>/dev/null || true
if ! ip netns exec "$NS_LAN" ping -c 3 -W 2 192.168.40.1; then
  echo "domestic WAN path died after WG down" >&2
  exit 1
fi
ip netns exec "$NS_LAN" python3 - <<'PY'
import socket
try:
    s = socket.create_connection(("203.0.113.1", 8080), timeout=2)
    s.close()
    raise SystemExit("overseas TCP still worked after WG down")
except OSError:
    print("overseas tcp failed after wg down as expected")
PY

echo "simulate reboot dataplane loss"
if [[ -n "$SB_PID" ]]; then
  kill "$SB_PID" 2>/dev/null || true
  wait "$SB_PID" 2>/dev/null || true
  SB_PID=""
fi
ip netns exec "$NS_GW" nft delete table inet gateway_kit || true
ip netns exec "$NS_GW" ip rule del fwmark 1 lookup 51820 2>/dev/null || true
ip netns exec "$NS_GW" ip route flush table 51820 2>/dev/null || true
ip netns exec "$NS_GW" wg-quick down "$WORKDIR/generated/wg0.conf" 2>/dev/null || \
  ip netns exec "$NS_GW" ip link del wg0 2>/dev/null || true

ip netns exec "$NS_GW" env -C "$WORKDIR" "$WORKDIR/gateway-kit" --local agent --once

if ! ip netns exec "$NS_GW" nft list table inet gateway_kit >/dev/null; then
  echo "boot restore did not reload nft table" >&2
  exit 1
fi
if ! ip netns exec "$NS_LAN" ping -c 3 -W 2 192.168.40.1; then
  echo "LAN client lost WAN after simulated reboot restore" >&2
  exit 1
fi

ip netns exec "$NS_GW" env -C "$WORKDIR/generated" PATH="$(dirname "$SB_BIN"):$PATH" "$SB_BIN" run -c "$WORKDIR/generated/sing-box.json" &
SB_PID=$!
sleep 2
ip netns exec "$NS_LAN" python3 - <<'PY'
import socket
s = socket.create_connection(("203.0.113.1", 8080), timeout=5)
data = s.recv(16)
s.close()
if data != b"wg-ok":
    raise SystemExit(f"unexpected payload after restore {data!r}")
print("lan-tcp-via-wg after boot restore ok")
PY

echo "netns lan-wan + wg/tproxy + boot-restore smoke ok"
