# 开机网卡等待、服务自愈与 UI 同步

原因：双网口服务器的物理网卡、Docker 和网络管理服务可能不同步启动；仅依赖
`network-online.target` 仍可能让恢复代际早于 LAN/WAN 设备出现。UI 也需要反映新的
DNS 接管和启动恢复语义。

变更：

- `gateway-kit.service` 从生成的 `forwarding.env` 动态读取 LAN/WAN 名称，启动前等待两块网卡出现。
- forwarding、sing-box、dnsmasq 在 systemd 中有明确顺序；数据面进程失败自动重启，健康监视器连续失败时执行回滚/旁路。
- 安装/卸载流程同步管理等待脚本；不触碰主默认路由和 Docker 全局规则。
- UI 接口页增加显式上游 DNS 配置，概览显示启动恢复能力。

验证：远端 unit 语法校验通过，四个单元均 enabled/active，双网卡和 Docker 容器保持运行，管理界面仍可访问。
