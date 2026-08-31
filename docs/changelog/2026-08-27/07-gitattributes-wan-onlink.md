# 安装脚本 LF 与 WAN 网关 on-link

原因：Windows 检出 CRLF 会让 Debian 上 `install.sh` 直接失败；WAN 网关地址此前未落到路由。  
意图：`.gitattributes` 锁定 shell LF；apply 只下发 `{gateway}/32 dev WAN`，仍禁止主表 default。
