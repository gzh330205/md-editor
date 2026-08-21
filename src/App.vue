<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import { load } from '@tauri-apps/plugin-store'
import type { Store } from '@tauri-apps/plugin-store'
import { EditorState, StateEffect } from '@codemirror/state'
import type { Extension } from '@codemirror/state'
import { EditorView, keymap } from '@codemirror/view'
import { basicSetup } from 'codemirror'
import { markdown } from '@codemirror/lang-markdown'
import { languages } from '@codemirror/language-data'
import { indentWithTab } from '@codemirror/commands'
import { oneDark } from '@codemirror/theme-one-dark'
import { marked, Tokens, TokenizerAndRendererExtension } from 'marked'
import type { RendererObject, Token } from 'marked'
import katex from 'katex'
import DOMPurify from 'dompurify'
import hljs from 'highlight.js/lib/common'
import githubLight from 'github-markdown-css/github-markdown.css?raw'
import githubDark from 'github-markdown-css/github-markdown-dark.css?raw'
import hljsLight from 'highlight.js/styles/github.css?raw'
import hljsDark from 'highlight.js/styles/github-dark.css?raw'
import katexCss from 'katex/dist/katex.min.css?raw'
import FileTree from './components/FileTree.vue'
import SettingsModal from './components/SettingsModal.vue'
import { DEFAULT_SETTINGS, fontFamilyFor } from './types'
import type { AppSettings, ThemeMode } from './types'

// ---------- KaTeX 数学公式（marked 自定义扩展：$...$ 与 $$...$$） ----------
const inlineMathExt: TokenizerAndRendererExtension = {
  name: 'inlineMath',
  level: 'inline',
  start(src: string) {
    const idx = src.indexOf('$')
    return idx < 0 ? undefined : idx
  },
  tokenizer(src: string) {
    const match = src.match(/^\$([^$\n]+)\$/)
    if (!match) return undefined
    return { type: 'inlineMath', raw: match[0], text: match[1] }
  },
  renderer(token: Tokens.Generic) {
    return katex.renderToString(token.text, { throwOnError: false })
  },
}

const blockMathExt: TokenizerAndRendererExtension = {
  name: 'blockMath',
  level: 'block',
  start(src: string) {
    const idx = src.indexOf('$$')
    return idx < 0 ? undefined : idx
  },
  tokenizer(src: string) {
    const match = src.match(/^\$\$([\s\S]+?)\$\$/)
    if (!match) return undefined
    return { type: 'blockMath', raw: match[0], text: match[1].trim() }
  },
  renderer(token: Tokens.Generic) {
    return katex.renderToString(token.text, { displayMode: true, throwOnError: false })
  },
}

marked.use({ extensions: [inlineMathExt, blockMathExt] })

// ---------- 源码行号标注（用于两侧滚动条行级联动） ----------
const ESC_MAP: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
}
function escHtml(s: string) {
  return s.replace(/[&<>"']/g, (c) => ESC_MAP[c])
}

let currentLineMap: WeakMap<object, number> | null = null

function childrenOf(t: Token): Token[] | null {
  if (t.type === 'list') return (t as Tokens.List).items
  return (t as Tokens.Generic).tokens ?? null
}

/** 遍历 lexer 产出的 token，计算每个 token 对应的源码起始行号（1-based） */
function buildLineMap(src: string, tokens: Token[]): WeakMap<object, number> {
  const map = new WeakMap<object, number>()
  // marked 的 lexer 会把 \r\n 规范成 \n，这里同步规范化，
  // 否则 CRLF 文档的 token.raw 与源文本永远匹配不上，行号全部错乱
  const norm = src.replace(/\r\n/g, '\n').replace(/\r/g, '\n')
  // 预计算每行起始偏移，供 O(log n) 行号查询
  const lineStarts: number[] = [0]
  for (let i = 0; i < norm.length; i++) {
    if (norm[i] === '\n') lineStarts.push(i + 1)
  }
  const lineAt = (offset: number) => {
    let lo = 0
    let hi = lineStarts.length - 1
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1
      if (lineStarts[mid] <= offset) lo = mid
      else hi = mid - 1
    }
    return lo + 1
  }
  // 按顺序消费：每个 token 在 [pos, end) 范围内找自己的 raw；
  // 嵌套 token（列表项/引用内部）只在父 token 的 raw 范围内找，避免匹配到
  // 文档后面重复出现的相同文本导致位置跳飞；
  // 找不到时退化到当前位置，保证位置只进不退、行号单调。
  const walk = (list: Token[], start: number, end: number): number => {
    let pos = start
    for (const t of list) {
      let idx = norm.indexOf(t.raw, pos)
      if (idx < 0 || idx + t.raw.length > end) idx = -1
      const at = idx >= 0 ? idx : pos
      map.set(t, lineAt(at))
      const children = childrenOf(t)
      let childEnd = at
      if (children && children.length) {
        childEnd = walk(children, at, idx >= 0 ? idx + t.raw.length : end)
      }
      pos = idx >= 0 ? idx + t.raw.length : Math.max(pos, childEnd + t.raw.length)
      if (pos > end) pos = end
    }
    return pos
  }
  walk(tokens, 0, norm.length)
  return map
}

/** 给块级元素注入 data-line 属性（仅覆盖需要定位的块级方法） */
const lineRenderer: RendererObject = {
  heading(token) {
    const line = currentLineMap?.get(token) ?? 0
    return `<h${token.depth} data-line="${line}">${this.parser.parseInline(token.tokens)}</h${token.depth}>\n`
  },
  paragraph(token) {
    const line = currentLineMap?.get(token) ?? 0
    return `<p data-line="${line}">${this.parser.parseInline(token.tokens)}</p>\n`
  },
  code(token) {
    const line = currentLineMap?.get(token) ?? 0
    const lang = (token.lang || '').match(/^\S*/)?.[0] ?? ''
    const text = token.text.replace(/\n$/, '') + '\n'
    return lang
      ? `<pre data-line="${line}"><code class="language-${escHtml(lang)}">${escHtml(text)}</code></pre>\n`
      : `<pre data-line="${line}"><code>${escHtml(text)}</code></pre>\n`
  },
  blockquote(token) {
    const line = currentLineMap?.get(token) ?? 0
    return `<blockquote data-line="${line}">\n${this.parser.parse(token.tokens)}</blockquote>\n`
  },
  hr(token) {
    const line = currentLineMap?.get(token) ?? 0
    return `<hr data-line="${line}">\n`
  },
  list(token) {
    const line = currentLineMap?.get(token) ?? 0
    let body = ''
    for (const item of token.items) body += this.listitem(item)
    const tag = token.ordered ? 'ol' : 'ul'
    const start = token.ordered && token.start !== 1 ? ` start="${token.start}"` : ''
    return `<${tag} data-line="${line}"${start}>\n${body}</${tag}>\n`
  },
  table(token) {
    const line = currentLineMap?.get(token) ?? 0
    let head = ''
    for (const cell of token.header) head += this.tablecell(cell)
    let rows = ''
    for (const row of token.rows) {
      let cells = ''
      for (const cell of row) cells += this.tablecell(cell)
      rows += this.tablerow({ text: cells })
    }
    const tbody = rows ? `<tbody>${rows}</tbody>` : ''
    return `<table data-line="${line}">\n<thead>\n${head}</thead>\n${tbody}</table>\n`
  },
}

marked.use({ renderer: lineRenderer })

/** 渲染 Markdown：lexer 计算行号 → parser 输出带 data-line 的 HTML */
function renderMarkdown(src: string): string {
  // 必须合并全局 defaults（含 renderer/扩展注册），否则自定义 renderer 会丢失
  const opts = { ...marked.defaults, async: false, breaks: true }
  const tokens = marked.lexer(src, opts)
  currentLineMap = buildLineMap(src, tokens)
  const out = marked.parser(tokens, opts)
  currentLineMap = null
  return out
}

// ---------- 状态 ----------
interface Tab {
  key: string // path（已命名）或 __untitled__N（未命名）
  path: string | null
  name: string
  dirty: boolean
}
// 说明：vite watcher 已配置忽略编辑工具的临时文件，编辑本文件不会导致 dev 崩溃

const editorEl = ref<HTMLElement | null>(null)
const previewPaneEl = ref<HTMLElement | null>(null)
const previewEl = ref<HTMLElement | null>(null)
const mainEl = ref<HTMLElement | null>(null)

let editorView: EditorView | null = null
let store: Store | null = null
let suppressDirty = false
let renderTimer: number | undefined
let untitledCounter = 0

const tabs = ref<Tab[]>([])
const activeKey = ref<string | null>(null)
const tabContents = reactive(new Map<string, string>())
const recentFiles = ref<string[]>([])
const splitRatio = ref(50)
const showTree = ref(false)
const treeRoot = ref<string | null>(null)
const showSettings = ref(false)
const settings = ref<AppSettings>({ ...DEFAULT_SETTINGS })
let lastSavedPath: string | null = null

const activeTab = computed(() => tabs.value.find((t) => t.key === activeKey.value) ?? null)
const content = computed(() => {
  const key = activeKey.value
  return key ? (tabContents.get(key) ?? '') : ''
})
const fileName = computed(() => activeTab.value?.name ?? '未命名')
const dirty = computed(() => activeTab.value?.dirty ?? false)

function basename(path: string) {
  const parts = path.split(/[\\/]/)
  return parts[parts.length - 1] || path
}

const statLine = computed(() => {
  const text = content.value
  const cjk = (text.match(/[\u4e00-\u9fa5]/g) ?? []).length
  const words = (text.match(/[A-Za-z0-9]+(?:['’-][A-Za-z0-9]+)*/g) ?? []).length
  const lines = text ? text.split('\n').length : 0
  return `${lines} 行 · ${cjk + words} 词 · ${text.length} 字符`
})

const WELCOME = `# 欢迎使用 Markdown 编辑器 📝

这是一个基于 **Tauri 2 + Vue 3** 的 Markdown 读写软件：

- **多标签页**编辑，支持未保存标记（●）与关闭（× / 中键 / \`Ctrl+W\`）
- 左侧 **📁 文件树**：选择文件夹后浏览、点击打开文件
- 工具栏：新建 / 打开 / 保存 / 另存为（\`Ctrl+N/O/S\`、\`Ctrl+Shift+S\`）
- 自动记忆**最近打开的文件**，下次启动自动恢复
- **⚙️ 设置**：主题模式（跟随系统/浅色/深色）、编辑器字体与字号
- 支持 **KaTeX 数学公式** 与 **Mermaid 图表**，可**导出 HTML / PDF**

## 数学公式（KaTeX）

行内公式：$E = mc^2$，块级公式：

$$
\int_{-\infty}^{\infty} e^{-x^2}\,dx = \sqrt{\pi}
$$

## 图表（Mermaid）

\`\`\`mermaid
flowchart LR
    A[编辑 Markdown] --> B[实时渲染]
    B --> C{满意?}
    C -- 是 --> D[保存 / 导出]
    C -- 否 --> A
\`\`\`

## 代码高亮

\`\`\`rust
fn main() {
    println!("Hello, MD Editor!");
}
\`\`\`

> 试试修改左侧内容，右侧会实时更新。
`

// ---------- 主题（跟随系统或手动指定） ----------
const mq = window.matchMedia('(prefers-color-scheme: dark)')
const effectiveDark = computed(() => {
  const mode: ThemeMode = settings.value.themeMode
  return mode === 'dark' || (mode === 'auto' && mq.matches)
})
const themeStyleEl = document.createElement('style')
themeStyleEl.id = 'theme-styles'
document.head.appendChild(themeStyleEl)

function applyThemeCss() {
  themeStyleEl.textContent =
    katexCss +
    '\n' +
    (effectiveDark.value ? githubDark : githubLight) +
    '\n' +
    (effectiveDark.value ? hljsDark : hljsLight)
}
applyThemeCss()

function onSchemeChange() {
  if (settings.value.themeMode === 'auto') applyTheme()
}
mq.addEventListener('change', onSchemeChange)

function editorFontExt(): Extension {
  return EditorView.theme({
    '.cm-content': { fontSize: `${settings.value.fontSize}px` },
    '.cm-scroller': { fontFamily: fontFamilyFor(settings.value.fontFamily) },
  })
}

// ---------- 编辑器 ----------
const updateListener = EditorView.updateListener.of((update) => {
  if (!update.docChanged) return
  const key = activeKey.value
  if (!key) return
  if (suppressDirty) {
    suppressDirty = false
    return
  }
  tabContents.set(key, update.state.doc.toString())
  const idx = tabs.value.findIndex((t) => t.key === key)
  if (idx >= 0 && !tabs.value[idx].dirty) {
    tabs.value = tabs.value.map((t, i) => (i === idx ? { ...t, dirty: true } : t))
  }
  scheduleRender()
})

function getExtensions(): Extension[] {
  return [
    basicSetup,
    markdown({ codeLanguages: languages }),
    keymap.of([indentWithTab]),
    EditorView.lineWrapping,
    updateListener,
    editorFontExt(),
    effectiveDark.value ? oneDark : [],
  ]
}

function applyTheme() {
  document.documentElement.classList.toggle('dark', effectiveDark.value)
  applyThemeCss()
  editorView?.dispatch({ effects: StateEffect.reconfigure.of(getExtensions()) })
  scheduleRender() // 让 Mermaid 等用新主题重渲染
}

// ---------- 标签页 ----------
function setDoc(text: string) {
  if (!editorView) return
  suppressDirty = true
  editorView.dispatch({
    changes: { from: 0, to: editorView.state.doc.length, insert: text },
  })
}

function activateTab(key: string) {
  if (activeKey.value === key) return
  activeKey.value = key
  setDoc(tabContents.get(key) ?? '')
  void renderPreview()
}

function openTab(path: string, text: string) {
  const existing = tabs.value.find((t) => t.path === path)
  if (existing) {
    activateTab(existing.key)
    return
  }
  const key = path
  tabs.value.push({ key, path, name: basename(path), dirty: false })
  tabContents.set(key, text)
  activateTab(key)
}

function openUntitled(text: string) {
  const n = ++untitledCounter
  const key = `__untitled__${n}`
  tabs.value.push({ key, path: null, name: `未命名-${n}`, dirty: false })
  tabContents.set(key, text)
  activateTab(key)
}

function closeTab(key: string) {
  const tab = tabs.value.find((t) => t.key === key)
  if (!tab) return
  if (tab.dirty && !window.confirm(`「${tab.name}」有未保存的修改，关闭将丢失。是否继续？`)) return
  const idx = tabs.value.findIndex((t) => t.key === key)
  tabs.value.splice(idx, 1)
  tabContents.delete(key)
  if (activeKey.value === key) {
    const next = tabs.value[Math.min(idx, tabs.value.length - 1)]
    if (next) activateTab(next.key)
    else {
      activeKey.value = null
      setDoc('')
      html.value = ''
    }
  }
}

function newFile() {
  openUntitled('')
}

// ---------- 文件操作 ----------
async function openFile() {
  try {
    const res = await invoke<{ path: string; content: string } | null>('open_file')
    if (!res) return
    openTab(res.path, res.content)
    await rememberFile(res.path)
  } catch (e) {
    window.alert(`打开文件失败：${e}`)
  }
}

async function doSave(targetPath: string | null): Promise<boolean> {
  const tab = activeTab.value
  if (!tab) return false
  try {
    const p = await invoke<string | null>('save_file', {
      path: targetPath,
      content: tabContents.get(tab.key) ?? '',
    })
    if (p === null) return false
    // 未命名标签保存后绑定到真实路径
    if (tab.path === null || targetPath === null) {
      const idx = tabs.value.findIndex((t) => t.key === tab.key)
      if (idx >= 0) {
        const oldKey = tab.key
        const c = tabContents.get(oldKey) ?? ''
        tabContents.delete(oldKey)
        tabContents.set(p, c)
        tabs.value = tabs.value.map((t, i) =>
          i === idx ? { key: p, path: p, name: basename(p), dirty: false } : t,
        )
        activeKey.value = p
      }
    } else {
      tabs.value = tabs.value.map((t) => (t.key === tab.key ? { ...t, dirty: false } : t))
    }
    await rememberFile(p)
    return true
  } catch (e) {
    window.alert(`保存文件失败：${e}`)
    return false
  }
}

function saveFile() {
  void doSave(activeTab.value?.path ?? null)
}

function saveAsFile() {
  void doSave(null)
}

// ---------- 最近文件 / 持久化 ----------
async function rememberFile(path: string) {
  recentFiles.value = [path, ...recentFiles.value.filter((p) => p !== path)].slice(0, 8)
  lastSavedPath = path
  await saveStore()
}

async function saveStore() {
  if (!store) return
  try {
    await store.set('recentFiles', recentFiles.value)
    await store.set('lastFile', lastSavedPath)
    await store.set('settings', { ...settings.value })
    await store.set('treeRoot', treeRoot.value)
    await store.save()
  } catch {
    /* 持久化失败不影响主流程 */
  }
}

async function initStore() {
  let restored = false
  try {
    store = await load('settings.json', { autoSave: false })
    const recent = await store.get<string[]>('recentFiles')
    if (Array.isArray(recent)) recentFiles.value = recent
    const saved = await store.get<AppSettings>('settings')
    if (saved && typeof saved === 'object') {
      settings.value = { ...DEFAULT_SETTINGS, ...saved }
    }
    const root = await store.get<string>('treeRoot')
    if (typeof root === 'string' && root) {
      treeRoot.value = root
      showTree.value = true
    }
    const lastFile = await store.get<string>('lastFile')
    if (typeof lastFile === 'string' && lastFile) {
      try {
        const c = await invoke<string>('read_file_at', { path: lastFile })
        openTab(lastFile, c)
        restored = true
      } catch {
        /* 文件已被移动/删除，忽略 */
      }
    }
  } catch {
    store = null
  }
  if (!restored && tabs.value.length === 0) openUntitled(WELCOME)
  applyTheme() // 确保界面框架/编辑器/预览主题一致
  void renderPreview()
}

async function openRecent(e: Event) {
  const sel = e.target as HTMLSelectElement
  const p = sel.value
  sel.value = ''
  if (!p) return
  try {
    const c = await invoke<string>('read_file_at', { path: p })
    openTab(p, c)
    await rememberFile(p)
  } catch (err) {
    window.alert(`打开文件失败：${err}`)
  }
}

// ---------- 文件树 ----------
async function toggleTree() {
  if (!showTree.value) {
    if (!treeRoot.value) {
      try {
        const folder = await invoke<string | null>('open_folder')
        if (!folder) return
        treeRoot.value = folder
      } catch (e) {
        window.alert(`选择文件夹失败：${e}`)
        return
      }
    }
    showTree.value = true
  } else {
    showTree.value = false
  }
  await saveStore()
}

async function onTreeOpenFile(path: string) {
  try {
    const c = await invoke<string>('read_file_at', { path })
    openTab(path, c)
    await rememberFile(path)
  } catch (e) {
    window.alert(`打开文件失败：${e}`)
  }
}

// ---------- 设置 ----------
function updateSettings(patch: Partial<AppSettings>) {
  settings.value = { ...settings.value, ...patch }
  applyTheme()
  void saveStore()
}

// ---------- 导出 ----------
function escapeHtml(s: string) {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

function buildExportHtml(): string {
  const body = DOMPurify.sanitize(renderMarkdown(content.value))
  const css = [githubLight, hljsLight, katexCss].join('\n')
  return `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>${escapeHtml(fileName.value)}</title>
<style>
${css}
body { margin: 0; background: #fff; }
.markdown-body { max-width: 900px; margin: 0 auto; padding: 24px; }
</style>
</head>
<body>
<article class="markdown-body">${body}</article>
<script src="https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js"><\/script>
<script>mermaid.initialize({ startOnLoad: true });<\/script>
</body>
</html>`
}

async function exportHtml() {
  try {
    const base = (fileName.value.replace(/\.md$/i, '') || 'untitled') + '.html'
    const p = await invoke<string | null>('export_file', {
      content: buildExportHtml(),
      defaultName: base,
      filterName: 'HTML 文件',
      extensions: ['html'],
    })
    if (p) window.alert(`已导出：${p}`)
  } catch (e) {
    window.alert(`导出失败：${e}`)
  }
}

function exportPdf() {
  // WebView2 原生打印对话框，可选择 "Microsoft Print to PDF"
  window.print()
}

// ---------- 预览渲染 ----------
function scheduleRender() {
  window.clearTimeout(renderTimer)
  renderTimer = window.setTimeout(renderPreview, 120)
}

let mermaidPromise: Promise<typeof import('mermaid')> | null = null
function loadMermaid() {
  if (!mermaidPromise) mermaidPromise = import('mermaid')
  return mermaidPromise
}

const html = ref('')

async function renderPreview() {
  const pane = previewPaneEl.value
  const ratio =
    pane && pane.scrollHeight > pane.clientHeight
      ? pane.scrollTop / (pane.scrollHeight - pane.clientHeight)
      : 0

  const raw = renderMarkdown(content.value)
  html.value = DOMPurify.sanitize(raw)
  await nextTick()

  // 代码高亮（跳过 Mermaid 块）
  previewEl.value?.querySelectorAll('pre code').forEach((el) => {
    if (el.classList.contains('language-mermaid')) return
    if (el.getAttribute('data-highlighted') !== 'yes') {
      hljs.highlightElement(el as HTMLElement)
      el.setAttribute('data-highlighted', 'yes')
    }
  })

  // Mermaid 图表
  const mermaidBlocks = Array.from(
    previewEl.value?.querySelectorAll('pre > code.language-mermaid') ?? [],
  )
  if (mermaidBlocks.length > 0) {
    try {
      const mermaid = await loadMermaid()
      mermaid.default.initialize({
        startOnLoad: false,
        theme: effectiveDark.value ? 'dark' : 'default',
        securityLevel: 'strict',
      })
      for (const [i, code] of mermaidBlocks.entries()) {
        const pre = code.parentElement
        if (!pre) continue
        try {
          const { svg } = await mermaid.default.render(
            `mmd-${Date.now()}-${i}`,
            code.textContent ?? '',
          )
          pre.innerHTML = svg
        } catch (err) {
          pre.innerHTML = `<div class="mermaid-error">⚠️ Mermaid 渲染失败：${String(err)}</div>`
        }
      }
    } catch {
      // Mermaid 动态加载失败时保留原始代码块
    }
  }

  // 恢复滚动位置
  if (pane && ratio > 0) {
    await nextTick()
    pane.scrollTop = ratio * (pane.scrollHeight - pane.clientHeight)
  }
}

// ---------- 快捷键 ----------
function onGlobalKey(e: KeyboardEvent) {
  if (!(e.ctrlKey || e.metaKey)) return
  const k = e.key.toLowerCase()
  if (k === 's') {
    e.preventDefault()
    if (e.shiftKey) saveAsFile()
    else saveFile()
  } else if (k === 'o') {
    e.preventDefault()
    void openFile()
  } else if (k === 'n') {
    e.preventDefault()
    newFile()
  } else if (k === 'w') {
    e.preventDefault()
    if (activeKey.value) closeTab(activeKey.value)
  }
}

// ---------- 分屏滚动同步（按源码行号双向定位） ----------
let syncSource: 'editor' | 'preview' | null = null
let syncUntil = 0

/** 编辑器滚动 → 预览定位：找到第一个源码行号 >= 视口顶部行的块元素 */
function syncEditorToPreview() {
  if (!editorView || !previewPaneEl.value) return
  const lineBlock = editorView.lineBlockAtHeight(editorView.scrollDOM.scrollTop)
  const line = editorView.state.doc.lineAt(lineBlock.from).number
  const pane = previewPaneEl.value
  const candidates = pane.querySelectorAll<HTMLElement>('[data-line]')
  let target: HTMLElement | null = null
  for (const el of candidates) {
    const l = Number(el.dataset.line ?? 0)
    if (l >= line) {
      target = el
      break
    }
    target = el
  }
  if (!target) return
  const paneRect = pane.getBoundingClientRect()
  const elRect = target.getBoundingClientRect()
  pane.scrollTop += elRect.top - paneRect.top - 8
}

/** 预览滚动 → 编辑器定位：视口顶部第一个可见块对应的源码行号 */
function syncPreviewToEditor() {
  if (!editorView || !previewPaneEl.value) return
  const pane = previewPaneEl.value
  const paneRect = pane.getBoundingClientRect()
  // 纯几何遍历：找第一个“底部越过视口顶边”的 data-line 块（不依赖命中测试）
  let target: HTMLElement | null = null
  for (const el of pane.querySelectorAll<HTMLElement>('[data-line]')) {
    if (el.getBoundingClientRect().bottom > paneRect.top + 4) {
      target = el
      break
    }
  }
  if (!target) return
  const line = Number(target.dataset.line ?? 0)
  if (!line) return
  const doc = editorView.state.doc
  const n = Math.min(line, doc.lines)
  const block = editorView.lineBlockAt(doc.line(n).from)
  editorView.scrollDOM.scrollTop = block.top - 10
}

function onEditorScroll() {
  const now = Date.now()
  if (syncSource === 'preview' && now < syncUntil) return
  syncSource = 'editor'
  syncUntil = now + 120
  syncEditorToPreview()
}

function onPreviewScroll() {
  const now = Date.now()
  if (syncSource === 'editor' && now < syncUntil) return
  syncSource = 'preview'
  syncUntil = now + 120
  syncPreviewToEditor()
}

// ---------- 分隔条拖拽 ----------
function startDrag(e: PointerEvent) {
  e.preventDefault()
  const onMove = (ev: PointerEvent) => {
    const rect = mainEl.value?.getBoundingClientRect()
    if (!rect) return
    const ratio = ((ev.clientX - rect.left) / rect.width) * 100
    splitRatio.value = Math.min(85, Math.max(15, ratio))
  }
  const onUp = () => {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
  }
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
}

// ---------- 预览链接拦截（用系统浏览器打开） ----------
function onPreviewClick(e: MouseEvent) {
  const a = (e.target as HTMLElement).closest('a')
  if (!a || !a.href) return
  e.preventDefault()
  void openUrl(a.href)
}

// ---------- 生命周期 ----------
onMounted(() => {
  if (!editorEl.value) return
  editorView = new EditorView({
    parent: editorEl.value,
    state: EditorState.create({ doc: '', extensions: getExtensions() }),
  })
  editorView.scrollDOM.addEventListener('scroll', onEditorScroll)
  window.addEventListener('keydown', onGlobalKey, true)
  previewPaneEl.value?.addEventListener('scroll', onPreviewScroll)

  void initStore()
})

onBeforeUnmount(() => {
  editorView?.scrollDOM.removeEventListener('scroll', onEditorScroll)
  window.removeEventListener('keydown', onGlobalKey, true)
  previewPaneEl.value?.removeEventListener('scroll', onPreviewScroll)
  mq.removeEventListener('change', onSchemeChange)
  themeStyleEl.remove()
  window.clearTimeout(renderTimer)
  editorView?.destroy()
  editorView = null
})
</script>

<template>
  <div class="app">
    <header class="toolbar">
      <span class="brand">📝 Markdown 编辑器</span>
      <div class="btn-group">
        <button @click="newFile">新建</button>
        <button @click="openFile">打开…</button>
        <button @click="saveFile" :disabled="!dirty">保存</button>
        <button @click="saveAsFile">另存为…</button>
      </div>
      <div class="btn-group">
        <button @click="exportHtml">导出 HTML</button>
        <button @click="exportPdf">导出 PDF</button>
      </div>
      <select
        v-if="recentFiles.length"
        class="recent"
        value=""
        title="最近打开"
        @change="openRecent"
      >
        <option value="" disabled>最近打开</option>
        <option v-for="f in recentFiles" :key="f" :value="f">{{ basename(f) }}</option>
      </select>
      <div class="btn-group right">
        <button :class="{ on: showTree }" title="文件树" @click="toggleTree">📁</button>
        <button title="设置" @click="showSettings = true">⚙️</button>
      </div>
    </header>

    <div class="tabbar">
      <div
        v-for="t in tabs"
        :key="t.key"
        class="tab"
        :class="{ active: t.key === activeKey }"
        :title="t.path ?? t.name"
        @click="activateTab(t.key)"
        @auxclick="(e) => { if ((e as MouseEvent).button === 1) closeTab(t.key) }"
      >
        <span class="tab-name">{{ t.name }}</span>
        <span v-if="t.dirty" class="tab-dot">●</span>
        <button class="tab-close" title="关闭" @click.stop="closeTab(t.key)">×</button>
      </div>
    </div>

    <main ref="mainEl" class="main">
      <aside v-if="showTree" class="sidebar">
        <FileTree :root="treeRoot" @open-file="onTreeOpenFile" />
      </aside>
      <section class="pane editor-pane" :style="{ width: splitRatio + '%' }">
        <div ref="editorEl" class="editor"></div>
      </section>
      <div class="divider" @pointerdown="startDrag"></div>
      <section ref="previewPaneEl" class="pane preview-pane" @click="onPreviewClick">
        <article ref="previewEl" class="markdown-body" v-html="html"></article>
      </section>
    </main>

    <footer class="statusbar">
      <span class="path">{{ activeTab?.path ?? '未打开文件' }}</span>
      <span>{{ statLine }}</span>
      <span>{{ tabs.length }} 个标签</span>
      <span :class="dirty ? 'warn' : 'ok'">{{ dirty ? '● 未保存' : '已保存' }}</span>
    </footer>

    <SettingsModal
      :visible="showSettings"
      :settings="settings"
      @close="showSettings = false"
      @change="updateSettings"
    />
  </div>
</template>

<style>
* {
  box-sizing: border-box;
}

:root {
  --panel: #f6f8fa;
  --border: #d0d7de;
  --text: #1f2328;
  --muted: #57606a;
  --accent: #0969da;
  --btn-bg: #ffffff;
  --btn-hover: #f3f4f6;
  --preview-bg: #ffffff;
  --warn: #9a6700;
  --ok: #1a7f37;
  --tab-active: #ffffff;
  --scroll-thumb: #c9d1d9;
  --scroll-thumb-hover: #a5afbb;
}

html.dark {
  --panel: #161b22;
  --border: #30363d;
  --text: #e6edf3;
  --muted: #8b949e;
  --accent: #4493f8;
  --btn-bg: #21262d;
  --btn-hover: #30363d;
  --preview-bg: #0d1117;
  --warn: #d29922;
  --ok: #3fb950;
  --tab-active: #0d1117;
  --scroll-thumb: #3d444d;
  --scroll-thumb-hover: #565d66;
}

html,
body,
#app {
  height: 100%;
  margin: 0;
}

body {
  font-family:
    system-ui,
    -apple-system,
    'Segoe UI',
    'Microsoft YaHei',
    sans-serif;
  color: var(--text);
}

.app {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

/* ---------- 工具栏 ---------- */
.toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 46px;
  padding: 0 10px;
  background: var(--panel);
  border-bottom: 1px solid var(--border);
  user-select: none;
  flex: none;
}

.brand {
  font-weight: 600;
  white-space: nowrap;
}

.btn-group {
  display: flex;
  gap: 6px;
}

.btn-group.right {
  margin-left: auto;
}

.btn-group button,
.recent {
  padding: 4px 11px;
  font-size: 13px;
  font-family: inherit;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--btn-bg);
  color: var(--text);
  cursor: pointer;
}

.btn-group button:hover:not(:disabled),
.recent:hover {
  background: var(--btn-hover);
}

.btn-group button.on {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}

.btn-group button:disabled {
  opacity: 0.5;
  cursor: default;
}

.recent {
  max-width: 150px;
}

/* ---------- 标签栏 ---------- */
.tabbar {
  display: flex;
  align-items: stretch;
  height: 34px;
  background: var(--panel);
  border-bottom: 1px solid var(--border);
  overflow-x: auto;
  flex: none;
  user-select: none;
  scrollbar-width: thin;
}

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 6px 0 12px;
  min-width: 90px;
  max-width: 200px;
  border-right: 1px solid var(--border);
  font-size: 13px;
  color: var(--muted);
  cursor: pointer;
  white-space: nowrap;
  flex: none;
}

.tab:hover {
  background: var(--btn-hover);
}

.tab.active {
  background: var(--tab-active);
  color: var(--text);
  box-shadow: inset 0 2px 0 var(--accent);
}

.tab-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tab-dot {
  color: var(--warn);
  font-size: 10px;
}

.tab-close {
  border: none;
  background: none;
  color: var(--muted);
  font-size: 15px;
  line-height: 1;
  padding: 1px 5px;
  border-radius: 4px;
  cursor: pointer;
  flex: none;
}

.tab-close:hover {
  background: var(--accent);
  color: #fff;
}

/* ---------- 滚动条（WebView2 / Chromium 内核） ---------- */
* {
  scrollbar-width: thin;
  scrollbar-color: var(--scroll-thumb) transparent;
}

*::-webkit-scrollbar {
  width: 11px;
  height: 11px;
}

*::-webkit-scrollbar-track {
  background: transparent;
}

*::-webkit-scrollbar-thumb {
  background: var(--scroll-thumb);
  border-radius: 6px;
  border: 3px solid transparent;
  background-clip: padding-box;
}

*::-webkit-scrollbar-thumb:hover {
  background: var(--scroll-thumb-hover);
  border: 3px solid transparent;
  background-clip: padding-box;
}

*::-webkit-scrollbar-corner {
  background: transparent;
}

/* ---------- 分屏主体 ---------- */
.main {
  flex: 1;
  display: flex;
  min-height: 0;
}

.sidebar {
  flex: none;
  width: 240px;
  border-right: 1px solid var(--border);
  background: var(--panel);
  min-width: 0;
  display: flex;
}

.pane {
  overflow: hidden;
}

.editor-pane {
  flex: none;
  min-height: 0;
  position: relative;
}

/* 绝对定位撑满，确保 CodeMirror 滚动条稳定出现 */
.editor {
  position: absolute;
  inset: 0;
}

/* 注意：此样式块为非 scoped，必须用普通后代选择器（:deep 在非 scoped 下无效） */
.editor .cm-editor {
  height: 100%;
}

.editor .cm-scroller {
  overflow-y: auto;
}

.editor .cm-editor.cm-focused {
  outline: none;
}

.divider {
  flex: none;
  width: 6px;
  cursor: col-resize;
  background: var(--border);
  transition: background 0.15s;
}

.divider:hover {
  background: var(--accent);
}

.preview-pane {
  flex: 1;
  overflow: auto;
  background: var(--preview-bg);
}

.markdown-body {
  padding: 20px 28px 60px;
  max-width: 900px;
  margin: 0 auto;
}

.markdown-body .mermaid-error {
  padding: 8px 12px;
  color: #cf222e;
  background: #ffebe9;
  border: 1px solid #ff818266;
  border-radius: 6px;
  font-size: 13px;
}

/* ---------- 状态栏 ---------- */
.statusbar {
  display: flex;
  align-items: center;
  gap: 16px;
  height: 28px;
  padding: 0 12px;
  font-size: 12px;
  color: var(--muted);
  background: var(--panel);
  border-top: 1px solid var(--border);
  user-select: none;
  flex: none;
}

.statusbar .path {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.statusbar .warn {
  color: var(--warn);
}

.statusbar .ok {
  color: var(--ok);
}

/* ---------- 打印（导出 PDF 时只打印预览区） ---------- */
@media print {
  .toolbar,
  .statusbar,
  .tabbar,
  .sidebar,
  .editor-pane,
  .divider {
    display: none !important;
  }

  .app {
    height: auto !important;
    overflow: visible !important;
  }

  .main {
    display: block !important;
    height: auto !important;
  }

  .preview-pane {
    overflow: visible !important;
    background: #fff !important;
  }

  .markdown-body {
    max-width: none !important;
    padding: 0 !important;
    margin: 0 !important;
  }
}
</style>
