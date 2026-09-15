<script setup lang="ts">
import { computed } from 'vue'
import { DEFAULT_SETTINGS, FONT_OPTIONS } from '../types'
import type { AppSettings, AssociationStatus, ThemeMode } from '../types'

const props = defineProps<{
  visible: boolean
  settings: AppSettings
  version: string
  assoc: AssociationStatus | null
  assocBusy: boolean
  assocMessage: string
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'change', patch: Partial<AppSettings>): void
  (e: 'check-update'): void
  (e: 'set-default'): void
  (e: 'repair-assoc'): void
  (e: 'open-with'): void
}>()

function onThemeMode(e: Event) {
  emit('change', { themeMode: (e.target as HTMLSelectElement).value as ThemeMode })
}

function onFontSize(e: Event) {
  emit('change', { fontSize: Number((e.target as HTMLSelectElement).value) })
}

function onFontFamily(e: Event) {
  emit('change', { fontFamily: (e.target as HTMLSelectElement).value })
}

function restoreDefaults() {
  emit('change', { ...DEFAULT_SETTINGS })
}

/** 常见 Markdown 编辑器的 ProgID → 友好名称 */
const KNOWN_PROG_IDS: Record<string, string> = {
  'applications\\code.exe': 'Visual Studio Code',
  'vscode.md': 'Visual Studio Code',
  'applications\\typora.exe': 'Typora',
  'typora.md': 'Typora',
  'applications\\obsidian.exe': 'Obsidian',
  'applications\\notepad++.exe': 'Notepad++',
  'notepad++_file': 'Notepad++',
  'applications\\notepad.exe': '记事本',
  'txtfile': '记事本',
  'applications\\marktext.exe': 'MarkText',
  'marktext.md': 'MarkText',
  'applications\\wps.exe': 'WPS',
}

/** 把 ProgID 转成可读的程序名 */
function prettyProgId(id: string): string {
  const known = KNOWN_PROG_IDS[id.toLowerCase()]
  if (known) return known
  return id.replace(/^Applications\\/i, '').replace(/\.exe$/i, '')
}

const assocLabel = computed(() => {
  const a = props.assoc
  if (!a) return '检测中…'
  if (a.is_default) return '✅ 本程序'
  if (a.current_prog_id) return prettyProgId(a.current_prog_id)
  return '未设置'
})

const assocHint = computed(() => {
  const a = props.assoc
  if (!a) return '正在读取系统关联状态…'
  if (a.is_default) return '双击 .md / .markdown 文件会用本程序打开。'
  if (a.user_choice_locked)
    return 'Windows 会锁定你手动选择过的默认程序，第三方程序无法直接改写。点「设为默认」会打开系统的「默认应用」页面，请在那里选择「Markdown 编辑器」。'
  return '点「设为默认」，双击 .md / .markdown 文件就会用本程序打开。'
})
</script>

<template>
  <div v-if="visible" class="modal-mask" @click.self="emit('close')">
    <div class="modal">
      <h3>⚙️ 设置</h3>

      <label class="field">
        <span>主题模式</span>
        <select :value="settings.themeMode" @change="onThemeMode">
          <option value="auto">跟随系统</option>
          <option value="light">浅色</option>
          <option value="dark">深色</option>
        </select>
      </label>

      <label class="field">
        <span>编辑器字体大小</span>
        <select :value="String(settings.fontSize)" @change="onFontSize">
          <option v-for="n in [12, 13, 14, 15, 16, 18, 20]" :key="n" :value="String(n)">
            {{ n }}px
          </option>
        </select>
      </label>

      <label class="field">
        <span>编辑器字体</span>
        <select :value="settings.fontFamily" @change="onFontFamily">
          <option v-for="o in FONT_OPTIONS" :key="o.id" :value="o.id">{{ o.label }}</option>
        </select>
      </label>

      <div class="field">
        <span>当前版本</span>
        <span class="version">{{ version ? `v${version}` : '—' }}</span>
      </div>

      <div class="divider"></div>

      <div class="section-title">📄 文件关联</div>

      <div class="field">
        <span>.md 默认打开方式</span>
        <span class="version" :class="{ ok: assoc?.is_default }">{{ assocLabel }}</span>
      </div>

      <p class="hint">{{ assocHint }}</p>

      <div class="assoc-actions">
        <button
          class="primary"
          :disabled="assocBusy || assoc?.is_default"
          @click="emit('set-default')"
        >
          设为默认
        </button>
        <button :disabled="assocBusy" @click="emit('open-with')">用「打开方式」选择</button>
        <button :disabled="assocBusy" title="重新写入注册表关联" @click="emit('repair-assoc')">
          修复关联
        </button>
      </div>

      <p v-if="assocMessage" class="msg">{{ assocMessage }}</p>

      <div class="modal-actions">
        <button @click="emit('check-update')">检查更新</button>
        <button @click="restoreDefaults">恢复默认</button>
        <button class="primary" @click="emit('close')">关闭</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-mask {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.modal {
  width: 380px;
  max-height: 86vh;
  overflow-y: auto;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 18px 20px;
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.25);
}

.modal h3 {
  margin: 0 0 14px;
  font-size: 15px;
  color: var(--text);
}

.field {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
  font-size: 13px;
  color: var(--text);
}

.field select {
  padding: 4px 8px;
  font-size: 13px;
  font-family: inherit;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--btn-bg);
  color: var(--text);
}

.field .version {
  font-size: 13px;
  color: var(--muted);
}

.field .version.ok {
  color: var(--ok);
}

.divider {
  height: 1px;
  margin: 14px 0;
  background: var(--border);
}

.section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
  margin-bottom: 10px;
}

.hint {
  margin: 2px 0 10px;
  font-size: 12px;
  line-height: 1.55;
  color: var(--muted);
}

.assoc-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.assoc-actions button {
  padding: 5px 12px;
  font-size: 12px;
  font-family: inherit;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--btn-bg);
  color: var(--text);
  cursor: pointer;
}

.assoc-actions button:hover:not(:disabled) {
  background: var(--btn-hover);
}

.assoc-actions button:disabled {
  opacity: 0.55;
  cursor: default;
}

.assoc-actions .primary {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}

.assoc-actions .primary:hover:not(:disabled) {
  filter: brightness(1.1);
}

.msg {
  margin: 10px 0 0;
  font-size: 12px;
  line-height: 1.55;
  color: var(--accent);
}

.modal-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
  padding-top: 14px;
  border-top: 1px solid var(--border);
}

.modal-actions button {
  padding: 5px 14px;
  font-size: 13px;
  font-family: inherit;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--btn-bg);
  color: var(--text);
  cursor: pointer;
}

.modal-actions button:hover {
  background: var(--btn-hover);
}

.modal-actions .primary {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}

.modal-actions .primary:hover {
  filter: brightness(1.1);
}
</style>
