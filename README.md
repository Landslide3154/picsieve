# 图筛 PicSieve

一个 Windows 桌面工具，用来清理本地大批量的 Pixiv 图片：找重复、按条件筛、安全地删。

面向的场景很具体：图片总量十几万张、平铺在少数几个大文件夹里，Windows 资源管理器已经
打不开也理不动了。

## 现在能做什么

- **扫描**：遍历指定文件夹，读出文件大小、修改时间、真实格式与宽高；已扫过且没变的文件会跳过
- **指纹**：内容指纹（BLAKE3，先用文件大小预筛，大小唯一的文件根本不读内容）、
  视觉指纹（64 位 pHash）、灰度分数（越小越接近黑白灰）；中途退出可以续算
- **重复**：
  - 「一模一样」按内容指纹分组，自动给出建议保留哪张（含作品 ID 优先 → 路径更短 → 修改时间更早）
  - 「看着像」按 pHash 汉明距离聚类，阈值可调；同作品不同页不会被误判成重复
- **筛选**：分辨率、体积、格式、只看灰阶 / 只看重复 / 只看读不出的，另有按路径模糊搜索；
  拖动条件实时显示命中数量
- **删除**：所有删除都是「移到隔离区」，随时可搬回；「彻底清空」要二次确认，
  确认时明确写出将删除的文件数与释放空间

## 安全说明

- 扫描与指纹阶段**全程只读**，不写、不移动、不改名任何图片
- 删除即移动到隔离区；同一块盘上是原子重命名，不占额外空间
- 跨盘时走「复制 → 校验内容指纹 → 删除源文件」，校验不过就中止并保留原文件
- 每批移入、搬回、清空都有记录，可追溯
- 隔离区目录不允许设在扫描目录里面（软件会直接拒绝），避免下次扫描把待删文件又捞回来

## 开发

需要 Node 22+、pnpm、Rust（MSVC 工具链）、WebView2。

```powershell
pnpm install
pnpm tauri dev          # 开发模式，热重载
```

后端测试与静态检查：

```powershell
cd src-tauri
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

前端类型检查与构建：

```powershell
pnpm build              # 含 vue-tsc 类型检查
```

## 打包

```powershell
pnpm tauri build
```

产物在 `src-tauri/target/release/`：

- `picsieve.exe` —— 免安装，双击即用
- `bundle/nsis/*.exe` —— 安装包（按当前用户安装，不要求管理员权限）

## 自动冒烟（可选，开发用）

`tools/` 下有一套通过 WebView2 远程调试口真实点击界面的端到端脚本：

```powershell
# 1) 造一批受控测试图片（一模一样的两份、同图缩略版、灰阶、彩图、同作品两页、坏文件、txt）
python tools/make_test_images.py .e2e/images

# 2) 带调试口启动
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'
pnpm tauri dev

# 3) 另一个终端里跑
node tools/e2e-smoke.mjs .e2e/images           # 扫描 → 指纹 → 图库 → 筛选 → 重复组
node tools/e2e-quarantine.mjs                  # 移入隔离区 → 搬回 → 彻底清空（会在 Node 侧核对文件 sha256）
```

## 文档

- 设计规格：[`docs/superpowers/specs/2026-10-05-picsieve-design.md`](docs/superpowers/specs/2026-10-05-picsieve-design.md)
- 实现计划：[`docs/superpowers/plans/2026-10-05-picsieve-implementation.md`](docs/superpowers/plans/2026-10-05-picsieve-implementation.md)

## 明确不做

图片编辑、云端同步、按画师重新整理目录、画面语义识别（人脸 / 画风 / NSFW）、
macOS 与 Linux 支持、多用户与权限体系。
