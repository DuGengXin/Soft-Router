# netns LAN–WAN 烟雾与 wg strip

原因：真机双网口不在本开发机；`wg syncconf` 若吃带 Address 的 wg-quick 文件会在 Linux 上失败。  
意图：strip stdout 写入 `*.sync.conf` 再 syncconf；CI 用双 veth + `apply --confirm` 验证 LAN 客户端经 nft masquerade 打到 WAN 对端，并断言主机 default 不变。仍不是境外 WG/sing-box 分流或重启恢复。
