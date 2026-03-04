<template>
  <div class="mde-shell" :class="toneClass">
    <header class="mde-header">
      <div class="doc-info">
        <div class="doc-title-row">
          <h2 class="doc-title">{{ node.name }}</h2>
          <span v-if="isDirty" class="dirty-dot" title="存在未保存修改"></span>
        </div>
        <div class="doc-meta">
          <span>更新 {{ fmtDate(node.updated_at) }}</span>
          <span class="dot">·</span>
          <span>创建 {{ fmtDate(node.created_at) }}</span>
          <span class="dot">·</span>
          <span>{{ wordCount }} 字</span>
        </div>
      </div>

      <div class="mode-switch" role="tablist" aria-label="编辑模式">
        <button class="mode-btn" :class="{ active: viewMode === 'edit' }" @click="viewMode = 'edit'">编辑</button>
        <button class="mode-btn" :class="{ active: viewMode === 'split' }" @click="viewMode = 'split'">分栏</button>
        <button class="mode-btn" :class="{ active: viewMode === 'preview' }" @click="viewMode = 'preview'">预览</button>
      </div>

      <div class="actions">
        <button class="secondary-btn" @click="emit('share', node)">分享</button>
        <button class="save-btn" :class="{ dirty: isDirty }" :disabled="saving" @click="save">
          <span>{{ saving ? '保存中...' : '保存' }}</span>
          <kbd>⌘S</kbd>
        </button>
      </div>
    </header>

    <div class="editor-host">
      <VMdEditor
        v-model="draft"
        :mode="editorMode"
        :height="'100%'"
        :left-toolbar="leftToolbar"
        :right-toolbar="rightToolbar"
        :disabled-menus="disabledMenus"
        :tab-size="2"
        :codemirror-config="codemirrorConfig"
        :placeholder="'开始输入 Markdown...'"
        @save="save"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import VMdEditor from '@kangc/v-md-editor/lib/codemirror-editor'
import githubTheme from '@kangc/v-md-editor/lib/theme/github.js'
import hljs from 'highlight.js'
import Codemirror from 'codemirror'

import 'codemirror/lib/codemirror.css'
import 'codemirror/addon/scroll/simplescrollbars.css'
import 'codemirror/mode/markdown/markdown'
import 'codemirror/addon/display/placeholder'
import 'codemirror/addon/selection/active-line'
import 'codemirror/addon/edit/closebrackets'
import 'codemirror/addon/edit/closetag'
import 'codemirror/addon/scroll/simplescrollbars'

import '@kangc/v-md-editor/lib/style/base-editor.css'
import '@kangc/v-md-editor/lib/theme/style/github.css'

import { useDocsStore, type DocNode } from '@/stores/docs'

const vmd = VMdEditor as any
if (!vmd.__markflowConfigured) {
  vmd.Codemirror = Codemirror
  vmd.use(githubTheme, { Hljs: hljs })
  vmd.__markflowConfigured = true
}

const props = defineProps<{ node: DocNode }>()
const emit = defineEmits<{ share: [node: DocNode] }>()

const docs = useDocsStore()
const saving = ref(false)
const isDirty = ref(false)
const isSystemDark = ref(true)
const draft = ref(props.node.content || '')
const viewMode = ref<'edit' | 'split' | 'preview'>('split')
let originalContent = props.node.content || ''
let colorSchemeQuery: MediaQueryList | null = null
let colorSchemeListener: ((event: MediaQueryListEvent) => void) | null = null

const editorMode = computed(() => (viewMode.value === 'split' ? 'editable' : viewMode.value))
const toneClass = computed(() => (isSystemDark.value ? 'tone-dark' : 'tone-light'))
const wordCount = computed(() => {
  const text = draft.value
  const cjk = (text.match(/[\u4e00-\u9fff\u3040-\u30ff\uac00-\ud7ff]/g) || []).length
  const words = (text.match(/[a-zA-Z]+/g) || []).length
  return cjk + words
})

const leftToolbar = 'undo redo clear | h bold italic strikethrough quote | ul ol table hr | link image code'
const rightToolbar = 'fullscreen'
const disabledMenus = ['image/upload-image']
const codemirrorConfig = {
  indentUnit: 2,
  tabSize: 2,
  indentWithTabs: false,
  lineWrapping: true,
  scrollbarStyle: 'native',
}

function fmtDate(dateString: string) {
  const date = new Date(dateString.endsWith('Z') ? dateString : `${dateString}Z`)
  const MM = `${date.getMonth() + 1}`.padStart(2, '0')
  const dd = `${date.getDate()}`.padStart(2, '0')
  const hh = `${date.getHours()}`.padStart(2, '0')
  const mm = `${date.getMinutes()}`.padStart(2, '0')
  return `${MM}/${dd} ${hh}:${mm}`
}

watch(draft, (value) => {
  isDirty.value = value !== originalContent
})

watch(
  () => props.node.id,
  () => {
    const next = props.node.content || ''
    draft.value = next
    originalContent = next
    isDirty.value = false
  }
)

watch(
  () => props.node.content,
  (value) => {
    const next = value || ''
    if (!isDirty.value && draft.value !== next) {
      draft.value = next
      originalContent = next
    }
  }
)

async function save() {
  if (saving.value) return

  saving.value = true
  try {
    await docs.updateNode(props.node.id, { content: draft.value })
    originalContent = draft.value
    isDirty.value = false
    ElMessage({
      message: '已保存',
      type: 'success',
      duration: 1200,
      offset: 60,
    })
  } catch {
    ElMessage.error('保存失败')
  } finally {
    saving.value = false
  }
}

function handleBeforeUnload(event: BeforeUnloadEvent) {
  if (!isDirty.value) return
  event.preventDefault()
  event.returnValue = ''
}

function handleSaveHotkey(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
    event.preventDefault()
    save()
  }
}

onMounted(() => {
  colorSchemeQuery = window.matchMedia('(prefers-color-scheme: dark)')
  isSystemDark.value = colorSchemeQuery.matches
  colorSchemeListener = (event) => {
    isSystemDark.value = event.matches
  }

  if (colorSchemeQuery.addEventListener) {
    colorSchemeQuery.addEventListener('change', colorSchemeListener)
  } else if (colorSchemeQuery.addListener) {
    colorSchemeQuery.addListener(colorSchemeListener)
  }

  window.addEventListener('beforeunload', handleBeforeUnload)
  window.addEventListener('keydown', handleSaveHotkey)
})

onUnmounted(() => {
  window.removeEventListener('beforeunload', handleBeforeUnload)
  window.removeEventListener('keydown', handleSaveHotkey)

  if (colorSchemeQuery && colorSchemeListener) {
    if (colorSchemeQuery.removeEventListener) {
      colorSchemeQuery.removeEventListener('change', colorSchemeListener)
    } else if (colorSchemeQuery.removeListener) {
      colorSchemeQuery.removeListener(colorSchemeListener)
    }
  }
})
</script>

<style scoped>
.mde-shell {
  --mde-font: "Plus Jakarta Sans", "Noto Sans SC", "PingFang SC", "Microsoft YaHei", sans-serif;
  --mde-mono: "JetBrains Mono", "Fira Code", "Cascadia Code", "Consolas", monospace;
  --mde-radius: 10px;

  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  font-family: var(--mde-font);
}

.mde-shell.tone-dark {
  --mde-panel: #0b1728;
  --mde-panel-2: #102036;
  --mde-editor-bg: #091324;
  --mde-border: rgba(128, 157, 192, 0.2);
  --mde-border-strong: rgba(127, 180, 255, 0.4);
  --mde-text: #dfe9f8;
  --mde-text-soft: #9cb0cc;
  --mde-muted: #7c8fa9;
  --mde-accent: #5fa8ff;
}

.mde-shell.tone-light {
  --mde-panel: #ffffff;
  --mde-panel-2: #f7faff;
  --mde-editor-bg: #ffffff;
  --mde-border: rgba(18, 39, 68, 0.14);
  --mde-border-strong: rgba(47, 136, 247, 0.35);
  --mde-text: #12213a;
  --mde-text-soft: #40587b;
  --mde-muted: #6f82a0;
  --mde-accent: #2f88f7;
}

.mde-header {
  display: grid;
  grid-template-columns: minmax(180px, 300px) auto auto;
  align-items: center;
  gap: 10px;
  min-height: 56px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--mde-border);
  background: var(--mde-panel-2);
}

.doc-info {
  min-width: 0;
}

.doc-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.doc-title {
  margin: 0;
  font-size: 14px;
  line-height: 1.2;
  font-weight: 700;
  color: var(--mde-text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.dirty-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #ff9f43;
  box-shadow: 0 0 0 5px rgba(255, 159, 67, 0.14);
}

.doc-meta {
  margin-top: 4px;
  display: flex;
  gap: 6px;
  align-items: center;
  font-size: 12px;
  color: var(--mde-muted);
}

.dot {
  opacity: 0.5;
}

.mode-switch {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px;
  border-radius: 10px;
  border: 1px solid var(--mde-border);
  background: var(--mde-panel);
}

.mode-btn {
  height: 28px;
  min-width: 46px;
  border: 0;
  border-radius: 7px;
  background: transparent;
  color: var(--mde-text-soft);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.mode-btn.active {
  color: #fff;
  background: var(--mde-accent);
}

.actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.secondary-btn,
.save-btn {
  height: 34px;
  border-radius: 10px;
  border: 1px solid var(--mde-border);
  padding: 0 12px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.secondary-btn {
  color: var(--mde-text-soft);
  background: var(--mde-panel);
}

.secondary-btn:hover {
  border-color: var(--mde-border-strong);
  color: var(--mde-text);
}

.save-btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  border: none;
  color: #f8fffb;
  background: linear-gradient(135deg, #31b36b, #1e9352);
}

.save-btn.dirty {
  background: linear-gradient(135deg, #f59f3c, #dd7f20);
}

.save-btn:disabled {
  opacity: 0.7;
  cursor: not-allowed;
}

.save-btn kbd {
  font-family: var(--mde-mono);
  font-size: 11px;
  border-radius: 6px;
  padding: 2px 5px;
  border: 1px solid rgba(255, 255, 255, 0.35);
  background: rgba(255, 255, 255, 0.12);
}

.editor-host {
  flex: 1;
  min-height: 0;
}

:deep(.v-md-editor) {
  height: 100%;
  border-radius: 0;
  box-shadow: none;
  border: none;
  background: var(--mde-panel);
  color: var(--mde-text);
}

:deep(.v-md-editor__toolbar) {
  padding: 6px 8px;
  border-bottom: 1px solid var(--mde-border);
  background: var(--mde-panel);
}

:deep(.v-md-editor__toolbar-item) {
  color: var(--mde-text-soft);
}

:deep(.v-md-editor__toolbar-item:hover) {
  background: color-mix(in srgb, var(--mde-accent) 12%, transparent);
  color: var(--mde-text);
}

:deep(.v-md-editor__toolbar-item--active),
:deep(.v-md-editor__toolbar-item--active:hover) {
  background: color-mix(in srgb, var(--mde-accent) 22%, transparent);
}

:deep(.v-md-editor--editable .v-md-editor__editor-wrapper) {
  border-right: 1px solid var(--mde-border);
}

:deep(.CodeMirror) {
  height: 100%;
  background: var(--mde-editor-bg);
  color: var(--mde-text);
  font-family: var(--mde-mono);
  font-size: 14px;
  line-height: 1.7;
}

:deep(.CodeMirror-gutters) {
  min-width: 38px;
  border-right: 1px solid var(--mde-border);
  background: var(--mde-editor-bg);
}

:deep(.CodeMirror-linenumber) {
  min-width: 26px;
  padding: 0 8px 0 4px;
  color: var(--mde-muted);
}

:deep(.CodeMirror-lines) {
  padding: 14px 0 28px;
}

:deep(.CodeMirror pre) {
  padding-left: 4px;
}

:deep(.v-md-editor__preview-wrapper) {
  background: var(--mde-panel);
}

:deep(.github-markdown-body) {
  padding: 18px 24px 32px !important;
  color: var(--mde-text) !important;
  background: transparent !important;
}

:deep(.github-markdown-body pre) {
  border-radius: var(--mde-radius);
  border: 1px solid var(--mde-border);
  padding: 12px 14px !important;
}

:deep(.github-markdown-body code) {
  font-family: var(--mde-mono);
}

@media (max-width: 980px) {
  .mde-header {
    grid-template-columns: 1fr;
    gap: 8px;
  }

  .actions {
    justify-content: flex-end;
  }
}

@media (max-width: 768px) {
  .doc-meta {
    display: none;
  }
}
</style>
