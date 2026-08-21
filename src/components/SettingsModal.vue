<script setup lang="ts">
import { DEFAULT_SETTINGS, FONT_OPTIONS } from '../types'
import type { AppSettings, ThemeMode } from '../types'

defineProps<{ visible: boolean; settings: AppSettings }>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'change', patch: Partial<AppSettings>): void
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

      <div class="modal-actions">
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
  width: 320px;
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

.modal-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
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
