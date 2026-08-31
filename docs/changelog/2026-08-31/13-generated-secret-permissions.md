# 生成的出口配置权限收紧

原因：VLESS sing-box 配置包含 UUID 与 Reality 参数，不能按普通生成文件权限落盘。

变更：apply 写入 `sing-box.json` 时在 Linux 上显式设置 0600；原有包含 WireGuard 私钥的配置继续保持 0600。相关权限由 apply 测试覆盖。
