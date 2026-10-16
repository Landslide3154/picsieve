# 图筛 PicSieve — 开发约定

> 跨项目通用规则见全局 `~/.dsh/AGENTS.md`，本文件不重复，只写这个项目独有的东西。
> 本机专属的路径、数据规模、版本与实测数字在 `AGENTS.local.md`（不提交）；长篇取证在 `docs/`。

## 1. 这个项目是什么

Windows 桌面工具：清理本地大批量 Pixiv 图片（大量文件平铺在少数几个大目录里，规模以目录为准）——找重复、按条件筛、安全地删。
**Tauri 2 (Rust) + Vue 3 + TypeScript + SQLite**。仓库 `Landslide3154/picsieve`（公开）。
主链：`scanner.rs` → `fingerprint.rs`/`phash.rs` → `grouper.rs`（扫描 → 算指纹 → 分组）；删除一律经 `quarantine.rs` 隔离区。

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

**端到端验证（改了界面/交互必跑；类型检查测不出真实点击路径）**：`tools/e2e-smoke.mjs` 驱真实 DOM 与库，`tools/e2e-quarantine.mjs` 逐文件核对磁盘 sha256；脚本与截图放 `.e2e/`。手法本身（WebView2 远程调试口 + CDP，其它 Tauri 项目可复用）见全局 `AGENTS.local.md` §7。

**凡会真动磁盘的用例（移入隔离区等）必须跑沙箱**：启动时带 `PICSIEVE_DATA_DIR=<临时目录>`，把数据库与设置指到别处。

## 3. 硬约束（必须 / 禁止）

- 必须：**所有删除一律先移入隔离区**；「彻底清空」必须二次确认，确认框里写明将删除的文件数与释放空间。
- 必须：**扫描与算指纹阶段全程只读**，不得改动源文件。
- 必须：**相似组绝不自动删除**，只给建议（建议保留：作品 ID 优先 → 路径更短 → 修改时间更早）。
- 必须：版本号在 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` **三处保持一致**。
- 禁止：拿用户的真实图库做破坏性实验；测试隔离只能用 `PICSIEVE_DATA_DIR`（`LOCALAPPDATA` 无效，见 §6）。

## 4. 架构边界与因果（从代码看不出为什么）

- **只读扫描 + 只进隔离区**：源图库不可再生，任何不可回退的删除都不可接受。
- **读图片头必须「一次性读进 64KB 再在内存里解析」**：判断尺寸/格式只需文件头，而 `image` crate 打开文件走完整解码器，慢一个数量级以上。凡「只想知道图片尺寸」的批量扫描同理。
- **大规模写库必须攒批事务**：逐条提交时每条都要重解析 SQL 并单独提交一次，吞吐差若干倍。
- **不要给 `(status, short_side)` 之类加复合索引**：它会让 `ORDER BY size DESC LIMIT n` 失去有序扫描（改走该索引再排序），列表首屏从毫秒级劣化成秒级；列表查询的索引列顺序必须与 `ORDER BY` 对齐。

## 5. 版本与发布

- 版本号三处同改（见 §3）。
- 发版：`pnpm tauri build` → NSIS 安装包，产物在 `src-tauri\target\release\bundle\nsis\`（`bundle.targets = ["nsis"]`）。
- **发布与推送等用户明确口令**（见全局 `AGENTS.md` 的「Git 与 GitHub」节）；本地 `git commit` 照常，做完汇报「已提交、未推送」。

## 6. 已知坑

- 用 `LOCALAPPDATA` 改数据目录 → Tauri 走系统「已知文件夹」API，不读该变量，会写进用户真实库 → 用应用自己的开关 `PICSIEVE_DATA_DIR`（`src-tauri/src/lib.rs`）。
- 用 `Process.MainWindowHandle` 量窗口 → 进程内还有辅助/隐藏窗，取到的可能不是主窗 → 枚举全部顶层窗口**按面积挑最大**的。
- 把限流做成 Rust 侧信号量 → 排队中的 `spawn_blocking` 每条都占一条阻塞线程，前端一次要几十张时阻塞线程池占满、CPU 归零、命令全卡死 → 限流放前端排队（`src/api.ts`）。
- 缩略图请求不做取消 → 虚拟列表滚过十万张会堆积十几万个过期请求，队列堵死 → 必须可取消（格子 `onUnmounted` 后跳过，含排队中的）。
- 十万级列表用深响应式 `ref` → 每条记录包成 Proxy，内存与访问开销翻数倍 → `shallowRef` + 整表替换，统计增量维护（否则每批整表重算退化成 O(n²)）。

## 7. 指针

- 实测取证：`docs/性能实测.md`（性能/并发/内存/线程数与数据规模的完整数字）。
- 设计依据：`docs/superpowers/specs/`、`docs/superpowers/plans/`——**改行为前先读对应规格，改完同步更新**；`README.md` 面向普通用户。
