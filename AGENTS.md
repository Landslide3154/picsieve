# 图筛 PicSieve — 开发约定

> 跨项目通用规则见全局 `C:\Users\godis\.dsh\AGENTS.md`，本文件不重复，只写这个项目独有的东西。

## 1. 这个项目是什么

Windows 桌面工具：清理本地大批量 Pixiv 图片（大量文件平铺在少数几个大目录里，实际规模以目录为准）——找重复、按条件筛、安全地删。
**Tauri 2 (Rust) + Vue 3 + TypeScript + SQLite**。仓库 `Landslide3154/picsieve`（公开）；本地 `D:\code\PicSieve`。

关键位置：`src-tauri/src/scanner.rs` → `fingerprint.rs`/`phash.rs` → `grouper.rs` 是「扫描 → 算指纹 → 分组」主链；`quarantine.rs` 是隔离区；`db.rs` + `query.rs` 是 SQLite 与列表查询；`src/` 是 Vue 界面；`tools/cdp.mjs` 是 e2e 共用的 CDP 封装。

## 2. 常用命令（可直接复制执行）

| 目的 | 命令 |
| --- | --- |
| 安装依赖 | `pnpm install` |
| 类型检查 + 前端打包 | `pnpm build`（= `vue-tsc --noEmit && vite build`） |
| 开发模式 | `pnpm tauri dev` |
| 生产构建（NSIS 安装包） | `pnpm tauri build` |
| Rust 测试 / 静态检查 | `cd src-tauri; cargo test` / `cargo clippy` |
| 造测试图 | `python tools\make_test_images.py [目标目录]`（默认 `.e2e/images`） |
| 生成图标 | `python tools\make_icon.py` |

**端到端验证（本项目的看家手段，改了界面/交互必跑）**：`tools/e2e-smoke.mjs`、`tools/e2e-quarantine.mjs` 经 **WebView2 远程调试口**真实点击界面并核对磁盘 sha256。启动应用时带 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`；`.e2e/` 下放检查脚本与截图。

**凡会真动磁盘的用例（移入隔离区等）必须跑沙箱**：启动时带 `PICSIEVE_DATA_DIR=<临时目录>` 即可把数据库/设置指到别处；参考 `.e2e/reset-sandbox2.ps1` + `.e2e/verify-round7.mjs`。

## 3. 硬约束（必须 / 禁止）

- 必须：**所有删除一律先移入隔离区** `D:\色图\_待确认删除`，随时可搬回；「彻底清空」必须二次确认，并在确认框里写明将删除的文件数与释放空间。
- 必须：**扫描与算指纹阶段全程只读**，不得改动源文件。
- 必须：**相似组绝不自动删除**，只给建议（建议保留：作品 ID 优先 → 路径更短 → 修改时间更早）。
- 必须：版本号在 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` **三处保持一致**（当前值以文件为准）。
- 必须：commit message 用 Conventional Commits（中英混合均可）。
- 禁止：拿用户的真实图库（`D:\色图` 下的 PIXIV daily / daily r18）做破坏性实验，规模以实际目录为准。
- 禁止：用 `LOCALAPPDATA` 环境变量做测试隔离——对 Tauri 无效（原因见第 4 节）。

## 4. 架构边界与因果（从代码看不出为什么）

- **读图片头要「一次性读进 64KB 再在内存里解析」**：直接让 image crate 打开文件走解码器慢 13–67 倍（实测 229.8s → 4.8–8.5s）。
- **大规模写库必须攒批事务**：逐条提交时每条都要重解析 SQL。
- **不要给 `(status, short_side)` 之类加复合索引**：会让 `ORDER BY size DESC LIMIT n` 失去有序扫描，列表查询从 3ms 劣化成 1.7s。
- **测试隔离只能用 `PICSIEVE_DATA_DIR`**：`LOCALAPPDATA` 对 Tauri 无效，它走系统「已知文件夹」API，改了会写进用户的真实库。

## 5. 版本与发布

- 版本号写在三处（`package.json` / `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml`），必须同时改、保持一致；本文件不写当前值。
- 发版：`pnpm tauri build` → NSIS 安装包，产物在 `src-tauri\target\release\bundle\nsis\PicSieve_<版本>_x64-setup.exe`（`bundle.targets = ["nsis"]`）。
- 会漂移的当前值（当前版本号、提交数、进度）→ 记忆空间「图筛 PicSieve」，本文件不写。

## 6. 已知坑

- 界面/交互改动只跑 `pnpm build` 就收工 → 类型检查测不出真实点击路径 → 必跑第 2 节的 e2e（并带 `--remote-debugging-port=9222`）。
- 破坏性用例（移入隔离区、清空）直接对着真实库跑 → 无法回退 → 必须先起 `PICSIEVE_DATA_DIR` 沙箱。
- 只改 `package.json` 的版本号 → 三处不一致会让安装包/关于页版本对不上 → 三处同改。

## 7. 指针

- 全局规则：`C:\Users\godis\.dsh\AGENTS.md`（git 推送见 §2、shell 与编码见 §3、MCP 见 §5、图标见 §7）——本节不复述。
- 记忆空间：图筛 PicSieve（当前版本、进度与实测快照）。
- 设计依据：`docs/superpowers/specs/`、`docs/superpowers/plans/`（含 UI 改版规格）——**改行为前先读对应规格，改完同步更新**；`README.md` 面向普通用户，写「能做什么 / 怎么用」。
- 图标参考图：`docs/reference/icon-reference.png`（视觉规范见全局 §7）。
