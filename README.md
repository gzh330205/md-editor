# 📝 Markdown 编辑器

一个基于 **Tauri 2 + Vue 3** 的桌面 Markdown 读写软件：左侧编辑、右侧实时渲染的分屏体验，支持多标签页、文件树、公式图表和自动更新。

## ✨ 功能特性

- **分屏编辑**：CodeMirror 6 编辑器（语法高亮、行号）+ marked 实时渲染预览，双向**行级滚动联动**（拖拽滚动条/滚轮均精确跟随）
- **多标签页**：未保存标记（●）、中键/`Ctrl+W` 关闭、未命名标签另存为自动绑定路径
- **文件树**：选择文件夹后浏览，懒加载展开目录，点击文件直接打开
- **最近文件**：自动记忆最近 8 个文件，下次启动自动恢复上次文档
- **Markdown 渲染**：GFM 表格/任务清单、KaTeX 数学公式（`$...$` / `$$...$$`）、Mermaid 图表、代码语法高亮（highlight.js）
- **主题与设置**：暗色主题跟随系统（或手动指定），编辑器字体/字号可调
- **导出**：导出独立 HTML 文件；通过系统打印对话框另存为 PDF
- **自动更新**：启动静默检查新版本，发现更新后横幅提示，一键下载安装

## 📦 安装

从 [GitHub Releases](https://github.com/gzh330205/md-editor/releases/latest) 下载：

| 文件 | 说明 |
|---|---|
| `md-editor_<版本>_x64-setup.exe` | NSIS 安装包（推荐，支持自动更新） |
| `md-editor_<版本>_x64_en-US.msi` | MSI 安装包（适合企业分发/静默安装） |

> 自动更新仅对 NSIS 安装包生效；MSI 安装的用户需手动下载新版。

## ⌨️ 快捷键

| 快捷键 | 功能 |
|---|---|
| `Ctrl+N` | 新建标签 |
| `Ctrl+O` | 打开文件 |
| `Ctrl+S` | 保存 |
| `Ctrl+Shift+S` | 另存为 |
| `Ctrl+W` | 关闭当前标签 |

## 🛠️ 开发

环境要求：Node.js 18+、Rust 1.77+、Windows（WebView2 随系统自带）

```bash
npm install
npm run tauri dev     # 开发模式（热更新）
npm run tauri build   # 打包 release
```

项目结构：

```
src/                  # 前端（Vue 3 + TS + Vite）
├── App.vue           # 主界面：标签页 + 分屏编辑器 + 工具栏
├── components/
│   ├── FileTree.vue      # 文件树
│   └── SettingsModal.vue # 设置弹窗（主题/字体/更新）
└── types.ts          # 共享类型定义
src-tauri/            # Tauri 外壳（Rust）
└── src/lib.rs        # 文件读写/目录/导出命令 + 插件注册
scripts/release.sh    # 一键发布脚本
```

## 🚀 发布新版本

### 首次准备（已完成）

1. 生成签名密钥对（私钥请妥善备份！丢失将无法再发布更新）：
   ```bash
   npm run tauri signer generate -w ~/.tauri/md-editor.key
   ```
2. 安装并登录 [GitHub CLI](https://cli.github.com/)：`gh auth login`

### 每次发版

```bash
# 1. 修改版本号（两处保持一致）：
#    src-tauri/tauri.conf.json  ->  version
#    package.json               ->  version

# 2. 一键发布（打包 → 签名 → latest.json → GitHub Release）
bash scripts/release.sh 0.2.0

# 3. 提交推送代码
git add -A && git commit -m "chore: release v0.2.0" && git push
```

发布后，已安装用户打开应用即会收到更新提示。

## 🧱 技术栈

- **外壳**：[Tauri 2](https://tauri.app/)（Rust）—— 窗口、系统对话框、文件系统、自动更新
- **前端**：Vue 3 + TypeScript + Vite
- **编辑器**：[CodeMirror 6](https://codemirror.net/)
- **渲染**：[marked](https://marked.js.org/) + [KaTeX](https://katex.org/) + [Mermaid](https://mermaid.js.org/) + [highlight.js](https://highlightjs.org/) + [DOMPurify](https://github.com/cure53/DOMPurify)
- **界面样式**：[github-markdown-css](https://github.com/sindresorhus/github-markdown-css)

## 📄 License

MIT
