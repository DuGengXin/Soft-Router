# VLESS/Xray 链接作为可选境外出口

意图：服务端 Xray 配置由用户维护，控制面只提供一个敏感的服务端链接文本位置，不把服务端实现或具体参数硬编码进项目。

变更：`secrets.toml` 的 `proxy_uri` 支持 VLESS URI；控制面校验并渲染为 sing-box VLESS outbound，WireGuard 与 VLESS 二选一。VLESS 的 UUID、Reality 公钥等内容不会出现在 API 脱敏计划中。未填写链接时仍使用原有 WireGuard 路径。

边界：只支持 VLESS URI 的常用 TCP/TLS/Reality 参数（服务器、端口、UUID、sni/serverName、pbk、sid、fp、flow）；不实现 Xray 服务端、不主动生成或修改服务端参数。真实链接由用户自行粘贴到 `/etc/gateway-kit/secrets.toml` 的 `proxy_uri` 或 Web「隧道」页。
