# AGENTS.md — 图筛 PicSieve

Windows 桌面工具：清理本地大批量 Pixiv 图片（十几万张、平铺在少数几个大目录里）——找重复、按条件筛、安全地删。
仓库 `Landslide3154/picsieve`（公开）；本地目录 `D:\code\PicSieve`；技术栈 **Tauri 2 (Rust) + Vue 3 + TypeScript + SQLite**。

## 硬约束（用户明确要求，不得放松）

- **所有删除一律先移入隔离区** `D:\色图\_待确认删除`，随时可搬回；「彻底清空」必须二次确认，并在确认框里写明将删除的文件数与释放空间
- **扫描与算指纹阶段全程只读**，不得改动源文件
- **相似组绝不自动删除**，只给建议（建议保留：作品 ID 优先 → 路径更短 → 修改时间更早）
- 目标数据是用户的真实图库（`D:\色图` 下 PIXIV daily / daily r18，约 13.7 万文件 / 281 GB），**不要拿它做破坏性实验**

## 版本号

`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` **三处必须一致**。当前值以文件为准，不写死在这里。commit message 用 Conventional Commits（中英混合均可）。

## 构建与验证

```bash
pnpm install
pnpm build          # vue-tsc --noEmit && vite build（前端类型检查 + 打包）
pnpm tauri dev      # 开发模式
pnpm tauri build    # 生产构建（NSIS 安装包）
```

- Rust 侧：`cd src-tauri && cargo test` / `cargo clippy`
- **端到端验证（本项目的看家手段，改了界面/交互必跑）**：`tools/e2e-smoke.mjs`、`tools/e2e-quarantine.mjs` 经 **WebView2 远程调试口**真实点击界面并核对磁盘 sha256——启动应用时带
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`
  （`tools/cdp.mjs` 是共用的 CDP 封装；`.e2e/` 下放检查脚本与截图）
- **凡会真动磁盘的用例（移入隔离区等）必须跑沙箱**：启动时带 `PICSIEVE_DATA_DIR=<临时目录>`
  即可把数据库/设置指到别处。**改 `LOCALAPPDATA` 环境变量对 Tauri 无效**（它走系统「已知文件夹」API），
  那样会写进用户的真实库。参考 `.e2e/reset-sandbox2.ps1` + `.e2e/verify-round7.mjs`
- 造测试图：`tools/make_test_images.py`；图标：`tools/make_icon.py`

## 文档约定

- `docs/superpowers/specs/` 与 `docs/superpowers/plans/` 是设计依据（含 UI 改版规格）——**改行为前先读对应规格，改完同步更新**
- `README.md` 面向使用者（普通用户，不是开发者），写"能做什么/怎么用"
- 图标遵循全局 `~/.dsh/AGENTS.md` 第 7 节的视觉规范；参考图在 `docs/reference/icon-reference.png`

## 三条实测性能教训（别再踩回去）

1. **读图片头要「一次性读进 64KB 再在内存里解析」**——直接让 image crate 打开文件走解码器慢 13–67 倍（86,269 个真实文件：229.8s → 4.8–8.5s）
2. **大规模写库必须攒批事务**，逐条提交每条都要重解析 SQL
3. **给 `(status, short_side)` 之类加复合索引，会让 `ORDER BY size DESC LIMIT n` 失去有序扫描**——列表查询从 3ms 劣化成 1.7s

## 其它

- 全仓推送到 GitHub：改完立即 `git push`，遵循全局 `~/.dsh/AGENTS.md` 第 2 节的推送原则
- 会漂移的状态（当前版本、提交数、进度）见 DSH 记忆空间「图筛 PicSieve」，本文件只放规则
