# apply 分配地址且不改主默认路由

原因：网关模式需要把 LAN/WAN 地址落到指定网卡，但不得改写主表 default。  
意图：`ip addr replace` + 安装 sysctl.d 掉落；install 仍为零网络变更；uninstall 撤走 nft/51820/数据面单元。
