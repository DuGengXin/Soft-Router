## 重要提醒 / Important Notice

使用本项目前，请务必仔细阅读以下内容：

- **🚨 服务条款风险 / Terms risk**：本项目可能连接或配置 WireGuard、sing-box、VLESS 或其他上游服务。使用方式可能违反相关软件、节点提供商或上游服务商的服务条款；请先阅读并遵守适用协议。由此产生的一切风险由用户自行承担。
- **⚖️ 中国大陆法律与合规 / Mainland China compliance**：在中国大陆，VPN、代理、跨境网络连接及相关经营/提供行为可能受到法律法规和监管要求约束，特定情形可能需要许可或触犯法条。请在符合所在国家或地区法律法规、取得必要授权的前提下使用，严禁用于任何违法违规用途；使用前请咨询合资格的本地法律专业人士。
- **📖 免责声明 / Disclaimer**：本项目仅供技术学习、研究、合法网络管理和自有设备测试使用，不构成法律、合规、网络安全或电信业务建议。作者和贡献者不对账户封禁、服务中断、数据丢失、网络故障或其他直接、间接损失承担责任。
- **🚫 商业运营不背书 / No commercial endorsement**：项目作者不为任何基于本项目的商业化网络运营、节点销售、跨境电信服务或第三方服务提供背书、授权或支持；相关行为主体自行承担全部纠纷、损失和法律责任。注意：本项目采用 MIT License，许可证本身允许商业使用，具体权利和义务以 [`LICENSE`](LICENSE) 为准。

Before using this project, read the following carefully:

- **🚨 Terms risk**: The project may connect to or configure WireGuard, sing-box, VLESS, or other upstream services. Your use may violate software, node-provider, or upstream-service terms. Read and comply with all applicable agreements; you assume all related risk.
- **⚖️ Mainland China compliance**: In mainland China, VPNs, proxies, cross-border network connections, and related operation or provision may be regulated, require licensing in particular circumstances, or potentially implicate criminal or other legal provisions. Use only where lawful and authorized, never for unlawful purposes, and consult qualified local counsel.
- **📖 Disclaimer**: This project is for technical learning, research, lawful network administration, and testing equipment you own or are authorized to manage. It is not legal, compliance, cybersecurity, or telecommunications advice. The authors and contributors are not liable for account bans, service interruptions, data loss, network failures, or other direct or indirect losses.
- **🚫 No commercial endorsement**: The authors do not endorse, authorize, or support commercial network operations, node sales, cross-border telecommunications services, or third-party services based on this project. The relevant operator assumes all disputes, losses, and legal responsibility. The MIT License does permit commercial use; see [`LICENSE`](LICENSE) for the actual license terms.

官方背景资料 / Official background: [工信部关于清理规范互联网网络接入服务市场的通知](https://www.cac.gov.cn/2017-01/23/c_1120366809.htm)。该链接不构成针对任何个人或部署的法律结论。

# Gateway-Kit · Rust Linux Soft-Router Gateway Control Plane

[English version](README.en.md)

Linux 软路由控制面，使用 Rust 构建为单一 `gateway-kit` 二进制。它负责配置、预检、计划、应用、健康检查和恢复；数据面由 Linux `nftables`、WireGuard、`sing-box` 与 `dnsmasq` 承担。

## 项目业务介绍

Gateway-Kit 面向需要把一台 Linux 主机改造成家庭、小型办公室或实验环境网关的用户。它不是一个单纯的命令行脚本，而是一个负责“把网络配置安全地落地”的控制面：将网卡、地址、DHCP、DNS、转发、访问规则和远程加密出口组织成可检查、可预览、可确认、可回滚的配置变更。

它主要解决四类业务问题：

- **统一管理**：通过 Web UI 和命令行管理 WAN/LAN、客户端、端口转发、DNS 与路由策略，减少手工修改多套系统配置的风险。
- **变更可审计**：先读取主机现状，识别 Docker、已有防火墙、旧网关、路由和端口冲突，再生成计划；没有明确确认时保持观察，不直接改网络。
- **家庭/小型网络网关**：为 LAN 客户端提供地址分配、网关和 DNS 服务，并根据规则选择本地直连或已配置的远程加密出口。
- **故障可恢复**：应用过程记录代际和操作日志，应用失败、进程异常或重启恢复异常时进入安全旁路，便于继续通过 SSH 排查和关闭功能。

典型使用流程是：准备一台双网口 Linux 主机 → 运行 doctor 检查现有环境 → 在 UI 填写网络参数 → 预览计划并处理阻塞项 → 明确确认应用 → 观察健康状态 → 必要时回滚或进入紧急旁路。项目默认不替换主默认路由，也不要求用户把真实凭据写进仓库。

项目不提供云端节点、账号体系、内容服务或网络接入资质，也不替用户判断某种网络连接是否合法；它只管理用户拥有或获授权管理的 Linux 主机及其网络配置。

## 安全边界

默认是观察模式：doctor、Web UI 和 agent 只读，不改默认路由、不 NAT、不拉起隧道。任何网络变更都必须经过计划检查，并由 UI 或 `apply --confirm` 明确确认；失败时收敛到安全旁路。

本仓库只提交示例配置。真实的 `config.toml`、`secrets.toml`、数据库、日志、生成文件、构建产物和本机诊断输出均不得提交。密钥放在 `/etc/gateway-kit/secrets.toml`，安装后权限应为 `0600`；API 不回显明文密钥。若凭据曾经被提交，即使后来删除，也必须立即撤销/轮换，并按“安全清理历史”流程重写远端历史。

## 快速开始

开发机（Windows、Linux 或 macOS）可运行模型和核心测试；实际 apply 只支持目标 Linux 主机：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p gateway-app -- --local doctor
cargo run -p gateway-app -- --local agent --once
```

构建 Web UI 需要 Node.js 20+；发布后的单二进制运行时不依赖 npm。前端源码位于 `crates/gateway-app/ui/`。

## Linux 安装

支持目标为 Debian 12、Ubuntu 22.04/24.04，systemd，x86_64 或 aarch64。完整步骤见 [`docs/用户文档/install.md`](docs/用户文档/install.md)：

```bash
cargo build --release -p gateway-app
sudo ./packaging/install.sh --bin ./target/release/gateway-kit
```

安装脚本默认只安装文件和 systemd 单元，不改变主路由。安装后访问 `http://127.0.0.1:7676`，在向导中填写 WAN/LAN 与已授权的加密隧道或远程出口，预览计划并确认应用。紧急旁路：`sudo gateway-kit disable --confirm`；卸载：`sudo ./packaging/uninstall.sh`。

示例配置：[`config.example.toml`](config.example.toml)；敏感配置模板：[`secrets.example.toml`](secrets.example.toml)。不要把真实连接 URI、私钥、UI token 或服务器地址写入示例文件、issue、日志或提交记录。

## 仓库结构

| 路径 | 用途 |
| --- | --- |
| `crates/gateway-model` | 配置、资源、冲突、计划和健康状态模型 |
| `crates/gateway-core` | 发现、状态库、渲染、应用、回滚和恢复 |
| `crates/gateway-app` | CLI、agent、HTTP API 与嵌入式 Vue UI |
| `packaging/` | 安装、卸载和 systemd 单元 |
| `scripts/ci/` | 手动架构边界检查与 Linux 网络烟雾测试脚本 |
| `docs/domain/` | 领域不变量、术语和业务边界 |
| `docs/architecture/` | 依赖地图与编码规则 |
| `docs/用户文档/` | 面向部署者的安装和真机验收 |

## 验证与验收

项目提供格式、测试、clippy、架构边界和网络环境检查脚本，可在部署前进行基础验证；GitHub Actions 会自动运行核心门禁。实际部署仍应结合目标环境完成必要的功能与稳定性验收。

权威项目规则和文档入口见 [`AGENTS.md`](AGENTS.md)。提交前建议执行：

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p gateway-app -- --local doctor
```

## 许可证

MIT，详见 [`LICENSE`](LICENSE)。
