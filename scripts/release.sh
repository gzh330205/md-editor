#!/usr/bin/env bash
# 发布脚本：打包（签名）→ 生成 latest.json → 创建 GitHub Release 并上传资产
# 用法: scripts/release.sh <version>   （如 scripts/release.sh 0.1.0）
# 前置: 已安装 GitHub CLI 并登录（gh auth status 确认）
set -euo pipefail

VERSION="${1:?usage: release.sh <version>}"
cd "$(dirname "$0")/.."

command -v gh >/dev/null || { echo "错误: 未安装 GitHub CLI，请先: winget install GitHub.cli"; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "错误: 未登录 GitHub，请先: gh auth login"; exit 1; }

export TAURI_SIGNING_PRIVATE_KEY_PATH="$HOME/.tauri/md-editor.key"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(cat "$HOME/.tauri/md-editor.key.password")"

# 1. 版本号一致性检查
node -e "
const c = require('./src-tauri/tauri.conf.json');
const p = require('./package.json');
if (c.version !== '$VERSION') { console.error('tauri.conf.json version =', c.version, '!=', '$VERSION'); process.exit(1); }
if (p.version !== '$VERSION') { console.error('package.json version =', p.version, '!=', '$VERSION'); process.exit(1); }
console.log('版本一致:', '$VERSION');
"

# 2. 打包（含 updater 签名）
echo "==> npm run tauri build"
npm run tauri build

BUNDLE="src-tauri/target/release/bundle"
NSIS="$BUNDLE/nsis/md-editor_${VERSION}_x64-setup.exe"
SIG="${NSIS}.sig"
MSI=$(ls "$BUNDLE"/msi/*.msi | head -1)
[ -f "$SIG" ] || { echo "错误: 缺少签名文件 $SIG（检查签名环境变量）"; exit 1; }
echo "==> 打包完成:"
ls -la "$NSIS" "$SIG" "$MSI"

# 3. 生成 latest.json（自动更新清单）
SIGNATURE="$(cat "$SIG")"
PUB_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
cat > "$BUNDLE/latest.json" <<EOF
{
  "version": "$VERSION",
  "notes": "https://github.com/gzh330205/md-editor/releases/tag/v$VERSION",
  "pub_date": "$PUB_DATE",
  "platforms": {
    "windows-x86_64": {
      "signature": "$SIGNATURE",
      "url": "https://github.com/gzh330205/md-editor/releases/download/v$VERSION/md-editor_${VERSION}_x64-setup.exe"
    }
  }
}
EOF
echo "==> latest.json 已生成"

# 4. 创建 Release 并上传资产
echo "==> 创建 GitHub Release v$VERSION 并上传资产"
gh release create "v$VERSION" \
  "$NSIS" "$SIG" "$MSI" "$BUNDLE/latest.json" \
  --repo gzh330205/md-editor \
  --title "v$VERSION" \
  --notes "**Markdown 编辑器 v$VERSION**

下载 **md-editor_${VERSION}_x64-setup.exe** 安装，应用内支持自动更新。

包含：分屏编辑、多标签页、文件树、KaTeX/Mermaid、暗色主题、双向滚动联动、导出 HTML/PDF。"

echo ""
echo "✅ Release v$VERSION 发布完成！"
echo "   下载页: https://github.com/gzh330205/md-editor/releases/tag/v$VERSION"
echo "   更新清单: https://github.com/gzh330205/md-editor/releases/latest/download/latest.json"
