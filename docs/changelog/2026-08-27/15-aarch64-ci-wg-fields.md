# aarch64 交叉编译与向导 AllowedIPs

原因：产品承诺 x86_64 与 aarch64；向导未暴露 WG listen/AllowedIPs 时只能改 toml。  
意图：CI 增加 `aarch64-unknown-linux-gnu` 链接构建；Web 向导可填 ListenPort 与 AllowedIPs。仍不是真机跑 aarch64。
