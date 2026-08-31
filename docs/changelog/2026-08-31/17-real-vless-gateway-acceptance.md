# 真实双网口 VLESS Gateway 验收

验收环境：真实双网口 Linux 主机（具体接口名不记录在仓库）；WAN 默认网关与原有地址保持不变。

结果：真实 `proxy_uri` 读取成功，生产 plan 为 ready；确认 apply 成功；sing-box 1.13、dnsmasq、策略路由和 `gateway_kit` nft 表均 active；临时本机 mixed 入站经生成的 VLESS outbound 访问 `www.google.com:443` 返回 204。默认路由和 Docker nft 表未被改写，临时测试文件已删除。

剩余：尚未有独立 LAN 客户端在现场完成 DHCP、国内直连和境外分流的端到端流量验收。
