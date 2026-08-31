# Gateway-Kit 产品开发计划

> 状态: `ACTIVE`（真实双网口 Gateway apply 与 VLESS 出口已验收；仍待真实 LAN 客户端端到端验收）  
> 类别: 计划（唯一权威：本文件；代码现状以 README 与 crate 为准）  
> 日期: 2026-08-27

**目标:** 在 `gateway-model` / `gateway-core` / `gateway-app` 单二进制控制面上继续演进，交付一套易安装、可 Web 管理、开机自启、尽量不接管原 Linux 业务的软路由控制面；数据面仍交给 Linux 内核、nftables、WireGuard 和 sing-box。

**不做:** 推倒重写；Rust 自己做 TCP/IP 转发；第一版管理 VPS 上的 3x-ui；第一版接管 NetworkManager / Docker / 主默认路由。

---

## 1. 产品成功标准

用户在一台双网口 Debian/Ubuntu 机器上，按文档执行安装后，应能在 15 分钟内达到：

1. 一条安装命令完成二进制、systemd、目录与 Web 入口。
2. 浏览器打开 Web UI，看到网卡发现、冲突、向导。
3. 填写 WAN/LAN、WireGuard 或 VLESS 出口、分流策略后，可 `plan` 预览、确认后 `apply`。
4. 重启后服务自动起来；上次成功配置继续生效；上次失败则进入旁路/观察模式，SSH 仍可用。
5. 工作 PC 经 LAN 上网：国内直连路由器 A，**可访问上级局域网（WAN 网段，MASQUERADE）**，境外走 VPS。

## 2. 约束（全阶段默认生效）

- 平台：Linux + systemd；首发 `x86_64` 与 `aarch64`。
- 发行版：Debian 12、Ubuntu 22.04 / 24.04 为一级目标；其他发行版仅“尽力”。
- 数据面组件：`iproute2`、`nftables`、`wireguard-tools`、`sing-box`（独立二进制，不链进 Rust）。
- 防火墙只使用独立 nft 表，默认名 `gateway_kit`（替换当前示例里的 `router`）。
- 策略路由使用独立 table id（建议 `51820`），不改写主表默认路由，除非用户在向导中明确选择“接管网关模式”。
- 本机 SSH / Web UI / systemd 管理流量 bypass。
- 任何变更：`discover` → `plan` → 确认 → `apply` → 健康检查；失败自动回滚。
- 密钥不进 `config.toml`，只进 `secrets.toml`（权限 `0600`）。
- `0.x` 接口可破坏性变更，但安装路径与 systemd 单元名保持稳定：`/etc/gateway-kit`、`gateway-kit.service`。

## 3. 推荐拓扑（配置默认值）

```text
工作 PC
  │
Wi-Fi 路由器 B（AP/交换，关 DHCP/NAT）
  │
Linux 软路由
  ├─ LAN  192.168.50.1/24   DHCP 由 gateway-kit 管理（可选）
  └─ WAN  192.168.40.2/24   网关 192.168.40.1
              │
        路由器 A → 国内直连
Linux ── WireGuard 或 VLESS/sing-box ── VPS（3x-ui + Xray-core）
```

若现场仍使用 `192.111.40.0/24`：向导必须警告“这是公网地址空间，可能冲突”，允许高级用户继续，但默认建议改为 `192.168.40.0/24`。

VPS 不在本仓库管理范围内。Rust 只消费用户提供的出站凭证（VLESS URI 或 WireGuard 配置）。

## 4. 架构

继续演进，不新开仓库。

| 组件 | 职责 |
|---|---|
| `gateway-model` | 配置、计划、代际、健康、冲突的类型与校验 |
| `gateway-core` | 发现、plan 生成、渲染器、apply/rollback、SQLite 代际 |
| `gateway-app` / `gateway-kit` | 合并后的 CLI、常驻 agent、HTTP API、嵌入式 Web UI 与健康循环 |
| `gateway-app/ui` | 嵌入式前端源码；发布运行时零 Node，构建阶段需要 Node 20+ |
| `packaging/` | `install.sh`、systemd unit、依赖检测、卸载 |

运行时形态：**一个发布二进制 `gateway-kit`**（agent 内嵌 CLI 子命令也可，但对外文档只教一个入口），避免用户面对四个可执行文件。

数据流：

```text
Web UI / CLI
    → Unix socket 或 本机 HTTP（仅 LAN/loopback）
    → gateway-kit agent
    → gateway-core（plan/apply）
    → 写出 wg*.conf / nft / sing-box.json
    → 调用 wg-quick/nft/systemctl（注入 CommandRunner）
    → 健康检查；失败回滚上一代
```

Web 技术选型（已定）：**Axum + 嵌入式静态前端**。理由：单二进制、运行时离线可装、兼容旧浏览器、安装体积小；构建阶段使用 Vite/Node 20+，发布物不依赖 Node。不做 Electron。

安装选型（已定）：**官方 `install.sh` + systemd**；后续再补 `.deb`。不把 Docker 作为默认安装方式（这台机器本身要当路由器，容器网络会增加冲突面）。

## 5. 用户体验：安装与开机

### 5.1 安装

```bash
curl -fsSL https://example.invalid/gateway-kit/install.sh | sudo bash
# 或本地：
sudo ./packaging/install.sh --bin ./gateway-kit
```

安装脚本必须：

1. 检测 systemd、nft、ip；缺依赖则打印发行版安装命令后退出（不静默乱装第三方仓库，sing-box 可提供可选下载开关）。
2. 安装二进制到 `/usr/local/bin/gateway-kit`。
3. 创建 `/etc/gateway-kit`、`/var/lib/gateway-kit`、`/var/log/gateway-kit`、`/var/backups/gateway-kit`。
4. 若无配置，写入安全默认：`proxy.enabled=false`，不启用转发。
5. 安装并 `enable --now gateway-kit.service`。
6. 打印本机访问地址：`http://127.0.0.1:7676` 以及检测到的 LAN 地址（若已有）。
7. **不**在安装时 apply 路由/NAT。

### 5.2 systemd

`gateway-kit.service`：

- `Type=simple`，`Restart=on-failure`
- `After=network-online.target nss-lookup.target`
- `Wants=network-online.target`
- 以 root 运行（需要 nft/wg）；后续可拆 capabilities，不作为 0.x 阻塞项
- `ExecStart=/usr/local/bin/gateway-kit agent`
- 环境：`GATEWAY_KIT_CONFIG=/etc/gateway-kit/config.toml`

开机策略：

| 上次代际状态 | 开机行为 |
|---|---|
| 无成功 apply | 仅发现 + Web UI，不改网络（旁路） |
| 成功 apply | 应用最后成功代际，再做健康检查 |
| 上次 apply 失败且已回滚 | 保持回滚后状态，UI 标红 |
| 健康检查失败超过阈值 | 自动 rollback 到上一成功代际；再失败则 `emergency_bypass` |

`gateway-kit disable --confirm` / UI「紧急旁路」：卸掉本产品 nft 表与策略路由，停止 sing-box/wg 由本产品拉起的单元，保留 Web UI。

### 5.3 Web UI 页面（0.x 必做）

1. 仪表盘：健康、WAN/LAN、WG、分流模式、冲突数
2. 发现/冲突：复用现有 doctor
3. 向导：网卡选择、地址、DHCP 开关、WG、出站
4. 计划预览：将要改的文件、nft、路由、服务
5. 应用/回滚/旁路
6. 日志与最近事件（只读）

认证：默认本机 loopback 免密；监听 LAN 时必须设置密码（或 token）。禁止默认 `0.0.0.0` 无认证暴露。

## 6. 分阶段交付

### Phase 0 — 定盘与配置模型（当前缺口）

**状态：** doctor/discover、真 plan、配置校验、安装元数据和 WireGuard 分流模型已落地；剩余重点是真机验收与出口能力闭环。

交付：

- 扩展 `AppConfig`：`wireguard`、`routing`（直连 CIDR/geosite、代理默认）、`ui`（bind/port）、`mode`（observe | gateway）
- `config.example.toml` 与现网默认对齐：nft 表名 `gateway_kit`；WAN 示例改为 `192.168.40.0/24` 并注释公网地址风险
- 配置校验：网段不重叠、DHCP 落在 LAN、WG 端口不冲突、公网 RFC1918 警告
- 单元测试覆盖 TOML 往返与校验错误

验收：`gateway-kit doctor` 行为不变；无配置文件时仍只读可运行。

### Phase 1 — 真 Plan（仍不执行）

交付：

- `ChangePlan` / `Generation` 模型
- 渲染器（纯函数）：nftables、wg-quick、sing-box JSON、dnsmasq/kea 片段、systemd drop-in
- `gateway-kit plan` 输出结构化 JSON + 人类可读摘要
- 有 blocker 时 plan 状态为 `blocked`，不生成可执行步骤

验收：离线 fixture 生成的 plan 快照测试稳定；Windows 开发机也能跑测试（CommandRunner 注入保持）。

### Phase 2 — 易安装与开机自启（仍默认旁路）

交付：

- 合并发布入口 `gateway-kit`
- `packaging/install.sh` / `uninstall.sh`
- `gateway-kit.service`
- `gateway-kit agent` 常驻 + HTTP 健康 `GET /api/v1/health`
- 开机进入 observe 模式
- CI：`cargo test` + 在 Debian bookworm 容器里跑 install 脚本的 dry-run

验收：干净 Debian 上 install 后 `systemctl is-enabled gateway-kit` 为 enabled；卸载后无残留单元与 nft 表。

### Phase 3 — Apply / Rollback（网关能力第一次上线）

交付：

- 仅 Linux：`apply` 需要 `--confirm` 或 UI 二次确认
- 代际写入 SQLite `generations`
- 应用顺序：备份 → sysctl 转发（仅 LAN 相关）→ nft 表 → 地址/DHCP → WG → sing-box → 健康检查
- 回滚与 `disable`
- 本机 bypass 规则（SSH 22、UI 端口、已建立 conntrack 可选）

验收：虚拟机双网卡拓扑中，LAN 客户端能 DHCP；NAT 出 WAN；`disable` 后客户端不再经本机转发。失败注入（sing-box 起不来）必须回滚。

### Phase 4 — 分流与 VPS 出口

交付：

- WireGuard 或 VLESS 到 VPS
- sing-box rule-set：国内直连、境外走已配置出口
- Linux 本机默认直连
- 健康：WG handshake、出站探测（国内/国外各一）

验收：国内 IP 不进隧道；境外进隧道；拔掉 WG 时境外失败但国内与 SSH 仍可用。

### Phase 5 — Web UI

交付：

- 嵌入式 UI + `/api/v1/*`
- 向导走完可 apply
- 实时状态（短轮询即可，0.x 不做复杂 WS）
- LAN 监听强制认证

验收：不看 CLI 也能完成 Phase 3+4 的配置与旁路。

### Phase 6 — 兼容性加固与 0.1.0

交付：

- 冲突矩阵：UFW/firewalld 默认 block；NM 存在时不写 `NetworkManager.conf`；Docker 网桥只观察
- 发行版矩阵文档与手工 checklist
- aarch64 交叉或原生构建
- 用户文档：安装、拓扑、故障旁路、与 3x-ui 的分工

验收：在“已有 Docker 的 Ubuntu 22.04”上 install 不破坏现有容器网络；doctor 给出明确 blocker。

## 7. 建议 crate / 文件落点

| 路径 | 职责 |
|---|---|
| `crates/gateway-model/src/config.rs` | 从 `lib.rs` 拆出配置类型 |
| `crates/gateway-model/src/plan.rs` | ChangePlan、Action、GenerationStatus |
| `crates/gateway-core/src/render/` | nft/wg/sing-box 渲染 |
| `crates/gateway-core/src/apply.rs` | 仅 Linux cfg 的执行器 |
| `crates/gateway-core/src/health.rs` | 应用后检查 |
| `crates/gateway-app/src/http.rs` | Axum |
| `crates/gateway-app/ui/` | `index.html` 等静态文件 |
| `packaging/gateway-kit.service` | systemd |
| `packaging/install.sh` | 安装 |
| `docs/用户文档/install.md` | 安装手册（Phase 2 才写正文） |

现有 `discovery.rs` 保持只读；apply 不得调用 discover 里的解析器去改系统。

## 8. 兼容性矩阵（一级）

| 项目 | 要求 |
|---|---|
| OS | Debian 12, Ubuntu 22.04/24.04 |
| init | systemd |
| 防火墙 | nftables；iptables-nft 可检测；iptables-legacy + UFW 默认 block apply |
| 网络管理 | 允许 NetworkManager 管理 WAN 地址（observe 友好）；gateway 模式只认用户指定网卡 |
| 容器 | Docker/Podman 存在时不修改 docker0；DHCP 不得占用已监听 67 |
| 架构 | x86_64, aarch64 |
| 内核 | WireGuard 模块或 wireguard-tools |

明确非目标：OpenWrt 发行版替换、Windows 主机当软路由、无 systemd 环境。

## 9. 测试策略

- 模型与渲染：纯单测 + 快照（任何开发机）。
- 发现：现有 CommandRunner fixture，继续加样例。
- apply：`#[cfg(target_os = "linux")]` 集成测试，用 network namespace 或 skip-if-not-root。
- 安装：容器内 install dry-run。
- 门禁：`cargo fmt`、`clippy -D warnings`、`cargo test --workspace`。

## 10. 风险

| 风险 | 缓解 |
|---|---|
| 一装就改默认路由导致 SSH 断 | 安装默认 observe；apply 前检查管理源 IP |
| sing-box/3x-ui 双头管理 | VPS 只跑 3x-ui；Linux 只跑 sing-box 客户端 |
| 公网网段当内网 | 校验警告 + 文档 |
| Web UI 暴露到 WAN | 默认 bind 127.0.0.1；WAN 防火墙不放行 UI 端口 |
| 范围膨胀 | Phase 5 之前不做订阅转换、多 WAN、广告过滤 |

## 11. 建议立即开工顺序

只把 **Phase 0 → Phase 1 → Phase 2** 当作当前开发主线。Web UI 依赖真实 plan/apply 契约，提前做界面会返工。

里程碑版本建议：

- `0.1.0-alpha`：Phase 2（能装、能开、能看 doctor）
- `0.2.0-alpha`：Phase 3（能当基础网关）
- `0.3.0-beta`：Phase 4（分流 + WG）
- `0.4.0-beta`：Phase 5（Web 向导）
- `0.1.0` 正式号在 Phase 6 后按 SemVer 再定（0.x 允许调整）
