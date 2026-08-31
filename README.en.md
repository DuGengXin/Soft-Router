# Gateway-Kit · Rust Linux Soft-Router Gateway Control Plane

[中文 README](README.md)

Gateway-Kit is a Rust Linux soft-router control plane shipped as a single `gateway-kit` binary. It manages configuration, preflight discovery, plans, apply, health checks, and recovery while delegating low-level packet processing to the host network stack and configured data-plane components.

## Business overview

Gateway-Kit is for users who want to operate a Linux host as a home, small-office, or lab gateway. It turns interfaces, addressing, DHCP, DNS, forwarding, access rules, and an authorized encrypted remote connection into changes that can be inspected, previewed, confirmed, rolled back, and recovered.

Its value is to centralize network administration, detect conflicts before mutation, provide LAN gateway services, and preserve a safe recovery path when an apply or service fails. The normal workflow is: prepare the host, run doctor, configure the network in the UI, review the plan, explicitly confirm it, monitor health, and roll back or disable the feature when needed.

The project does not provide cloud nodes, accounts, content services, or telecommunications authorization. It manages only Linux hosts and network configurations that the user owns or is authorized to administer, and it does not determine whether a particular network use is lawful.

## Important notice

- **Terms risk**: The project may connect to configured upstream services. Your use may violate applicable software, provider, or service terms; read and follow all agreements.
- **Compliance**: Network tunneling, proxying, and cross-border connectivity may be regulated or require authorization in some jurisdictions, including mainland China. Use only where lawful and authorized, and consult qualified local counsel when necessary.
- **Disclaimer**: This project is for technical learning, research, lawful network administration, and testing equipment you own or are authorized to manage. The authors and contributors are not liable for account bans, service interruptions, data loss, network failures, or other direct or indirect losses.
- **Commercial use**: The authors do not endorse or support commercial network operations based on this project. The MIT License itself permits commercial use; see [`LICENSE`](LICENSE) for the actual terms.

## Safety and secrets

Observation mode is the default. Doctor, the Web UI, and the agent are read-only: they do not replace the default route, enable NAT, or bring up a tunnel. Network mutation requires plan review and explicit `apply --confirm` confirmation; failures converge to a safe bypass.

Only example configuration is committed. Never commit real configuration, secrets, databases, logs, generated files, build artifacts, diagnostics, connection URIs, private keys, UI tokens, or server credentials. Store secrets in `/etc/gateway-kit/secrets.toml` with mode `0600`; the API does not echo secret values.

## Quick start

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p gateway-app -- --local doctor
```

For Linux installation on Debian 12 or Ubuntu 22.04/24.04, see [`docs/用户文档/install.md`](docs/用户文档/install.md):

```bash
cargo build --release -p gateway-app
sudo ./packaging/install.sh --bin ./target/release/gateway-kit
```

Open `http://127.0.0.1:7676`, configure the network and an authorized encrypted remote connection, review the plan, and confirm it. Emergency bypass: `sudo gateway-kit disable --confirm`. Uninstall: `sudo ./packaging/uninstall.sh`.

## Repository structure

| Path | Purpose |
| --- | --- |
| `crates/gateway-model` | Configuration, resource, conflict, plan, and health models |
| `crates/gateway-core` | Discovery, state, rendering, apply, rollback, and recovery |
| `crates/gateway-app` | CLI, agent, HTTP API, and embedded Vue UI |
| `packaging/` | Installation, removal, and systemd units |
| `scripts/ci/` | Manual architecture and network-environment checks |
| `docs/` | Domain, architecture, change, and deployment documentation |

## Verification and acceptance

The repository includes manual formatting, test, clippy, architecture-boundary, and network-environment checks for pre-deployment validation. GitHub Actions runs the core checks automatically. Complete functional and stability acceptance in the target environment before production use.

See [`config.example.toml`](config.example.toml), [`secrets.example.toml`](secrets.example.toml), [`docs/用户文档/install.md`](docs/用户文档/install.md), [`AGENTS.md`](AGENTS.md), and [`LICENSE`](LICENSE).

## License

MIT License. See [`LICENSE`](LICENSE).
