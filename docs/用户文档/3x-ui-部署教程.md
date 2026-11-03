# 3x-ui 部署教程

本文说明如何在一台 VPS 上部署 [3x-ui](https://github.com/Mhsanaei/3x-ui) 面板，发布 VLESS + REALITY 节点，并接入 Gateway-Kit 作为网关的境外出口。

Gateway-Kit 只负责网关侧的控制面与数据面编排，**不包含 3x-ui 或 Xray 服务端**。本文的服务端步骤在 VPS 上执行，与网关主机无关。

## 拓扑

```text
LAN 客户端
    │
    ▼
Gateway-Kit 网关（sing-box TProxy + 分流）
    │  VLESS / REALITY（本机 Xray 客户端）
    ▼
VPS：3x-ui 面板 + Xray 服务端
    │
    ▼
Internet
```

网关侧只需要一条 VLESS 链接作为隧道凭证，不需要在网关上输入 UUID、公钥等参数。

## 1. 前提条件

| 项目 | 要求 |
|---|---|
| 系统 | Debian 11/12 或 Ubuntu 20.04/22.04/24.04，x86_64 或 arm64 |
| 权限 | root |
| 网络 | 能访问 GitHub（安装脚本需要） |
| 端口 | 至少一个对外端口供节点使用；面板端口建议不对外暴露 |
| 域名 | 可选。REALITY 不需要自己的证书或域名 |
| 内存 | 空闲 300MB 以上（3x-ui 面板 + Xray 约占用 100–150MB） |

> 如果 VPS 访问 GitHub 受限，见「附录 A：离线安装」。

## 2. 系统准备

```bash
apt update && apt upgrade -y
apt install -y curl socat ufw ca-certificates

# 时间必须准确，REALITY 对时钟偏差敏感
apt install -y systemd-timesyncd
timedatectl set-ntp true
timedatectl status | grep -E "System clock synchronized|NTP service"
```

**确认 `System clock synchronized: yes` 再继续**。时钟漂移超过几十秒会导致握手失败，且报错信息很有迷惑性。

可选：启用 BBR（对高延迟链路有帮助）

```bash
echo "net.ipv4.tcp_congestion_control = bbr" > /etc/sysctl.d/99-bbr.conf
modprobe tcp_bbr
sysctl -p /etc/sysctl.d/99-bbr.conf
```

## 3. 部署前评估线路

在动手之前，先确认这台 VPS 的回国链路值不值得用。**用对照组**，不要只看单台延迟。

在网关主机上执行：

```bash
probe() {
  local host=$1 port=$2 label=$3 ok=0 sum=0
  for i in $(seq 1 10); do
    s=$(date +%s%N)
    if timeout 5 bash -c "exec 3<>/dev/tcp/$host/$port" 2>/dev/null; then
      ms=$(( ($(date +%s%N)-s)/1000000 )); ok=$((ok+1)); sum=$((sum+ms))
      printf " %.0fms" "$ms"
    else printf " FAIL"; fi
  done
  echo; echo "  => $label: ok=$ok/10 avg=$([ $ok -gt 0 ] && echo $((sum/ok)) || echo -)ms"
}

probe <候选VPS的IP> 443        "候选 VPS"
probe 13.107.42.14 443         "基线：微软"
probe 223.5.5.5 53             "基线：国内"
```

判读标准：

| 现象 | 结论 |
|---|---|
| 候选 VPS 平均 < 150ms、成功率 10/10 | 可用 |
| 候选 VPS 明显差于微软基线 | 线路或 IP 有问题，换 IP 或换机房 |
| 只有该 IP 差、其它境外目标正常 | **该 IP 被针对性限速/干扰**，不是整体网络问题 |
| 所有境外目标都差 | 本地国际出口拥塞 |

**务必在晚高峰（20:00–23:00）复测**。白天测什么都好看。

## 4. 在已有网站的服务器上共存部署

如果这台 VPS 已经跑着门户网站、并配好了 SSL 证书，3x-ui 完全可以共存。原则是：**只做加法，不碰现有站点**。

### 4.1 部署前盘点

```bash
# 现有监听端口，避免撞车
ss -lntp

# 内存余量
free -h

# 当前防火墙规则，先看清楚再动
ufw status verbose        # 或者
nft list ruleset | head -50
iptables -S | head -30

# 现有 Web 服务
systemctl list-units --type=service --state=running | grep -Ei 'nginx|apache|httpd|caddy'
```

把结果记下来，尤其是防火墙当前允许的端口。**后面只做增量放行，绝不要 `ufw enable` 或重置规则**——那会把你现有的 80/443 一起关掉，网站立刻下线。

### 4.2 端口规划

| 用途 | 建议 |
|---|---|
| 网站 HTTP/HTTPS | 保持 80/443 不动 |
| 3x-ui 面板 | 高位随机端口，**不对外开放**，用 SSH 隧道访问 |
| VLESS 节点 | 单独高位端口，例如 `24614`，对外放行 |

面板和节点端口都要避开 `ss -lntp` 里已有的端口。

### 4.3 关于现有 SSL 证书

**REALITY 不需要证书，所以现有证书在这个方案里用不上。** 这不是浪费——REALITY 的伪装性优于真证书的 TLS，因为证书是公开可探测的。

只有这两种情况会用到证书：

| 方案 | 是否用证书 | Gateway-Kit 支持 |
|---|---|---|
| VLESS + REALITY + TCP | 不需要 | ✅ 支持 |
| VLESS + WebSocket + TLS（反代 443） | 需要 | ❌ 暂不支持 |

所以现阶段按 REALITY 部署，证书留给网站继续用。

### 4.4 安装时的注意事项

```bash
bash <(curl -Ls https://raw.githubusercontent.com/mhsanaei/3x-ui/master/install.sh)
```

安装过程**只新增** `/usr/local/x-ui`、`/etc/x-ui` 和一个 systemd 单元，不会修改 Nginx/Apache 配置，也不会接管 80/443。

唯一要注意的是它会安装自己的一份 Xray 到 `/usr/local/x-ui/bin/`，与系统里已有的 `xray` 二进制**互不影响**。

### 4.5 防火墙：只加不删

```bash
# 先看当前规则
ufw status numbered

# 只追加节点端口，不要 enable、不要 reset
ufw allow 24614/tcp

# 面板端口不对外，只在本地监听
# 如果安装时误开了，删掉那一条：
ufw delete allow <面板端口>/tcp
```

如果 3x-ui 安装脚本提示要自动配置防火墙，选择**手动配置**。

云厂商的安全组同理：只加一条入站规则放行节点端口。

### 4.6 部署后验证网站未受影响

```bash
# 网站仍然正常
curl -sS -o /dev/null -w "site_http=%{http_code}\n"  https://<你的域名>/
curl -sS -o /dev/null -w "site_https=%{http_code}\n" https://<你的域名>/

# 证书未变
echo | openssl s_client -connect <你的域名>:443 -servername <你的域名> 2>/dev/null | openssl x509 -noout -subject -dates

# 节点端口正常监听
ss -lntp | grep 24614

# 两个服务互不干扰
systemctl is-active nginx 2>/dev/null; systemctl is-active x-ui
```

### 4.7 资源与排障边界

| 现象 | 判断 |
|---|---|
| 网站变慢 | 看 `free -h` 和 `top`，可能是内存不足导致 swap |
| 网站打不开但 3x-ui 正常 | 检查防火墙是否误删了 80/443 规则 |
| 3x-ui 打不开但网站正常 | 面板端口未放行，或路径输错（这是预期行为） |
| 两者都正常但节点不通 | 走第 13 节的节点排错流程 |

## 5. 安装 3x-ui

```bash
bash <(curl -Ls https://raw.githubusercontent.com/mhsanaei/3x-ui/master/install.sh)
```

安装过程会提示设置：

| 提示项 | 建议 |
|---|---|
| 自定义面板端口 | **不要用 80/443**，选一个高位随机端口，例如 `31284` |
| 自定义 Web 路径 | 设置一个随机字符串，例如 `/a7f3k2`。留空则用默认路径 |
| 管理员用户名 | 不要用默认 `admin` |
| 管理员密码 | 强密码，至少 16 位 |

常用管理命令：

```bash
x-ui            # 进入交互菜单
x-ui status     # 服务状态
x-ui restart    # 重启面板（会一并重启 Xray）
x-ui log        # 查看日志
x-ui setting -port 31284 -webBasePath /a7f3k2   # 非交互修改端口与路径
```

验证面板已启动：

```bash
systemctl status x-ui --no-pager | head -5
ss -lntp | grep x-ui
```

## 6. 首次登录与安全设置

浏览器访问 `http://<VPS-IP>:31284/a7f3k2`。

登录后立即做三件事：

1. **修改密码**（面板 → 设置 → 用户管理）
2. **确认面板端口不对外暴露**（见第 9 节防火墙）
3. **开启 Xray 流量统计**（面板 → 设置 → 启用流量统计与账号统计）

面板需要远程访问时，用 SSH 隧道而不是直接开放端口：

```bash
ssh -L 8080:127.0.0.1:31284 root@<VPS-IP>
# 然后浏览器打开 http://127.0.0.1:8080/a7f3k2
```

## 7. 创建 VLESS + REALITY 入站

面板 → **入站列表** → **添加入站**

### 基础设置

| 字段 | 值 |
|---|---|
| 备注 | 任意，例如 `vless-reality` |
| 协议 | `vless` |
| 监听 IP | 留空（监听全部） |
| 端口 | 节点端口，例如 `24614`。避开 80/443 |
| 传输 | `tcp`（默认） |

### 客户端设置

| 字段 | 值 |
|---|---|
| ID | 点「生成」创建 UUID |
| Flow | 留空，或选 `xtls-rprx-vision`（见下方说明） |
| 安全 | `reality` |

### REALITY 设置

| 字段 | 说明 |
|---|---|
| Dest | 伪装目标，例如 `www.microsoft.com:443` |
| ServerNames | 与 Dest 域名一致 |
| 私钥 / 公钥 | 点「Get New Cert」生成一对，**公钥要记下来** |
| ShortIds | 点生成，或手填 8 位十六进制 |
| SpiderX | 留空或填 `/` |

**Dest 的选择要求**：

- 支持 TLS 1.3 和 HTTP/2
- 境外站点
- 不要用 `www.cloudflare.com` 这类大量用户共用的目标，容易撞特征
- 常用且稳定的：`www.microsoft.com`、`www.apple.com`、`www.lovelive-anime.jp`

验证目标是否合格：

```bash
openssl s_client -connect www.microsoft.com:443 -tls1_3 </dev/null 2>/dev/null | grep -E "Protocol|ALPN"
```

应能看到 `TLSv1.3` 和 `h2`。

### 关于 Flow 的取舍

| 选择 | 优点 | 代价 |
|---|---|---|
| 留空 | 客户端可以启用 **Mux**，大量短连接时表现更好 | 无 |
| `xtls-rprx-vision` | 抗主动探测更强 | **不能与 Mux 同时使用** |

Gateway-Kit 的 Xray 出口默认启用 Mux（`concurrency: 8`）。**服务端一旦设置 vision flow，客户端必须关闭 Mux**，否则连接失败。在丢包链路上 Mux 收益明显，建议保持 Flow 留空。

保存后回到入站列表，确认状态为运行中。

## 8. 生成客户端链接

在入站列表点击该入站的**二维码 / 分享**图标，复制 `vless://` 开头的链接：

```text
vless://<UUID>@<VPS-IP>:24614?encryption=none&security=reality&sni=www.microsoft.com&fp=chrome&pbk=<公钥>&sid=<shortId>&type=tcp#vless-reality
```

| 参数 | 含义 |
|---|---|
| `pbk` | REALITY 公钥 |
| `sid` | ShortId |
| `sni` | 伪装域名，必须与服务端 ServerNames 一致 |
| `fp` | 客户端 TLS 指纹，建议 `chrome` |
| `flow` | 服务端启用 vision 时才会出现 |

**这条链接就是网关侧唯一需要的凭证**，请完整保存。

## 9. 防火墙

```bash
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp
ufw allow 24614/tcp
ufw enable
ufw status verbose
```

已有网站的服务器见第 4.5 节，**只追加规则，不要重置**。

如果 VPS 前面还有云厂商的安全组，同步放行节点端口。

## 10. 接入 Gateway-Kit

在网关侧：

1. 浏览器打开网关管理界面（默认 `http://<网关IP>:7676`）
2. 进入 **隧道** 页
3. 出口方式选择 **VLESS 链接**
4. 粘贴第 8 步复制的 `vless://` 链接
5. 保存
6. 进入 **计划与变更**，预览后确认应用

应用后网关会生成：

| 文件 | 作用 |
|---|---|
| `/etc/gateway-kit/generated/xray.json` | Xray 客户端，在 `127.0.0.1:10808` 提供 SOCKS5 |
| `/etc/gateway-kit/generated/sing-box.json` | sing-box，境外流量经 SOCKS 转给 Xray |

链接只写入 `/etc/gateway-kit/secrets.toml`（权限 0600），不会出现在计划 JSON 或 API 响应里。

## 11. 验证

### 在 VPS 上确认服务端正常

```bash
ss -lntp | grep 24614
curl -sS -m 10 -o /dev/null -w "vps_egress=%{http_code} t=%{time_total}\n" https://www.google.com/generate_204
```

如果 VPS 自己访问外网就慢，问题在 VPS 的网络，与 3x-ui 配置无关。

### 在网关上确认隧道可用

```bash
curl -sS -x socks5h://127.0.0.1:10808 -m 15 -o /dev/null \
     -w "egress=%{http_code} t=%{time_total}\n" \
     https://www.google.com/generate_204
```

期望 `egress=204`。

### 在网关界面上看

概览页的 **出口探测** 卡片应显示「通」，并附带探测时间。该探测每 60 秒刷新一次。

### 从 LAN 客户端确认

连上网关 WiFi 后打开境外站点，同时观察网关日志：

```bash
journalctl -u gateway-kit-singbox -f -o cat | grep -E "dns: exchanged|outbound/(direct|socks)"
```

正常应看到境外域名解析出真实 IP，并走 `outbound/socks[proxy-out]`；国内域名走 `outbound/direct[direct]`。

## 12. 加固建议

| 项目 | 做法 |
|---|---|
| 面板端口 | 不对外开放，用 SSH 隧道 |
| 面板路径 | 随机字符串，不要用 `/` 或 `/panel` |
| 密码 | 16 位以上，不与其它服务复用 |
| SSH | 禁用密码登录，改用密钥 |
| 系统更新 | 开启 `unattended-upgrades` |
| 面板登录保护 | 3x-ui 设置中启用 fail2ban 集成 |

## 13. 排错

### 面板打不开

```bash
systemctl status x-ui --no-pager
ss -lntp | grep -E "31284"
ufw status verbose
journalctl -u x-ui -n 50 --no-pager
```

常见原因：端口没放行、路径输错、服务未启动。

### 节点连不上（客户端握手失败）

按顺序排查：

1. **时间同步**：`timedatectl status` 必须显示 synchronized
2. **参数一致性**：`sni` 等于服务端 ServerNames；`pbk` 是公钥不是私钥；`sid` 匹配
3. **Flow 匹配**：服务端设了 vision，客户端必须有 flow，且不能开 Mux
4. **端口可达**：从客户端侧 `telnet <VPS-IP> 24614`

### 网关日志出现 `reality verification failed`

| 原因 | 排查 |
|---|---|
| 客户端与服务端 Xray 版本差异过大 | 两边都用近期版本 |
| UUID / 公钥 / shortId 不匹配 | 重新复制链接 |
| 服务端设了 vision 而客户端没设 | 对齐 flow 参数 |
| 系统时间偏差 | 校准时间 |

### 能连但速度慢、频繁超时

先判断瓶颈位置，别急着改配置：

```bash
for h in 13.107.42.14:443 8.8.8.8:53 <VPS-IP>:24614; do
  s=$(date +%s%N)
  timeout 5 bash -c "exec 3<>/dev/tcp/${h%:*}/${h#*:}" 2>/dev/null \
    && echo "$h $(( ($(date +%s%N)-s)/1000000 ))ms" || echo "$h fail"
done
```

其它境外目标快、只有该 VPS 慢 → 问题在 VPS 所在线路或 IP，与 3x-ui 无关。考虑换 IP、换机房或换线路。

### 流量统计不显示

面板 → 设置 → 确认已启用「流量统计」和「账号统计」，然后 `x-ui restart`。

## 14. 维护

```bash
# 备份面板数据库（默认在 /etc/x-ui/x-ui.db）
cp /etc/x-ui/x-ui.db /root/x-ui-backup-$(date +%F).db

# 更新 3x-ui
x-ui update

# 查看 Xray 版本
/usr/local/x-ui/bin/xray-linux-amd64 -version 2>/dev/null || xray version
```

修改入站配置前先备份数据库，并在客户端留存可用的链接。

## 附录 A：离线安装

如果 VPS 无法访问 GitHub：

1. 在可联网的机器上下载 3x-ui 安装包与 Xray 二进制
2. 通过 SFTP/SCP 上传到 VPS
3. 解压到 `/usr/local/x-ui` 并配置 systemd 单元

使用社区镜像时务必先核对脚本来源与完整性，不要盲目执行未知来源的脚本。

## 附录 B：与 Gateway-Kit 的兼容性说明

| 项目 | 说明 |
|---|---|
| 支持的传输 | 当前仅 `tcp`。WebSocket、gRPC 等会明确报错，不会静默忽略 |
| 安全类型 | `reality` 或 `tls` |
| 必需参数 | `uuid`、`sni`、`pbk`、`sid`（REALITY） |
| Mux | 客户端默认启用；服务端启用 vision flow 时需在代码中关闭 |
| 链接解析 | 未知的查询参数会被忽略，可在「计划与变更」页面核对 |

## 附录 C：连接地址用 IP 而不是域名

3x-ui 生成链接时，主机名可能取面板的对外地址。如果那里填的是域名，链接就会变成：

```text
vless://<UUID>@<你的域名>:<端口>?...
```

网关会把这个主机名直接写进 Xray 出站的 `address`，于是**每建立一次隧道连接都要先解析一次域名**：

```text
Xray → 系统解析器 → 国内 DNS → 拿到 IP → 再连
```

### 实测差异

同一台 VPS、同一时刻：

| 连接地址 | 握手延迟 | 出口成功率 |
|---|---|---|
| 域名 | 86ms、101ms、**1107ms** | 7/8 |
| IP | 79–95ms | **10/10** |

域名的抖动来自解析开销，偶尔会卡到秒级，表现为「能连但偶发超时」。

### 建议做法

把链接里的主机名换成 IP：

```text
改前：vless://<UUID>@<你的域名>:<端口>?...
改后：vless://<UUID>@<VPS-IP>:<端口>?...
```

**不影响 REALITY 伪装**。容易混淆的是这两个字段：

| 字段 | 作用 | 是否需要改成 IP |
|---|---|---|
| `address`（连接地址） | 连哪台机器的哪个端口 | ✅ 建议用 IP |
| `serverName`（伪装 SNI） | 对外伪装的 TLS 目标 | ❌ 保持境外域名 |

REALITY 的隐蔽性来自 `serverName`，与连接地址无关。

代价是 VPS 换 IP 时要同步改链接。如果用的是固定 IP 的云主机，这个代价可以接受。

## 附录 D：资源受限环境的部署

在已有业务的低配机器上部署时，内存是最容易出问题的地方。

### D.1 先加 swap

```bash
df -h /                                  # 确认有 2G 空间
fallocate -l 2G /swapfile
chmod 600 /swapfile
mkswap /swapfile
swapon /swapfile
echo '/swapfile none swap sw 0 0' >> /etc/fstab
sysctl vm.swappiness=10
echo 'vm.swappiness=10' >> /etc/sysctl.conf
free -h
```

`swappiness=10` 让它只做兜底，不参与日常调度。

swap 不是内存的替代品——它的意义是**内存见底时不会触发 OOM killer 杀掉业务进程**。

### D.2 用数据决定 Java 堆大小

不要凭感觉调 `-Xmx`，先测量：

```bash
# OC=老年代容量 OU=老年代已用  MC=Metaspace容量 MU=Metaspace已用
jstat -gc <PID> 1000 3
```

判读：

| 现象 | 结论 |
|---|---|
| `OU` 远小于 `OC` | 堆上限设大了，可以下调 |
| `OU` 接近 `OC`，多次 GC 后不降 | 真实需要，或存在泄漏 |
| `MU` 只有几十 MB | `MaxMetaspaceSize` 设太大，可收紧 |
| `FGC` 次数持续增长 | 堆不够，需要调回去 |

一个参考：若老年代实际只用 60–80MB，`-Xmx256m` 足够；`MaxMetaspaceSize` 设 128–192MB 通常即可。

改完重启应用，用 `jcmd <PID> VM.flags` 确认生效，再观察两三天。

### D.3 MySQL 也是大头

```bash
mysql -e "SHOW VARIABLES LIKE 'innodb_buffer_pool_size';"
```

数据量不大时，这个值往往是按「机器内存的百分比」自动设的，可以适当调小。改动前先备份配置文件。

## 附录 E：更新 Gateway-Kit 后必须重启服务

一个容易遗漏的坑：**替换二进制文件不会自动影响正在运行的常驻进程**。

```bash
install -m 0755 target/release/gateway-kit /usr/local/bin/gateway-kit
```

执行完上面这条命令，磁盘上已经是新版本，但 `gateway-kit agent` 进程仍在运行旧代码。此后通过 Web 界面触发的 apply，生成配置用的还是**旧逻辑**。

### 判断方法

```bash
# 进程启动时间
ps -eo pid,lstart,cmd | grep "[g]ateway-kit agent"

# 二进制文件时间
ls -l --time-style=full-iso /usr/local/bin/gateway-kit
```

**进程启动时间早于二进制时间，就说明它还是旧的。**

### 正确流程

```bash
install -m 0755 target/release/gateway-kit /usr/local/bin/gateway-kit
systemctl restart gateway-kit
sleep 10
ps -eo pid,lstart,cmd | grep "[g]ateway-kit agent"   # 确认启动时间已更新

# 确认生成物符合预期后再重启数据面
python3 -c "import json;d=json.load(open('/etc/gateway-kit/generated/xray.json'));print(d['outbounds'][0].keys())"
systemctl restart gateway-kit-xray
```

重启控制面会触发开机恢复流程，按上一次成功代际重新生成配置，不会丢失已应用的设置。

### 验证生成物

改完配置后，用生成出来的文件核对预期项，比看日志更快：

```bash
# Xray 出站：地址、端口、mux
python3 -c "
import json
d=json.load(open('/etc/gateway-kit/generated/xray.json'))
o=d['outbounds'][0]; v=o['settings']['vnext'][0]
print('address:', v['address'], '| port:', v['port'], '| mux:', bool(o.get('mux')))
"

# sing-box：DNS 服务器与分流规则
python3 -c "
import json
d=json.load(open('/etc/gateway-kit/generated/sing-box.json'))
print('dns:', [s.get('type') for s in d['dns']['servers']])
print('rules:', [r.get('server') for r in d['dns']['rules']])
"
```
