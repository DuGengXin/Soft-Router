# install 打印已有地址

原因：产品安装清单要求打印本机 UI 与已检测到的 LAN 地址；脚本原先只打 127.0.0.1。  
意图：安装结束只读 `ip -4 -o addr show`，不 `ip addr add/replace`。
