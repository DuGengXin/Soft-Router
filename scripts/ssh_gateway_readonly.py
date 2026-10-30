#!/usr/bin/env python3
"""Interactive, read-only SSH diagnostics for the deployed Gateway-Kit Linux host.

Password is requested via getpass (never echoed or placed in argv). Requires
Paramiko: python -m pip install paramiko
"""
from __future__ import annotations

import getpass
import sys

try:
    import paramiko
except ImportError:
    print("Missing dependency: paramiko. Install with: python -m pip install paramiko", file=sys.stderr)
    raise SystemExit(2)

HOST = "192.111.40.33"
USER = "root"
COMMAND = r'''set +e
printf '\n===== HOST/TIME =====\n'; hostname; date -Is
printf '\n===== SERVICE =====\n'; systemctl --no-pager --full status gateway-kit; printf '\n-- is-enabled --\n'; systemctl is-enabled gateway-kit 2>&1
printf '\n===== LISTENERS 7676 / SERVICES =====\n'; ss -lntp 2>&1 | grep -E '(:7676|State)' || true
printf '\n===== HTTP LOCAL =====\n'; curl -sS -m 5 -D - http://127.0.0.1:7676/ -o /dev/null 2>&1
printf '\n===== ADDRESSES / LINKS =====\n'; ip -br link; ip -br -4 addr
printf '\n===== ROUTES =====\n'; ip -4 route show table main; ip -4 rule show
printf '\n===== FORWARDING =====\n'; sysctl net.ipv4.ip_forward 2>&1
printf '\n===== GATEWAY-KIT STATE =====\n'; gateway-kit --version 2>&1; gateway-kit --local doctor 2>&1
printf '\n===== RELATED SERVICES =====\n'; systemctl --no-pager --full status gateway-kit sing-box dnsmasq 2>&1
printf '\n===== FIREWALL SNAPSHOT (READ ONLY) =====\n'; nft list table inet gateway_kit 2>&1; nft list ruleset 2>&1 | sed -n '1,220p'
printf '\n===== RECENT LOGS =====\n'; journalctl -u gateway-kit -u sing-box -u dnsmasq -n 100 --no-pager -o short-iso 2>&1
'''

def main() -> None:
    password = getpass.getpass(f"SSH password for {USER}@{HOST} (input hidden): ")
    client = paramiko.SSHClient()
    client.load_system_host_keys()
    # This host already has locally pinned keys in OpenSSH known_hosts.
    host_key_file = __import__("pathlib").Path.home() / ".ssh" / "known_hosts"
    try:
        client.get_host_keys().load(str(host_key_file))
    except OSError:
        pass
    client.set_missing_host_key_policy(paramiko.RejectPolicy())
    try:
        client.connect(HOST, username=USER, password=password, timeout=10,
                       banner_timeout=10, auth_timeout=10,
                       look_for_keys=False, allow_agent=False)
        stdin, stdout, stderr = client.exec_command(COMMAND, timeout=60)
        output = stdout.read().decode("utf-8", "replace")
        errors = stderr.read().decode("utf-8", "replace")
        print(output, end="")
        if errors:
            print("\n===== STDERR =====\n" + errors, end="")
        print(f"\nRemote exit status: {stdout.channel.recv_exit_status()}")
    except paramiko.BadHostKeyException:
        print("SSH host key mismatch. Stop and verify the server fingerprint out of band.", file=sys.stderr)
        raise SystemExit(3)
    except paramiko.AuthenticationException:
        print("SSH authentication failed; verify username/password and server auth policy.", file=sys.stderr)
        raise SystemExit(4)
    except Exception as exc:
        print(f"SSH diagnostic failed: {type(exc).__name__}: {exc}", file=sys.stderr)
        raise SystemExit(5)
    finally:
        client.close()

if __name__ == "__main__":
    main()
