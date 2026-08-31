# 兼容 nftables 1.0.x 的表替换与 IPv4 DNAT

原因：目标服务器 nftables 1.0.2 不接受规则文件中的 `destroy table` 语句；`inet` 表中的 IPv4 DNAT 也需要显式地址族限定。

变更：apply 前仅 best-effort 删除 `inet gateway_kit`，随后加载完整规则表；生成的 DNAT 规则增加 `ip daddr 0.0.0.0/0`。不执行全局 nft flush，不触碰 Docker 或其他表。

验收：远端 Linux nftables 1.0.2 的 `nft -c` 测试通过。
