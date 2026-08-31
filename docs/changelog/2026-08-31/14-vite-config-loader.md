# 前端构建使用 Vite runner 配置加载器

原因：默认 bundle loader 会在 `node_modules/.vite-temp` 写入临时配置文件；在部分受限 Windows 工作树中会导致 Rust workspace 构建失败。

变更：UI production build 使用 `vite build --configLoader runner`，不改变运行时产物或 Linux/CI 的清理输出策略。远端 Linux release 构建已验证通过。
