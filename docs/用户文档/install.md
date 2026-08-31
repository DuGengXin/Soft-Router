# 安装

目标发行版：Debian 12、Ubuntu 22.04/24.04（systemd，x86_64 或 aarch64）。

```bash
cargo build --release -p gateway-app
sudo ./packaging/install.sh --bin ./target/release/gateway-kit
```

`install.sh --dry-run` 只打印将要做的事。安装**不会**改默认路由、不会 NAT、不会 `ip addr`、不会拉起 WireGuard/sing-box。

推荐拓扑：

```text
工作 PC → AP（关 DHCP/NAT）→ Linux LAN（默认 192.168.50.1/24）
Linux WAN（默认 192.168.40.2/24）→ 路由器 A → 国内直连
Linux WireGuard 或 sing-box VLESS → 用户 VPS（3x-ui/Xray 不在本仓库）
```

装完打印 `http://127.0.0.1:7676` 以及当时 `ip -4` 看到的地址（只读，不改地址）。  
在向导里填写 WAN/LAN，以及 WireGuard 地址/Endpoint/密钥，或在「隧道」页粘贴服务端生成的 VLESS/Xray 链接文本；敏感内容只写入 `/etc/gateway-kit/secrets.toml`（0600），API 不回显明文。两种境外出口二选一。LAN 监听必须同时配置 `ui_lan_token`，并在「访问令牌」页把同一令牌存进浏览器。

向导必须把运行模式改成「网关」，并配置 WireGuard 或 VLESS 链接，保存后再预览计划、确认应用。仍为观察模式时确认应用不会改网络。网关模式且检测到旧版 Router、PATH 中没有 `sing-box`（或启用 WG 却没有 `wg`、启用 DHCP 却没有 `dnsmasq`）时，计划为 blocked，不会改网络。确认后才会：

- `ip addr replace` 到你指定的网卡（仍不改主表 default）
- 加载 nft 表 `gateway_kit` 与策略表 `51820`
- 按需启动本产品的 sing-box / dnsmasq 单元

WAN 上原有的默认路由应仍指向路由器 A（国内直连）。本产品不替换它。

交叉编译 aarch64（在已安装目标工具链时）：

```bash
rustup target add aarch64-unknown-linux-gnu
cargo build --release -p gateway-app --target aarch64-unknown-linux-gnu
```

紧急旁路：`sudo gateway-kit disable --confirm` 或 UI「紧急旁路」。  
卸载：`sudo ./packaging/uninstall.sh`（保留 `/etc/gateway-kit` 配置）。

真机验收（Debian/Ubuntu 双网口，本仓库 Windows 开发机无法代替）：

1. `install.sh --dry-run` 后正式安装，`systemctl is-enabled gateway-kit` 为 enabled，默认路由未变。
2. Web 向导填 WAN/LAN/WG 或 VLESS 链接，预览 plan，确认 apply。
3. LAN 客户端能 DHCP，网关和 DNS 均为 Linux LAN 地址；DNS 由 sing-box 统一接管并按配置出口查询，国内 IP 出 WAN，境外经已配置的出口。LAN 的 TCP/UDP 53 会被网关强制接管。首次 apply 时 sing-box 经 WAN 从 jsDelivr 拉 geoip/geosite（不走 GitHub raw）；成功后缓存在 `/etc/gateway-kit/generated/sing-box-cache.db`，重启不必再下载。
4. 重启后恢复成功代际；`disable --confirm` 后 SSH 仍可用。

启动可靠性：确认应用后，Gateway-Kit 会启用控制面、转发兼容、sing-box 和 dnsmasq
单元。开机阶段先从已生成的 `forwarding.env` 动态读取 LAN/WAN 网卡名，并等待两块网卡出现
后再恢复代际；物理网卡或 Docker 初始化较慢时由 systemd 自动重试。sing-box/dnsmasq
进程异常会自动重启，连续健康检查失败则回滚到安全旁路。接口名或 DNS 等配置修改后，仍须
重新预览并确认应用，避免未应用配置影响现网。
