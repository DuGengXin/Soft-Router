# Gateway-Kit — Agent 目标提示词（North Star）

> 用法：整份复制为仓库根 `AGENTS.md`（唯一入口）。每个新会话第一件事是读本文件；与代码冲突时以本文件的不变量为准，代码服从不变量。  
> 仓库现状：**允许完全重建**。旧 crate / README / 配置示例都不是基线，只可当反例或素材，不得“为了兼容旧结构”而妥协产品目标。  
> 状态指针：`当前阶段 = P0 定盘`（尚无必须保留的实现）。完成 P0 后把本行改成真实阶段。

---

## 0. 你是谁、每次会话怎么开工

你是本仓库的长期执行代理，不是一次性脚本生成器。

**每个会话开始（强制）：**

1. 读本文件全文。
2. 读 `docs/domain/invariants.md`、`docs/architecture/dependency-map.md`（若尚不存在：本会话的任务就是先把定盘文件落地，而不是写业务功能）。
3. 读 `docs/plan/active/` 里最新 ACTIVE 计划的「当前任务」一节。
4. 用 `git status` 看清工作区；不丢弃用户未提交改动。
5. 用 3～6 条列出本会话要交付的可验证结果，然后才改代码。

**每个会话结束（强制）：**

1. 跑门禁（见 §7）；贴真实命令结果，禁止口头“应该过了”。
2. 更新本文件「状态指针」和计划文档的 checkbox。
3. 若引入结构性决策：写 `docs/changelog/YYYY-MM-DD/` 一条（原因 + 意图，不抄 git log）。
4. 未完成项写成「下一会话第一任务」，不要藏在聊天里。
5. **不要**擅自 `git commit` / `git push`，除非用户明确要求。

**会话目标粒度：** 一次会话只推进一个阶段内的一个可验收切片（例如“配置校验 + 单测”，而不是“把软路由做完”）。

---

## 1. 产品一句话

在用户现有 Linux 上安装一套 **Rust 控制面软路由套件（Gateway-Kit）**：双网口机器变成家用网关；工作 PC 的国内流量走上级路由器直连，境外流量经本机隧道到用户自己的 VPS；**不替换发行版、不劫持无关服务、安装后即可用 Web UI 管理，并开机自启。**

VPS 上的 `3x-ui` + Xray-core **不是本仓库的实现范围**。本产品只消费用户提供的出站凭证（WireGuard 配置和/或代理 URI）。

## 2. 目标用户体验（验收语言，不是实现细节）

干净的 Debian 12 / Ubuntu 22.04 或 24.04（x86_64 或 aarch64，systemd）上：

1. **易安装：** 一条官方安装命令（或本地 `install.sh`）装好二进制、目录、systemd；缺依赖则打印发行版包名后退出，不偷偷加第三方源（sing-box 可提供**显式可选**下载）。
2. **安装安全：** 安装过程 **不得** 修改默认路由、不得启用 NAT、不得拉起代理。装完只处于 **观察模式**。
3. **Web UI：** 浏览器能打开管理界面；向导可完成网卡、地址、DHCP、隧道、分流；可预览计划、确认应用、一键旁路。
4. **开机自启：** `gateway-kit.service` enable；有成功代际则恢复之；无成功代际或健康检查失败则旁路，**SSH 不断**。
5. **兼容：** 与 NetworkManager、Docker/Podman 共存时默认不接管它们的对象；冲突则 **阻止 apply** 并说人话，而不是覆盖。
6. **拓扑：** 工作 PC → AP（关 DHCP/NAT）→ Linux LAN；Linux WAN → 路由器 A（互联网）。Linux 经 WireGuard 或 sing-box VLESS 到 VPS。

默认网段：LAN `192.168.50.0/24`，WAN `192.168.40.0/24`。若用户填写看起来像公网的网段（例如 `192.111.40.0/24`），必须警告，允许高级继续，不得静默当成私网。

## 3. 硬性不变量（违反即错误，必须改设计而不是加特例）

1. **控制面 / 数据面分离：** Rust 只做发现、校验、渲染、plan/apply/rollback、健康、UI/API。转发、NAT、封装、代理协议由内核、nftables、WireGuard、sing-box（或同等独立数据面二进制）执行。禁止在 Rust 里实现 TCP/IP 栈或 Xray 协议栈。
2. **只拥有自己的对象：** 只管理名为 `gateway_kit` 的 nft 表、独立路由表（默认 id `51820`）、本产品生成的 wg/sing-box/dhcp 配置与 systemd 单元。禁止改 NetworkManager 全局配置、docker0、UFW/firewalld 策略来“曲线救国”。
3. **默认不改主默认路由。** 网关模式必须用户在 UI/CLI 明确确认。本机 SSH、Web UI、agent 自身流量默认 bypass 代理。
4. **无确认不变更。** 顺序永远是 discover → plan → 用户确认 → apply → health → 失败 rollback。`plan` 在任何 OS 可运行；`apply` 仅 Linux 且需确认。
5. **密钥隔离：** 密钥只在 `secrets.toml`（或等价，权限 0600），不进 git、不进普通 config、不进 Web 日志。
6. **Web 默认不暴露：** 默认 bind `127.0.0.1`。监听 LAN 必须认证。WAN 不得默认放行 UI 端口。
7. **可卸载、可旁路：** `disable`/`uninstall` 能撤走本产品对象，尽量恢复观察前状态；失败也不得留下半套 nft 规则。
8. **可测试：** 所有解析/渲染/校验必须可在无网、非 Linux、无 root 下单测（命令执行必须可注入）。真正改系统的路径必须可跳过或隔离。

## 4. 明确非目标（agent 不得自行开做）

- 管理或安装 VPS 上的 3x-ui / 面板账号体系
- 做成 OpenWrt/OPNsense 发行版
- 多 WAN、负载均衡、广告过滤、流媒体解锁全家桶
- Electron / 依赖 Node 才能构建的生产前端
- 用 Docker 作为默认安装形态（本机就是路由器，容器网络是冲突源）
- 为了“复用旧仓库模块划分”而保留错误抽象

若用户下一句明确要求其中某项，先更新本文件非目标列表，再写代码。

## 5. 推荐结构（绿地；可在 P0 微调但须先改依赖图）

发布形态：**一个二进制** `gateway-kit`（内含 agent + CLI 子命令 + 嵌入式 Web 静态资源）。

逻辑 crate（名称可改，依赖方向不可改）：

```text
gateway-model          # 类型、校验、错误码；零 I/O
    ↑
gateway-core           # 发现、渲染、plan、apply、SQLite 代际；I/O 可注入
    ↑
gateway-app            # CLI + HTTP + systemd 入口 + 嵌入 UI
```

`gateway-app` 不得把领域规则写成 Axum handler 里的散装 if。UI 只是控制面的皮肤。

数据面（系统包或可选下载，不链进 Rust）：`iproute2`、`nftables`、`wireguard-tools`、`sing-box`；DHCP 用成熟组件（dnsmasq 或 kea），由控制面生成配置。

## 6. 阶段门（做完前一扇门，再开下一扇）

开发模型：**阶段门 + 阶段内小迭代**。版本 SemVer，`0.x` 允许破坏性变更，但安装路径与单元名保持稳定：`/etc/gateway-kit`、`/var/lib/gateway-kit`、`gateway-kit.service`、nft 表 `gateway_kit`。

| 阶段 | 交付 | 门禁（全部真跑） |
|---|---|---|
| P0 定盘 | AGENTS / 术语 / 不变量 / 依赖图 / 编码规则 / changelog 规范；可空实现 | 文档无占位；链接有效 |
| P1 模型 | 配置与 plan 类型 + 校验 + TOML 往返测试 | `cargo test -p gateway-model` |
| P2 发现 | 只读 doctor/discover；冲突策略；Windows 也能测 parser | 注入 runner 的 fixture 测试 |
| P3 安装骨架 | install.sh、systemd、观察模式常驻、`/api/health` | 容器内 dry-run 安装；enable 成功；**零网络变更** |
| P4 真 Plan | 渲染 nft/wg/sing-box/dhcp；`plan --explain` | 快照测试 |
| P5 Apply | 确认后变更 + 代际 + rollback + disable | Linux netns 或 root 集成；失败必回滚 |
| P6 分流 | 国内直连、境外走 WG；本机 bypass | 对测：国内不进隧道 |
| P7 Web UI | 向导闭环；LAN 认证 | 不靠 CLI 能 apply/旁路 |
| P8 兼容发布 | NM/Docker 矩阵；aarch64；用户文档 | 清单全绿 → 打 alpha tag |

**当前允许写代码的最高阶段 = 状态指针中的阶段 + 其下一阶段。** 禁止跳做 P7 界面充数，或先写 apply 再补 plan。

## 7. 工程门禁与编码

- Rust 2024 edition，`clippy --all-targets -- -D warnings`，`fmt --check`，`cargo test --workspace`。
- 公共 API 有 rustdoc；crate 头有 `//!`。
- 错误用 `thiserror`；禁止在库代码 `unwrap` 当控制流。
- 禁止无必要 `unsafe`。
- 新增依赖先问用户（或先改 dependency-map + allow-list），默认拒绝“顺手加 crate”。
- 前端：嵌入静态 HTML/CSS/少量 JS；发布构建不得要求用户机器上的 npm。
- 回复用户用简体中文。

## 8. Agent 决策门（必须停下来问人）

出现以下情况不要自作主张：

- 改变不变量、监听默认值、nft 表名、安装路径
- 让 apply 在无确认时执行
- 引入会改主默认路由的“自动修复”
- 新增数据面组件（例如把 sing-box 换成别的）
- 大范围删用户未提交文件
- 提交 / 推送 / 改 git config

## 9. 质量口令（防 agent 腐化）

- 没有测试的渲染器 = 未完成。
- 能在开发者 Windows 上跑的逻辑，不准只在 Linux 上才能测。
- 发现与变更必须分模块，避免“一个 god.rs 又读 ss 又 nft add”。
- 文档只在权威位置写事实；入口文件只导航 + 短引。
- 用户要速度时：缩小切片，而不是削弱不变量。

## 10. 本会话若从零开始，第一优先级

若仓库仍是旧实验代码或空壳：

1. 落地 P0 定盘文件（本提示词进 `AGENTS.md`，不变量/依赖图进 `docs/`）。
2. 再建 workspace 骨架（model → core → app）。
3. 不要移植旧代码除非它通过不变量审查。

用户说“开始实现 / 执行计划”之前，默认只讨论或只改定盘，不把半套网关写进树里。
