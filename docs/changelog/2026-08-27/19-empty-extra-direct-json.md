# 空 extra_direct 不生成非法 ip_cidr

原因：向导可清空额外直连网段；原先 `join` 会得到 `"ip_cidr": [""]`，sing-box 可能拒配或匹配异常。  
意图：无有效 CIDR 时省略该 rule，只保留 `ip_is_private` 与可选 geoip。
