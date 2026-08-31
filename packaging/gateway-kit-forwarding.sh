#!/usr/bin/env bash
set -euo pipefail

action="${1:-apply}"
lan_if="${LAN_IF:?LAN_IF is required}"
wan_if="${WAN_IF:?WAN_IF is required}"

# Docker may install a DROP policy in FORWARD. Add only two interface-scoped,
# tagged rules; never flush or rewrite Docker/UFW/firewalld chains.
command -v iptables >/dev/null 2>&1 || exit 0

chain="FORWARD"
if iptables -nL DOCKER-USER >/dev/null 2>&1; then
  chain="DOCKER-USER"
fi

lan_rule=( -i "$lan_if" -o "$wan_if" -j ACCEPT -m comment --comment gateway-kit )
return_rule=( -i "$wan_if" -o "$lan_if" -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT -m comment --comment gateway-kit )

ensure_rule() {
  iptables -C "$chain" "$@" >/dev/null 2>&1 || iptables -I "$chain" 1 "$@"
}

remove_rule() {
  while iptables -C "$chain" "$@" >/dev/null 2>&1; do
    iptables -D "$chain" "$@"
  done
}

case "$action" in
  apply)
    ensure_rule "${lan_rule[@]}"
    ensure_rule "${return_rule[@]}"
    ;;
  remove)
    remove_rule "${lan_rule[@]}"
    remove_rule "${return_rule[@]}"
    ;;
  *)
    echo "usage: $0 {apply|remove}" >&2
    exit 2
    ;;
esac
