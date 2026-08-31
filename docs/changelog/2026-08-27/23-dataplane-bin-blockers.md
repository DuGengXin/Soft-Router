# 网关模式缺数据面二进制则阻断

确认 apply 时若 PATH 里没有 sing-box（或 wg/dnsmasq），原先会先改 nft/WG 再因 tproxy 健康失败回滚。发现阶段在网关模式把缺失标为 blocker，观察模式仅警告，避免半应用。
