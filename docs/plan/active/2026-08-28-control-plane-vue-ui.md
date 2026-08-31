# 控制面 Web：Vue 3 + Element Plus 落地计划

> 状态: `IMPLEMENTED`（代码已嵌入；真机 UI 验收在网关机器）  
> 类别: 计划  
> 日期: 2026-08-28  
> **For agentic workers:** 按任务顺序落地；本机不做 API/网关联测。

**Goal:** 用与 1Panel 同族的 Vue 3 + Vite + Element Plus 重做控制面四页，开发可用 npm；`gateway-kit` 发布仍为单二进制，**运行时不要求 npm**。

**Architecture:** UI 源码在 `crates/gateway-app/ui/`。`build.rs` 在编译 `gateway-app` 时 `npm ci && npm run build`，产物 `ui/dist` 用 `rust-embed` 嵌入。Axum 只服务 `/` 与 hashed `/assets/*`。信息架构不变：壳 + 概览 / 接口与分流 / 计划与变更 / 访问令牌。客户端用 hash 路由，避免多路径 fallback。

**Tech Stack:** Vue 3（Composition API）· TypeScript · Vite · Vue Router（hash）· Element Plus · `fetch`（不引入 axios/Pinia/i18n）· Node ≥ 20（仅构建）· `rust-embed`

## Global Constraints

- 不变量：discover → plan → 确认 → apply；密钥只进 `secrets.toml`；非回环 bind 必须 token。
- `docs/architecture/coding-rules.md`：发布 UI 运行时不得要求 npm；构建 `gateway-app` **需要** Node。
- 禁止新 crate `gateway-web`；UI 仍属 `gateway-app`。
- 不改 JSON API 契约；前端适配现有 `/api/v1/*`。
- 本机不跑真实 agent 联测；验收在网关机器。
- 未经用户要求不 commit。

---

## 1. 选型（相对 Tabler 手写页）

| 项 | 选择 | 原因 |
|---|---|---|
| 框架 | Vue 3 | 与 1Panel 同栈，表单/表格控制面最省 |
| 组件库 | Element Plus | 1Panel 同库；暗色、表单、抽屉、确认框现成 |
| 构建 | Vite | 标准 SPA；产物可嵌入 |
| 状态 | 无 Pinia | 四页 + 轮询，composable 足够 |
| HTTP | `fetch` | 与现 `app.js` 一致，少依赖 |
| 路由 | `createWebHashHistory` | 嵌入静态服务只需 `/` + `/assets` |
| 布局 | `el-container` + 小屏 `el-drawer` | 保留竖栏/抽屉信息架构，不新发明页 |

不用 React/shadcn：与「宝塔/1Panel」心智模型和中文表单密度不匹配。不用 Naive UI：生态与 1Panel 组件习惯差一截。

---

## 2. 目录（评审修订稿，不再按字段拆文件）

```text
crates/gateway-app/ui/
  package.json
  package-lock.json
  vite.config.ts
  tsconfig.json
  index.html
  src/
    main.ts
    App.vue                 # 壳：侧栏 / 抽屉 / 顶栏 / <router-view>
    api/client.ts           # token 头、错误
    overview/OverviewPage.vue
    wizard/WizardPage.vue
    plan/PlanPage.vue
    access/AccessPage.vue
  dist/                     # 构建产物，gitignore，由 build.rs 生成

crates/gateway-app/src/build.rs
crates/gateway-app/src/http.rs   # 去掉 Tabler 逐文件路由
```

删除（被取代）：`crates/gateway-app/web/index.html`、`app.js`、`style.css`、`vendor/tabler.*`。

页面刷新策略（避免上帝 `refresh()`）：

| 页 | 何时打 API |
|---|---|
| overview | 进入时 + 5s 轮询 `status` `monitor` `events` |
| wizard | 进入时 `config` `secrets` `status`（网卡列表）；保存后重拉 |
| plan | 进入时不自动 apply；按钮触发 `plan`/`apply`/`rollback`/`disable` |
| access | 只读写 `localStorage` |

---

## 3. 嵌入与 CI

1. `gateway-app/build.rs`：Windows 用 `npm.cmd`，其它用 `npm`；`npm ci`（无 lock 则 `npm install`）+ `npm run build`；`cargo:rerun-if-changed=ui/`。
2. `rust-embed`：`#[folder = "ui/dist"]`。
3. 路由：`GET /` → `index.html`；`GET /assets/{*path}` → embed；未知静态 404。API 仍 `/api/v1/*`。
4. CI `linux` 与 `aarch64` 均 `actions/setup-node@v4`（Node 22）后再 `cargo`。
5. 运行时：网关机器 **没有 npm 也能跑** 已编好的二进制。

Debug：改 UI 后需 `npm run build` 或再 `cargo build`（build.rs 会编）。可选 `npm run dev` + Vite proxy 到 agent，**不作为本机验收**。

---

## 4. 任务

### Task 1: 计划与基线文档

- Create: 本文件  
- Modify: `coding-rules.md`、`dependency-map.md`、changelog、`.gitignore`、CI  
- 验收：文档写明「运行时无 npm / 构建要 Node」

### Task 2: Vite 工程与四页

- 暗色 + 主色接近现有 `#3ecf8e`  
- 四页行为对齐现 `app.js`（字段名、confirm 文案、观察模式拦截 apply）  
- 验收：`npm run build` 在 `ui/dist` 产出 `index.html` + assets

### Task 3: Rust 嵌入

- `build.rs` + `http.rs` SPA  
- 删旧 `web/`  
- 验收：`cargo test -p gateway-app`、`cargo clippy -p gateway-app --all-targets -- -D warnings`（**不**起 agent、不打真实网关）

---

## 5. 本机验证（非功能）

- `npm ci && npm run build`（在 `ui/`）  
- `cargo test --workspace`（含 build.rs）  
- `cargo clippy --workspace --all-targets -- -D warnings`  
- `cargo fmt --all -- --check`  

不做：浏览器打真实 `/api`、改 nft、起 `--local agent`。

网关机器验收：四页打开、监控刷新、向导保存、计划预览/确认/回滚/旁路、LAN token。

---

## 评审 1 — 架构与不变量

- 通过：API 不变；apply 仍要 `confirm: true`；密钥不进 config。  
- 通过：不新 crate，依赖方向不变。  
- 修订：hash 路由，避免 `fallback` 把 `/api` 误当成页面（API 注册在静态之前即可，hash 更省事）。  
- 修订：不要 Pinia/axios/i18n/ECharts。

## 评审 2 — 构建、嵌入、CI

- 风险：`include_str` 不能 glob → 用 `rust-embed`，正当新依赖。  
- 风险：`ui/dist` 未生成时 rustc 失败 → **必须** build.rs 先 npm。  
- 风险：aarch64 job 现在没 Node → 必加 setup-node。  
- 修订：coding-rules 改成「运行时无 npm；编译 gateway-app 需要 Node ≥ 20」。  
- 修订：`.gitignore` 加 `crates/gateway-app/ui/node_modules/` 与 `ui/dist/`。  
- 否决：把 `dist` 提交进 git 当免 Node 构建——和「CI 编前端」重复且易脏。

## 评审 3 — 分块粒度与替换范围

- 通过：四页各一 SFC，不做 `mode.ts`/`wan-lan.ts`。  
- 通过：一个 `App.vue` 壳，不做 `shell/index.html` 当 HTTP 入口（Vite 入口是 `ui/index.html`）。  
- 修订：目录名 `plan/` 对齐 `data-section`/hash `#/plan`，不用 `change/`。  
- 否决：HTML 碎片 fetch 组装。  
- 否决：继续保留 Tabler 双栈。  
- 注意：`refresh()` 拆到按页拉取，否则换框架仍是一锅粥。

## 评审结论（锁定）

按第 2–4 节执行。三轮未发现需推翻的选型；仅收紧依赖与路由。
