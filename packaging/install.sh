#!/usr/bin/env bash
# Install Gateway-Kit in observe mode. Does not apply routing/NAT/tunnels.
set -euo pipefail

BIN=""
PREFIX="/usr/local"
DRY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin) BIN="$2"; shift 2 ;;
    --prefix) PREFIX="$2"; shift 2 ;;
    --dry-run) DRY=1; shift ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ "$DRY" -eq 1 ]]; then
  echo "dry-run: would install gateway-kit observe-mode unit and enable systemd"
  echo "no nft/sysctl/wg/sing-box/address changes"
  exit 0
fi

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing dependency: $1" >&2
    echo "Debian/Ubuntu: apt-get install iproute2 nftables" >&2
    exit 1
  fi
}

hint() {
  local bin="$1"
  local pkg="$2"
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "optional (needed only after confirmed apply): $bin  —  apt-get install $pkg" >&2
  fi
}

need systemctl
need ip
need nft
hint wg "wireguard-tools"
hint dnsmasq dnsmasq
if ! command -v sing-box >/dev/null 2>&1; then
  echo "optional (needed only after confirmed apply): sing-box is not in Debian by default." >&2
  echo "install the official binary yourself; this script does not add third-party apt sources." >&2
fi

if [[ -z "$BIN" ]]; then
  echo "usage: sudo $0 --bin ./target/release/gateway-kit" >&2
  exit 2
fi
if [[ ! -x "$BIN" ]]; then
  echo "binary not executable: $BIN" >&2
  exit 1
fi

install -d /etc/gateway-kit /var/lib/gateway-kit /var/log/gateway-kit /var/backups/gateway-kit /etc/gateway-kit/generated
install -m 0755 "$BIN" "$PREFIX/bin/gateway-kit"
HERE="$(cd "$(dirname "$0")" && pwd)"
install -m 0644 "$HERE/gateway-kit.service" /etc/systemd/system/gateway-kit.service
install -m 0644 "$HERE/gateway-kit-singbox.service" /etc/systemd/system/gateway-kit-singbox.service
install -m 0644 "$HERE/gateway-kit-dnsmasq.service" /etc/systemd/system/gateway-kit-dnsmasq.service
install -m 0644 "$HERE/gateway-kit-forwarding.service" /etc/systemd/system/gateway-kit-forwarding.service
install -m 0755 "$HERE/gateway-kit-forwarding.sh" /usr/local/libexec/gateway-kit-forwarding.sh
install -m 0755 "$HERE/gateway-kit-wait-network.sh" /usr/local/libexec/gateway-kit-wait-network.sh

if [[ ! -f /etc/gateway-kit/config.toml ]]; then
  if [[ -f "$HERE/../config.example.toml" ]]; then
    install -m 0644 "$HERE/../config.example.toml" /etc/gateway-kit/config.toml
  fi
fi
if [[ ! -f /etc/gateway-kit/secrets.toml ]]; then
  umask 077
  cat >/etc/gateway-kit/secrets.toml <<'EOF'
wireguard_private_key = ""
wireguard_peer_public_key = ""
wireguard_preshared_key = ""
ui_lan_token = ""
proxy_uri = ""
EOF
  chmod 600 /etc/gateway-kit/secrets.toml
fi

systemctl daemon-reload
systemctl enable --now gateway-kit.service
systemctl disable gateway-kit-singbox.service >/dev/null 2>&1 || true
systemctl disable gateway-kit-dnsmasq.service >/dev/null 2>&1 || true
systemctl disable gateway-kit-forwarding.service >/dev/null 2>&1 || true
echo "installed. UI: http://127.0.0.1:7676"
echo "network was NOT changed (observe mode). sing-box/dnsmasq units are installed but not started."
echo "detected IPv4 addresses (install did not assign them):"
ip -4 -o addr show | awk '{print "  " $2, $4}' || true
