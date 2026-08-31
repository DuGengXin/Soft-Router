# Linux nft -c 校验渲染表

原因：Windows 无法证明 nft 表语法能被内核工具接受；`flush table` 在表尚不存在时会失败。  
意图：渲染改为 `destroy table` 后再定义表；Linux CI 安装 nftables 后对渲染结果跑 `nft -c -f`。仍不是双网口分流实测。
