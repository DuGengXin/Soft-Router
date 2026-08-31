# china_direct 规则集走 jsDelivr 并缓存

国内首启 apply 若从 `raw.githubusercontent.com` 拉 geoip/geosite，常直接导致 sing-box 起不来、tproxy 健康失败回滚。改为 jsDelivr，并用 `cache_file` + 单元 `WorkingDirectory` 把成功下载缓存在 generated 目录，后续开机可离线用缓存。
