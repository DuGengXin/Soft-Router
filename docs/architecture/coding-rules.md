# 编码规则

- 公共项 `///`，crate 头 `//!`。
- 库内不用 `unwrap` 做控制流；错误用 `thiserror`。
- 禁止无必要 `unsafe`。
- 命令执行经 `CommandRunner`。
- 新增依赖须与本文件及 Cargo.toml 同步；默认少依赖。`gateway-app` 允许 `rust-embed` 以嵌入 Vite 产物；`gateway-model` 允许 `base64` 以解析 WG 配置粘贴。
- 发布 UI **运行时**不得要求 npm；编译 `gateway-app` 需要 Node.js ≥ 20（`build.rs` 调用 `npm ci` / `npm run build`）。
