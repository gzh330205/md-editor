# AGENTS.md

面向 AI 代理与开发者的项目操作规范。**发版流程是本文档的核心内容**，任何涉及"发布新版本"的任务必须严格按本规范执行。

## 项目概览

Tauri 2 + Vue 3 (TypeScript) 桌面 Markdown 编辑器：左编辑（CodeMirror 6）右预览（marked）分屏，支持多标签页、文件树、KaTeX/Mermaid、暗色主题、自动更新（GitHub Release + tauri-plugin-updater）。

- 仓库：`https://github.com/gzh330205/md-editor`（**公开**，自动更新依赖公开资产）
- 前端：`src/`（Vue 3 + Vite），外壳：`src-tauri/`（Rust）
- 自动更新：NSIS 安装包 + 签名（`.sig`）+ `latest.json` 清单，端点指向 GitHub Release

## 常用命令

| 命令 | 用途 |
|---|---|
| `npm install` | 安装前端依赖 |
| `npm run tauri dev` | 开发模式（热更新）。**注意**：此会话环境会在回合间清理终端进程，长驻 dev 需用 WMI 方式启动（见"注意事项"） |
| `npm run build` | 前端类型检查（vue-tsc）+ 构建 |
| `cargo check`（在 src-tauri/） | Rust 编译检查 |
| `npm run tauri build` | 打包 release（NSIS + MSI + 签名） |
| `bash scripts/release.sh <版本>` | **一键发布**（打包 → 签名 → latest.json → GitHub Release） |

## 发版规范（RELEASE）

### 1. 版本号管理

- 版本号同时存在于两处，**必须一致**：
  - `src-tauri/tauri.conf.json` → `version`
  - `package.json` → `version`
- 版本语义：遵循语义化版本（`主.次.补丁`）。功能新增 → 次版本 +1；修复 → 补丁 +1。
- 发布脚本会自动校验两处一致性，不一致会中止。

### 2. 发布前置条件

- **签名密钥**：`~/.tauri/md-editor.key`（私钥）+ `~/.tauri/md-editor.key.password`（密码）。**这两份文件不进仓库、不可丢失**——丢失后无法签发更新包，已装用户将永远无法升级。发布脚本自动读取，无需手动指定。
- **GitHub CLI**：`gh` 已安装并登录（`gh auth status` 确认）。脚本会自动探测常见安装路径（含 `D:\Program Files\GitHub CLI`）。
- **仓库可见性**：必须是 **public**（自动更新的 `latest.json` 和安装包由未认证客户端下载；private 仓库会返回 404 导致更新失败）。

### 3. 发布步骤（每次发版）

```bash
# ① 修改版本号（两处同步）：
#    src-tauri/tauri.conf.json  ->  version
#    package.json               ->  version

# ② 一键发布：
bash scripts/release.sh 0.2.0

# ③ 提交推送代码（含版本号改动）：
git add -A
git commit -m "chore: release v0.2.0"
git push
```

`scripts/release.sh` 自动完成：版本一致性校验 → `npm run tauri build`（生成 NSIS/MSI + `.sig` 签名）→ 生成 `latest.json`（含签名和下载 URL，URL 中的版本号与 tag 对应）→ `gh release create v<版本>` 并上传 4 个资产：

- `md-editor_<版本>_x64-setup.exe`（NSIS 安装包）
- `md-editor_<版本>_x64-setup.exe.sig`（更新签名）
- `md-editor_<版本>_x64_en-US.msi`
- `latest.json`（自动更新清单）

### 4. 发布后检查（必须验证）

```bash
# 更新清单可公开访问（应返回 200 且 JSON 中 version 正确）：
curl -sL "https://github.com/gzh330205/md-editor/releases/latest/download/latest.json"

# 安装包可下载（应 200）：
curl -sL -o /dev/null -w "%{http_code}" \
  "https://github.com/gzh330205/md-editor/releases/download/v<版本>/md-editor_<版本>_x64-setup.exe"
```

- 注意：刚上传的资产可能需要数十秒传播，404 时可等待后重试；持续 404 先检查仓库是否为 public。
- 发布后应用内的自动更新：已安装用户启动 5 秒后自动检查，发现新版本会横幅提示。

### 5. 回滚/撤销

- 如发布有误：`gh release delete v<版本> --repo gzh330205/md-editor --cleanup-tag`，然后修复后重新发布新版本号。
- **不要复用已发布过的版本号**（更新比对靠版本号，重复会混淆）。

## 项目注意事项（已知坑，改代码前必读）

- **编辑工具写入与 vite watcher**：本会话的编辑工具采用"临时目录 + 原子替换"写入文件，曾导致 vite FSWatcher EBUSY 崩溃。`vite.config.ts` 已配置 `watch.ignored` 规避——**不要移除该配置**。
- **dev 进程管理**：`npm run tauri dev` 直接跑在终端会话里会在回合间被清理。长驻方式（WMI，日志进 `dev.log`）：
  ```bash
  powershell -NoProfile -Command "([wmiclass]'Win32_Process').Create('cmd /c \"set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222\" && cd /d D:\workspace\research\md-editor && npm run tauri dev > dev.log 2>&1')"
  ```
  `--remote-debugging-port=9222` 可让 WebView2 暴露 CDP 端口，用于真实环境调试（`curl http://127.0.0.1:9222/json/list` 拿 target）。
- **滚动联动的 data-line 行号**（`src/App.vue` 的 `buildLineMap`）：marked 的 lexer 会把 `\r\n` 规范成 `\n`，匹配前必须先把源文本 `\r\n` 规范化为 `\n`，否则 CRLF 文档行号全部错乱（曾导致"拖拽滚动条回顶"bug）。嵌套 token 的查找必须限制在父 token 的 raw 范围内，防止重复文本匹配跳飞。
- **非 scoped 样式**：`App.vue` 的 `<style>` 块是非 scoped 的，` :deep()` 伪类不会被编译而是被浏览器丢弃（无效 CSS）——直接写普通后代选择器（如 `.editor .cm-editor`）。
- **marked 传参**：直接调用 `marked.lexer/parser` 时 options 会整体替换全局 defaults，导致 `marked.use()` 注册的 renderer/扩展丢失——必须合并：`{ ...marked.defaults, async: false, breaks: true }`。
- **滚动同步锁**：双向滚动同步（`onEditorScroll`/`onPreviewScroll`）用 `syncSource` + 120ms 时间戳锁防抖，修改时注意别破坏。
- **签名相关**：`tauri.conf.json` 的 `bundle.createUpdaterArtifacts: true` 必须保留（否则不生成 `.sig`）；发布时签名环境变量由 release.sh 设置（私钥内容 + Windows 路径，gitbash 的 `/c/...` 风格路径 Rust 端不识别）。
