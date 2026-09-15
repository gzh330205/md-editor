// 共享类型定义

/** 文件树目录条目（对应 Rust 侧 DirEntry） */
export interface DirEntry {
  name: string
  path: string
  is_dir: boolean
}

/** 主题模式 */
export type ThemeMode = 'auto' | 'light' | 'dark'

/** 应用设置 */
export interface AppSettings {
  themeMode: ThemeMode
  fontSize: number
  fontFamily: string
}

/** Windows 文件关联状态（对应 Rust 侧 AssociationStatus） */
export interface AssociationStatus {
  exe: string
  registered: boolean
  is_default: boolean
  current_prog_id: string | null
  user_choice_locked: boolean
}

/** 「设为默认」的返回结果 */
export interface SetDefaultResult {
  ok: boolean
  need_confirm: boolean
  settings_opened: boolean
}

export const DEFAULT_SETTINGS: AppSettings = {
  themeMode: 'auto',
  fontSize: 14,
  fontFamily: 'default',
}

/** 编辑器字体选项 */
export const FONT_OPTIONS: { id: string; label: string; family: string }[] = [
  { id: 'default', label: '默认等宽 (Consolas)', family: "Consolas, 'Courier New', monospace" },
  { id: 'jetbrains', label: 'JetBrains Mono', family: "'JetBrains Mono', Consolas, monospace" },
  { id: 'fira', label: 'Fira Code', family: "'Fira Code', Consolas, monospace" },
  { id: 'cascadia', label: 'Cascadia Code', family: "'Cascadia Code', Consolas, monospace" },
]

export function fontFamilyFor(id: string): string {
  return FONT_OPTIONS.find((o) => o.id === id)?.family ?? FONT_OPTIONS[0].family
}
