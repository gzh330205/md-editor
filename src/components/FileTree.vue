<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { DirEntry } from '../types'

const props = defineProps<{ root: string | null }>()
const emit = defineEmits<{
  (e: 'open-file', path: string): void
}>()

const dirMap = reactive(new Map<string, DirEntry[]>())
const expanded = ref(new Set<string>())
const loading = ref(false)
const treeError = ref('')

async function loadDir(path: string) {
  if (dirMap.has(path)) return
  loading.value = true
  treeError.value = ''
  try {
    const list = await invoke<DirEntry[]>('list_dir', { path })
    dirMap.set(path, list)
  } catch (e) {
    treeError.value = String(e)
  }
  loading.value = false
}

function toggle(entry: DirEntry) {
  if (!entry.is_dir) {
    emit('open-file', entry.path)
    return
  }
  if (expanded.value.has(entry.path)) {
    expanded.value.delete(entry.path)
  } else {
    expanded.value.add(entry.path)
    void loadDir(entry.path)
  }
}

function refresh() {
  if (!props.root) return
  dirMap.clear()
  expanded.value.clear()
  void loadDir(props.root)
  if (props.root) expanded.value.add(props.root)
}

watch(
  () => props.root,
  (root) => {
    dirMap.clear()
    expanded.value.clear()
    treeError.value = ''
    if (root) {
      expanded.value.add(root)
      void loadDir(root)
    }
  },
  { immediate: true },
)

const rows = computed(() => {
  const out: { entry: DirEntry; depth: number }[] = []
  const walk = (path: string, depth: number) => {
    const list = dirMap.get(path)
    if (!list) return
    for (const entry of list) {
      out.push({ entry, depth })
      if (entry.is_dir && expanded.value.has(entry.path)) walk(entry.path, depth + 1)
    }
  }
  if (props.root) walk(props.root, 0)
  return out
})

const rootName = computed(() => {
  if (!props.root) return ''
  const parts = props.root.split(/[\\/]/)
  return parts[parts.length - 1] || props.root
})
</script>

<template>
  <div class="file-tree">
    <div class="tree-header">
      <span class="tree-title" :title="root || ''">📁 {{ rootName || '未选择文件夹' }}</span>
      <button class="tree-refresh" title="刷新" @click="refresh">⟳</button>
    </div>
    <div v-if="treeError" class="tree-error">{{ treeError }}</div>
    <div class="tree-body">
      <template v-if="rows.length">
        <div
          v-for="{ entry, depth } in rows"
          :key="entry.path"
          class="tree-row"
          :class="{ dir: entry.is_dir }"
          :style="{ paddingLeft: 8 + depth * 14 + 'px' }"
          :title="entry.path"
          @click="toggle(entry)"
        >
          <span class="tree-icon">{{ entry.is_dir ? (expanded.has(entry.path) ? '▾' : '▸') : '·' }}</span>
          <span class="tree-name">{{ entry.name }}</span>
        </div>
      </template>
      <div v-else-if="loading" class="tree-hint">加载中…</div>
      <div v-else-if="props.root" class="tree-hint">（空目录）</div>
      <div v-else class="tree-hint">点击工具栏 📁 选择文件夹</div>
    </div>
  </div>
</template>

<style scoped>
.file-tree {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 0;
  font-size: 13px;
  user-select: none;
}

.tree-header {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 34px;
  padding: 0 8px;
  border-bottom: 1px solid var(--border);
  flex: none;
}

.tree-title {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text);
  font-weight: 600;
}

.tree-refresh {
  border: none;
  background: none;
  color: var(--muted);
  font-size: 14px;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
}

.tree-refresh:hover {
  background: var(--btn-hover);
  color: var(--text);
}

.tree-body {
  flex: 1;
  overflow: auto;
  padding: 4px 0;
}

.tree-row {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding-right: 8px;
  cursor: pointer;
  white-space: nowrap;
}

.tree-row:hover {
  background: var(--btn-hover);
}

.tree-icon {
  width: 14px;
  flex: none;
  text-align: center;
  color: var(--muted);
  font-size: 11px;
}

.tree-name {
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--text);
}

.tree-error {
  padding: 8px;
  color: #cf222e;
  font-size: 12px;
  border-bottom: 1px solid var(--border);
}

.tree-hint {
  padding: 12px;
  color: var(--muted);
  font-size: 12px;
}
</style>
