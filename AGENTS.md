# Gateway-Kit

Linux 软路由**控制面**。旧代码可推倒。入口文档只导航。

- 不变量：`docs/domain/invariants.md`
- 术语：`docs/domain/glossary.md`
- 依赖：`docs/architecture/dependency-map.md`
- 编码：`docs/architecture/coding-rules.md`

**命令：** `cargo test --workspace`；`cargo clippy --all-targets -- -D warnings`；`cargo fmt --check`；`cargo run -p gateway-app -- --local doctor`

**当前阶段：** P0–P7 控制面已落地；P8 已在真实双网口 Linux 主机完成 Gateway apply 与 VLESS 出口验证，默认路由/Docker 对象保持不变。仍待接入真实 LAN 客户端做 DHCP/NAT/分流端到端验收。未经用户要求不 commit。
