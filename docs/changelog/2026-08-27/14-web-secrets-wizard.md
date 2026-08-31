# Web 向导写入 secrets.toml

原因：产品要求 15 分钟内在浏览器填 WireGuard；此前密钥只能 SSH 改文件，向导无法闭环。  
意图：`GET /api/v1/secrets` 只返回是否已保存；`PUT` 合并补丁写入 0600 文件且不回显明文。LAN token 每次鉴权从文件读取。
