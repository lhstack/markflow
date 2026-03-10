<template>
  <div class="sy-editor-shell" :class="{ 'is-fullscreen': isFullscreen }">
    <header class="sy-editor-header">
      <div class="sy-editor-header-inner">
        <div class="sy-doc-meta">
          <div class="sy-doc-kicker">MARKDOWN NOTE</div>
          <div class="sy-doc-title-row">
            <h2 class="sy-doc-title">{{ node.name }}</h2>
            <span v-if="isDirty" class="sy-dirty-chip">未保存</span>
          </div>
          <div class="sy-doc-subline">
            <span>更新 {{ fmtDate(node.updated_at) }}</span>
            <span class="dot">·</span>
            <span>创建 {{ fmtDate(node.created_at) }}</span>
            <span class="dot">·</span>
            <span>{{ wordCount }} 字</span>
          </div>
        </div>

        <div class="sy-editor-actions">
          <div class="sy-shortcut-hint">上传、拖拽、粘贴都可插入附件</div>
          <button class="ghost-btn" @click="emit('share', node)">分享</button>
          <button
            v-if="isDirty"
            class="ghost-btn danger"
            :disabled="saving"
            @click="discardChanges"
          >
            <span>取消保存</span>
          </button>
          <button class="save-btn" :class="{ dirty: isDirty }" :disabled="saving" @click="save">
            <span>{{ saving ? '保存中...' : '保存' }}</span>
            <kbd>⌘S</kbd>
          </button>
        </div>
      </div>
    </header>

    <div
      v-if="assetUploadTask"
      class="sy-upload-banner"
      :class="{
        uploading: assetUploadTask.status === 'uploading',
        error: assetUploadTask.status === 'error',
      }"
    >
      <div class="sy-upload-banner-inner">
        <div class="sy-upload-main">
          <span class="sy-upload-title">{{ uploadBannerTitle }}</span>
          <span class="sy-upload-name">{{ assetUploadTask.fileName }}</span>
        </div>
        <div class="sy-upload-side">
          <div v-if="assetUploadTask.status === 'uploading'" class="sy-upload-progress">
            <span>{{ assetUploadTask.progress }}%</span>
            <el-progress :percentage="assetUploadTask.progress" :stroke-width="6" :show-text="false" />
          </div>
          <span v-else-if="assetUploadTask.status === 'error'" class="sy-upload-error">
            {{ assetUploadTask.error || '上传失败' }}
          </span>
          <span v-else class="sy-upload-success">已插入当前文档</span>
          <button
            v-if="assetUploadTask.status !== 'uploading'"
            class="sy-upload-close"
            @click="clearAssetUploadTask"
          >
            关闭
          </button>
        </div>
      </div>
    </div>

    <div class="sy-editor-stage">
      <div class="sy-editor-stage-inner">
        <div class="sy-split-shell" :class="`mode-${viewMode}`">
          <div class="sy-view-switch" role="tablist" aria-label="视图切换">
            <button
              class="sy-view-btn"
              :class="{ active: viewMode === 'source' }"
              @click="viewMode = 'source'"
            >
              源码
            </button>
            <button
              class="sy-view-btn"
              :class="{ active: viewMode === 'preview' }"
              @click="viewMode = 'preview'"
            >
              预览
            </button>
            <button
              class="sy-view-btn"
              :class="{ active: viewMode === 'split' }"
              @click="viewMode = 'split'"
            >
              分栏
            </button>
          </div>
          <section class="sy-editor-pane">
            <div ref="editorRef" class="sy-editor-host"></div>
          </section>
          <aside class="sy-preview-pane">
            <div ref="previewBodyRef" class="sy-preview-body">
              <VditorPreview :markdown="draft" @rendered="syncPreviewScrollFromEditor" />
            </div>
          </aside>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import Vditor from 'vditor'
import 'vditor/dist/index.css'

import { useDocsStore, type DocNode } from '@/stores/docs'
import { useSystemStore } from '@/stores/system'
import { VDITOR_CDN } from '@/utils/vditor'
import {
  AGENT_WRITER_CHUNK_EVENT,
  AGENT_WRITER_COMPLETE_EVENT,
  AGENT_WRITER_START_EVENT,
  clearAgentEditorSnapshot,
  setAgentEditorSnapshot,
  type AgentWriterChunkDetail,
  type AgentWriterCompleteDetail,
  type AgentWriterMode,
  type AgentWriterStartDetail,
} from '@/utils/agentWriter'
import {
  registerAgentEditorBridge,
  unregisterAgentEditorBridge,
} from '@/utils/agentEditorBridge'
import {
  clearDocDraftContent,
  getDocDraftContent,
  hasDocDraft,
  setDocDraftContent,
} from '@/utils/docDraftCache'
import VditorPreview from '@/components/VditorPreview.vue'
import { createManagedUploadTask, removeManagedUpload, type ManagedUploadTask } from '@/utils/managedUploads'
import { uploadFile, uploadImage } from '@/utils/uploads'

const props = defineProps<{ node: DocNode }>()
const emit = defineEmits<{ share: [node: DocNode] }>()

const docs = useDocsStore()
const system = useSystemStore()
const editorRef = ref<HTMLDivElement | null>(null)
const previewBodyRef = ref<HTMLDivElement | null>(null)
const saving = ref(false)
const isDirty = ref(false)
const draft = ref(props.node.content || '')
const assetUploadTask = ref<ManagedUploadTask | null>(null)
const viewMode = ref<'source' | 'preview' | 'split'>('split')
const isFullscreen = ref(false)
let originalContent = props.node.content || ''
let editor: Vditor | null = null
let editorScrollEl: HTMLElement | null = null
let scrollSyncLocked = false
let scrollUnlockFrame = 0
let writerBuffer = ''
let writerTargetDocId: number | null = null
let writerShouldSave = false
let writerMode: AgentWriterMode | null = null
let writerRenderFrame: number | null = null
let writerRenderedValue = ''

const wordCount = computed(() => {
  const text = draft.value
  const cjk = (text.match(/[\u4e00-\u9fff\u3040-\u30ff\uac00-\ud7ff]/g) || []).length
  const words = (text.match(/[a-zA-Z]+/g) || []).length
  return cjk + words
})

const uploadBannerTitle = computed(() => {
  if (!assetUploadTask.value) return ''
  if (assetUploadTask.value.status === 'uploading') return '附件上传中'
  if (assetUploadTask.value.status === 'error') return '附件上传失败'
  return '附件上传完成'
})

function fmtDate(dateString: string) {
  const date = new Date(dateString.endsWith('Z') ? dateString : `${dateString}Z`)
  const MM = `${date.getMonth() + 1}`.padStart(2, '0')
  const dd = `${date.getDate()}`.padStart(2, '0')
  const hh = `${date.getHours()}`.padStart(2, '0')
  const mm = `${date.getMinutes()}`.padStart(2, '0')
  return `${MM}/${dd} ${hh}:${mm}`
}

function syncDraftFromEditor() {
  if (!editor) return
  const value = editor.getValue()
  draft.value = value
  isDirty.value = value !== originalContent
  syncDocDraftCache(props.node.id, value, originalContent)
  setAgentEditorSnapshot(props.node.id, value)
}

function resolveInitialContent(docId: number, savedContent: string) {
  if (!hasDocDraft(docId)) return savedContent
  const cached = getDocDraftContent(docId)
  if (cached === null) return savedContent
  return cached
}

function syncDocDraftCache(docId: number, value: string, savedContent: string) {
  if (value === savedContent) {
    clearDocDraftContent(docId)
    return
  }
  setDocDraftContent(docId, value)
}

function persistCurrentDraft(docId = props.node.id) {
  syncDocDraftCache(docId, draft.value, originalContent)
}

function getEditorScrollElement() {
  if (!editorRef.value) return null
  return (
    (editorRef.value.querySelector('.vditor-sv') as HTMLElement | null) ||
    (editorRef.value.querySelector('.vditor-content') as HTMLElement | null)
  )
}

function lockScrollSync() {
  scrollSyncLocked = true
  if (scrollUnlockFrame) {
    cancelAnimationFrame(scrollUnlockFrame)
  }
  scrollUnlockFrame = requestAnimationFrame(() => {
    scrollSyncLocked = false
  })
}

function syncScrollPosition(source: HTMLElement, target: HTMLElement) {
  const sourceMax = source.scrollHeight - source.clientHeight
  const targetMax = target.scrollHeight - target.clientHeight
  if (sourceMax <= 0 || targetMax <= 0) return
  const ratio = source.scrollTop / sourceMax
  target.scrollTop = ratio * targetMax
}

function syncPreviewScrollFromEditor() {
  if (viewMode.value !== 'split' || scrollSyncLocked) return
  const previewBody = previewBodyRef.value
  const source = editorScrollEl || getEditorScrollElement()
  if (!previewBody || !source) return
  lockScrollSync()
  syncScrollPosition(source, previewBody)
}

function syncEditorScrollFromPreview() {
  if (viewMode.value !== 'split' || scrollSyncLocked) return
  const previewBody = previewBodyRef.value
  const source = editorScrollEl || getEditorScrollElement()
  if (!previewBody || !source) return
  lockScrollSync()
  syncScrollPosition(previewBody, source)
}

function bindScrollSync() {
  editorScrollEl?.removeEventListener('scroll', syncPreviewScrollFromEditor)
  previewBodyRef.value?.removeEventListener('scroll', syncEditorScrollFromPreview)

  editorScrollEl = getEditorScrollElement()
  editorScrollEl?.addEventListener('scroll', syncPreviewScrollFromEditor, { passive: true })
  previewBodyRef.value?.addEventListener('scroll', syncEditorScrollFromPreview, { passive: true })
}

function scrollEditorToBottom() {
  const target = editorScrollEl || getEditorScrollElement()
  if (!target) return
  target.scrollTop = target.scrollHeight
}

function syncAgentEditorBridge() {
  if (!editor) return
  registerAgentEditorBridge({
    docId: props.node.id,
    docName: props.node.name,
    getValue: () => editor?.getValue() || draft.value,
    setValue: (value: string) => {
      applyWriterValue(value)
    },
    insertValue: (value: string) => {
      if (!editor) return
      editor.insertValue(value)
      syncDraftFromEditor()
    },
    appendValue: (value: string) => {
      if (!editor) return
      applyWriterValue(`${editor.getValue()}${value}`)
    },
    replaceValue: (value: string) => {
      applyWriterValue(value)
    },
    focus: () => {
      editor?.focus()
    },
    scrollToBottom: () => {
      scrollEditorToBottom()
    },
    save,
  })
}

function beginAssetUpload(kind: 'doc-image' | 'doc-file', file: File) {
  if (assetUploadTask.value) {
    removeManagedUpload(assetUploadTask.value.id)
  }
  assetUploadTask.value = createManagedUploadTask(kind, file)
  return assetUploadTask.value
}

function clearAssetUploadTask() {
  if (assetUploadTask.value) {
    removeManagedUpload(assetUploadTask.value.id)
    assetUploadTask.value = null
  }
}

function syncFullscreenLock() {
  document.body.classList.toggle('sy-editor-fullscreen-open', isFullscreen.value)
  document.documentElement.classList.toggle('sy-editor-fullscreen-open', isFullscreen.value)
}

function fullscreenIconMarkup(active: boolean) {
  return active
    ? '<svg viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M5.25 2.75H2.75v2.5M10.75 2.75h2.5v2.5M5.25 13.25H2.75v-2.5M10.75 13.25h2.5v-2.5" stroke="currentColor" stroke-width="1.45" stroke-linecap="round" stroke-linejoin="round"/></svg>'
    : '<svg viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M6 2.75H2.75V6M10 2.75h3.25V6M6 13.25H2.75V10M10 13.25h3.25V10" stroke="currentColor" stroke-width="1.45" stroke-linecap="round" stroke-linejoin="round"/></svg>'
}

function syncFullscreenToolbarButton() {
  const button = editorRef.value?.querySelector('[data-type="sy-fullscreen"]') as HTMLElement | null
  if (!button) return
  button.innerHTML = fullscreenIconMarkup(isFullscreen.value)
  button.setAttribute('aria-label', isFullscreen.value ? '退出全屏' : '进入全屏')
}

function applyWriterValue(value: string) {
  if (!editor) return
  draft.value = value
  isDirty.value = value !== originalContent
  editor.setValue(value, true)
  writerRenderedValue = value
  setAgentEditorSnapshot(props.node.id, value)
  scrollEditorToBottom()
  syncAgentEditorBridge()
}

function isPartialWriterMode(mode: AgentWriterMode | null) {
  return mode === 'rewrite_section' || mode === 'replace_block'
}

function extractTaggedContent(source: string, tag: string) {
  const pattern = new RegExp(`\\[\\[${tag}\\]\\]([\\s\\S]*?)\\[\\[\\/${tag}\\]\\]`, 'i')
  const match = source.match(pattern)
  return match?.[1]?.trim() || ''
}

function normalizeHeadingText(value: string) {
  return value.replace(/^#{1,6}\s*/, '').trim()
}

function applyRewriteSection(original: string, payload: string) {
  const targetRaw = extractTaggedContent(payload, 'TARGET')
  const contentRaw = extractTaggedContent(payload, 'CONTENT')
  const target = normalizeHeadingText(targetRaw)
  const sectionContent = (contentRaw || payload).trim()
  if (!target || !sectionContent) return original

  const lines = original.split(/\r?\n/)
  const headingPattern = /^(#{1,6})\s+(.+)$/
  let start = -1
  let level = 0

  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(headingPattern)
    if (!match) continue
    const headingText = normalizeHeadingText(match[2] || '')
    if (headingText === target) {
      start = index
      level = (match[1] || '').length
      break
    }
  }

  if (start === -1) return original

  let end = lines.length
  for (let index = start + 1; index < lines.length; index += 1) {
    const match = lines[index].match(headingPattern)
    if (!match) continue
    const nextLevel = (match[1] || '').length
    if (nextLevel <= level) {
      end = index
      break
    }
  }

  const replacementLines = sectionContent.split(/\r?\n/)
  return [...lines.slice(0, start), ...replacementLines, ...lines.slice(end)]
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
}

function applyReplaceBlock(original: string, payload: string) {
  const find = extractTaggedContent(payload, 'FIND')
  const replacement = extractTaggedContent(payload, 'REPLACE')
  if (!find) return original
  const index = original.indexOf(find)
  if (index === -1) return original
  return `${original.slice(0, index)}${replacement}${original.slice(index + find.length)}`
}

function applyPartialWriterAction(mode: AgentWriterMode, original: string, payload: string) {
  if (mode === 'rewrite_section') {
    return applyRewriteSection(original, payload)
  }
  if (mode === 'replace_block') {
    return applyReplaceBlock(original, payload)
  }
  return original
}

function flushWriterRender() {
  writerRenderFrame = null
  if (!editor || writerTargetDocId !== props.node.id) return
  if (writerRenderedValue === writerBuffer) return
  applyWriterValue(writerBuffer)
}

function scheduleWriterRender() {
  if (writerRenderFrame !== null) return
  writerRenderFrame = window.requestAnimationFrame(() => {
    flushWriterRender()
  })
}

function resetWriterState() {
  writerBuffer = ''
  writerTargetDocId = null
  writerShouldSave = false
  writerMode = null
  writerRenderedValue = ''
  if (writerRenderFrame !== null) {
    window.cancelAnimationFrame(writerRenderFrame)
    writerRenderFrame = null
  }
}

function handleAgentWriterStart(event: Event) {
  const detail = (event as CustomEvent<AgentWriterStartDetail>).detail
  if (!detail || detail.docId !== props.node.id || !editor) return

  resetWriterState()
  writerTargetDocId = detail.docId
  writerShouldSave = detail.save === true
  writerMode = detail.mode

  const currentValue = editor.getValue()
  if (detail.mode === 'replace') {
    writerBuffer = ''
    applyWriterValue(writerBuffer)
    return
  }

  if (detail.mode === 'append') {
    writerBuffer = currentValue
    if (writerBuffer.trim()) {
      if (!writerBuffer.endsWith('\n\n')) {
        writerBuffer = writerBuffer.replace(/\n*$/, '\n\n')
      }
    }
    applyWriterValue(writerBuffer)
    return
  }

  // Partial modes buffer protocol payload and apply once on complete.
  writerBuffer = ''
  writerRenderedValue = currentValue
}

function handleAgentWriterChunk(event: Event) {
  const detail = (event as CustomEvent<AgentWriterChunkDetail>).detail
  if (!detail || detail.docId !== props.node.id || writerTargetDocId !== detail.docId) return
  writerBuffer += detail.chunk
  if (isPartialWriterMode(writerMode)) return
  scheduleWriterRender()
}

function handleAgentWriterComplete(event: Event) {
  const detail = (event as CustomEvent<AgentWriterCompleteDetail>).detail
  if (!detail || detail.docId !== props.node.id || writerTargetDocId !== detail.docId) return
  let finalValue = writerBuffer
  if (writerMode && isPartialWriterMode(writerMode)) {
    const currentValue = editor?.getValue() || draft.value
    finalValue = applyPartialWriterAction(writerMode, currentValue, writerBuffer)
    applyWriterValue(finalValue)
  } else {
    flushWriterRender()
    finalValue = writerBuffer
  }
  const shouldSave = writerShouldSave
  resetWriterState()
  draft.value = finalValue
  isDirty.value = finalValue !== originalContent
  setAgentEditorSnapshot(props.node.id, finalValue)
  if (shouldSave) {
    void save()
  }
}

function toggleFullscreen() {
  isFullscreen.value = !isFullscreen.value
  syncFullscreenLock()
  void nextTick().then(() => {
    syncFullscreenToolbarButton()
    bindScrollSync()
  })
}

async function handleEditorUpload(files: File[]) {
  if (!editor) return '编辑器尚未完成初始化'

  for (const file of files) {
    if (file.size > system.uploadMaxBytes) {
      return `文件 ${file.name} 超过 ${system.uploadLimitLabel} 限制`
    }

    const isImage = file.type.startsWith('image/')
    const kind = isImage ? 'doc-image' : 'doc-file'
    const task = beginAssetUpload(kind, file)

    try {
      const url = isImage
        ? await uploadImage(file, 'doc-image', { task })
        : await uploadFile(file, 'doc-file', { task })

      const markdown = isImage ? `![${file.name}](${url})\n` : `[${file.name}](${url})\n`
      editor.insertValue(markdown)
      syncDraftFromEditor()
    } catch (error: any) {
      return error?.response?.data?.error || error?.message || `文件 ${file.name} 上传失败`
    }
  }

  return null
}

function buildToolbar() {
  return [
    'emoji',
    'headings',
    'bold',
    'italic',
    'strike',
    '|',
    'link',
    'upload',
    '|',
    'list',
    'ordered-list',
    'check',
    '|',
    'quote',
    'line',
    'code',
    'inline-code',
    'table',
    '|',
    'undo',
    'redo',
    {
      name: 'sy-fullscreen',
      tip: '进入全屏',
      tipPosition: 'n',
      icon: fullscreenIconMarkup(false),
      click() {
        toggleFullscreen()
      },
    },
  ] as const
}

async function initEditor() {
  await nextTick()
  if (!editorRef.value) return

  if (editor) {
    editor.destroy()
    editor = null
  }

  const initialValue = resolveInitialContent(props.node.id, draft.value)
  draft.value = initialValue
  isDirty.value = initialValue !== originalContent

  editor = new Vditor(editorRef.value, {
    value: initialValue,
    cdn: VDITOR_CDN,
    tab: '    ',
    mode: 'sv',
    theme: 'classic',
    icon: 'material',
    lang: 'zh_CN',
    minHeight: 520,
    height: '100%',
    width: '100%',
    toolbar: buildToolbar() as unknown as any[],
    toolbarConfig: {
      pin: false,
    },
    resize: {
      enable: false,
      position: 'bottom',
    },
    counter: {
      enable: true,
      type: 'text',
    },
    cache: {
      enable: false,
    },
    placeholder: '开始记录内容，支持 Markdown、拖拽上传、图片粘贴...',
    preview: {
      mode: 'editor',
      actions: [],
      delay: 0,
      markdown: {
        toc: true,
        mark: true,
        footnotes: true,
        autoSpace: true,
        codeBlockPreview: false,
        mathBlockPreview: false,
      },
      hljs: {
        style: 'github',
        lineNumber: false,
      },
      theme: {
        current: 'light',
      },
    },
    upload: {
      accept: 'image/*,.pdf,.doc,.docx,.xls,.xlsx,.ppt,.pptx,.zip,.rar,.7z,.txt,.md,.csv,.json',
      multiple: true,
      max: system.uploadMaxBytes,
      handler: handleEditorUpload,
    },
    after() {
      syncDraftFromEditor()
      editor?.setPreviewMode('editor')
      void nextTick().then(() => {
        syncFullscreenToolbarButton()
        bindScrollSync()
        syncPreviewScrollFromEditor()
        syncAgentEditorBridge()
      })
    },
    input(value) {
      draft.value = value
      isDirty.value = value !== originalContent
      syncDocDraftCache(props.node.id, value, originalContent)
      setAgentEditorSnapshot(props.node.id, value)
      syncAgentEditorBridge()
    },
  })
}

async function save() {
  if (saving.value || !editor) return

  saving.value = true
  try {
    const value = editor.getValue()
    await docs.updateNode(props.node.id, { content: value })
    draft.value = value
    originalContent = value
    isDirty.value = false
    clearDocDraftContent(props.node.id)
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
    syncAgentEditorBridge()
  }
}

async function discardChanges() {
  if (saving.value || !editor || !isDirty.value) return

  try {
    await ElMessageBox.confirm(
      '当前未保存内容将被丢弃，并恢复到上次保存后的版本。是否继续？',
      '取消未保存修改',
      {
        confirmButtonText: '恢复已保存内容',
        cancelButtonText: '继续编辑',
        type: 'warning',
      },
    )
  } catch {
    return
  }

  resetWriterState()
  draft.value = originalContent
  isDirty.value = false
  editor.setValue(originalContent, true)
  clearDocDraftContent(props.node.id)
  setAgentEditorSnapshot(props.node.id, originalContent)
  syncAgentEditorBridge()

  ElMessage({
    message: '已恢复到上次保存的内容',
    type: 'success',
    duration: 1200,
    offset: 60,
  })
}

function handleBeforeUnload(event: BeforeUnloadEvent) {
  if (!isDirty.value) return
  event.preventDefault()
  event.returnValue = ''
}

function handleSaveHotkey(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
    event.preventDefault()
    void save()
  }

  if (event.key === 'Escape' && isFullscreen.value) {
    event.preventDefault()
    toggleFullscreen()
  }
}

watch(
  () => props.node.id,
  async (nextId, previousId) => {
    if (typeof previousId === 'number') {
      persistCurrentDraft(previousId)
      clearAgentEditorSnapshot(previousId)
    }
    resetWriterState()
    const next = resolveInitialContent(nextId, props.node.content || '')
    draft.value = next
    originalContent = props.node.content || ''
    isDirty.value = next !== originalContent
    if (!editor) {
      await initEditor()
      return
    }
    editor.setValue(next, true)
    setAgentEditorSnapshot(props.node.id, next)
    syncAgentEditorBridge()
  }
)

watch(
  () => props.node.content,
  (value) => {
    const next = value || ''
    if (isDirty.value || !editor) return
    const restored = resolveInitialContent(props.node.id, next)
    draft.value = restored
    originalContent = next
    isDirty.value = restored !== originalContent
    if (editor.getValue() !== restored) {
      editor.setValue(restored, true)
    }
    setAgentEditorSnapshot(props.node.id, restored)
    syncAgentEditorBridge()
  }
)

watch(viewMode, async (mode) => {
  if (mode !== 'split') return
  await nextTick()
  bindScrollSync()
  syncPreviewScrollFromEditor()
})

watch(isFullscreen, async () => {
  await nextTick()
  bindScrollSync()
})

onMounted(() => {
  void initEditor()
  window.addEventListener('beforeunload', handleBeforeUnload)
  window.addEventListener('keydown', handleSaveHotkey)
  window.addEventListener(AGENT_WRITER_START_EVENT, handleAgentWriterStart as EventListener)
  window.addEventListener(AGENT_WRITER_CHUNK_EVENT, handleAgentWriterChunk as EventListener)
  window.addEventListener(AGENT_WRITER_COMPLETE_EVENT, handleAgentWriterComplete as EventListener)
  setAgentEditorSnapshot(props.node.id, draft.value)
})

onUnmounted(() => {
  window.removeEventListener('beforeunload', handleBeforeUnload)
  window.removeEventListener('keydown', handleSaveHotkey)
  window.removeEventListener(AGENT_WRITER_START_EVENT, handleAgentWriterStart as EventListener)
  window.removeEventListener(AGENT_WRITER_CHUNK_EVENT, handleAgentWriterChunk as EventListener)
  window.removeEventListener(AGENT_WRITER_COMPLETE_EVENT, handleAgentWriterComplete as EventListener)
  if (isFullscreen.value) {
    isFullscreen.value = false
    syncFullscreenLock()
  }
  resetWriterState()
  persistCurrentDraft()
  unregisterAgentEditorBridge(props.node.id)
  clearAgentEditorSnapshot(props.node.id)
  editorScrollEl?.removeEventListener('scroll', syncPreviewScrollFromEditor)
  previewBodyRef.value?.removeEventListener('scroll', syncEditorScrollFromPreview)
  if (scrollUnlockFrame) {
    cancelAnimationFrame(scrollUnlockFrame)
  }
  clearAssetUploadTask()
  if (editor) {
    editor.destroy()
    editor = null
  }
})
</script>

<style scoped>
:global(html.sy-editor-fullscreen-open),
:global(body.sy-editor-fullscreen-open) {
  overflow: hidden;
}

.sy-editor-shell {
  --sy-paper: linear-gradient(180deg, #fbfdf8 0%, #f6f8ef 100%);
  --sy-line: rgba(31, 41, 27, 0.08);
  --sy-line-strong: rgba(86, 112, 59, 0.18);
  --sy-text: #20251f;
  --sy-text-soft: #586456;
  --sy-text-faint: #7f8d7c;
  --sy-accent: #6f9a4f;
  --sy-accent-deep: #537535;
  --sy-danger: #c45a4d;

  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  background:
    radial-gradient(circle at top left, rgba(196, 214, 171, 0.24), transparent 36%),
    radial-gradient(circle at top right, rgba(242, 235, 203, 0.36), transparent 28%),
    var(--sy-paper);
  color: var(--sy-text);
}

.sy-editor-shell.is-fullscreen {
  position: fixed;
  inset: 0;
  z-index: 2000;
  min-height: 100vh;
  background:
    radial-gradient(circle at top left, rgba(196, 214, 171, 0.24), transparent 36%),
    radial-gradient(circle at top right, rgba(242, 235, 203, 0.36), transparent 28%),
    var(--sy-paper);
}

.sy-editor-header {
  display: flex;
  justify-content: center;
  padding: 14px 20px 12px;
  border-bottom: 1px solid var(--sy-line);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.74), rgba(255, 255, 255, 0.5));
  backdrop-filter: blur(12px);
}

.sy-editor-header-inner {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 18px;
  width: min(1700px, 100%);
}

.sy-doc-meta {
  min-width: 0;
}

.sy-doc-kicker {
  font-size: 11px;
  letter-spacing: 0.18em;
  color: var(--sy-text-faint);
}

.sy-doc-title-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 8px;
}

.sy-doc-title {
  margin: 0;
  font-size: 22px;
  line-height: 1.1;
  font-weight: 700;
  color: #181d17;
}

.sy-dirty-chip {
  padding: 4px 10px;
  border-radius: 999px;
  background: rgba(219, 142, 47, 0.12);
  color: #a86411;
  font-size: 12px;
  font-weight: 700;
}

.sy-doc-subline {
  display: flex;
  align-items: center;
  gap: 7px;
  margin-top: 8px;
  font-size: 12px;
  color: var(--sy-text-soft);
}

.dot {
  opacity: 0.45;
}

.sy-editor-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  justify-content: flex-end;
  align-self: center;
}

.sy-shortcut-hint {
  font-size: 12px;
  color: var(--sy-text-faint);
  margin-right: 6px;
}

.ghost-btn,
.save-btn {
  height: 38px;
  border-radius: 12px;
  padding: 0 14px;
  border: 1px solid var(--sy-line-strong);
  background: rgba(255, 255, 255, 0.82);
  color: var(--sy-text-soft);
  font-size: 13px;
  font-weight: 700;
  cursor: pointer;
}

.ghost-btn:hover {
  border-color: rgba(111, 154, 79, 0.36);
  color: var(--sy-text);
}

.save-btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  border: none;
  color: #f9fff4;
  background: linear-gradient(135deg, var(--sy-accent), var(--sy-accent-deep));
  box-shadow: 0 14px 28px rgba(93, 125, 64, 0.22);
}

.save-btn.dirty {
  background: linear-gradient(135deg, #d0a24a, #b57b21);
}

.save-btn:disabled,
.ghost-btn:disabled {
  opacity: 0.72;
  cursor: not-allowed;
}

.save-btn kbd {
  font-family: var(--mono);
  font-size: 11px;
  border-radius: 6px;
  padding: 2px 5px;
  border: 1px solid rgba(255, 255, 255, 0.28);
  background: rgba(255, 255, 255, 0.12);
}

.sy-upload-banner {
  display: flex;
  justify-content: center;
  padding: 8px 20px;
  border-bottom: 1px solid var(--sy-line);
  background: rgba(243, 247, 235, 0.92);
}

.sy-upload-banner.error {
  background: rgba(196, 90, 77, 0.08);
}

.sy-upload-banner-inner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  width: min(1700px, 100%);
}

.sy-upload-main {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 12px;
}

.sy-upload-title {
  font-size: 12px;
  font-weight: 700;
  color: var(--sy-text);
}

.sy-upload-name {
  min-width: 0;
  font-size: 12px;
  color: var(--sy-text-soft);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sy-upload-side {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 280px;
}

.sy-upload-progress {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  font-size: 12px;
  color: var(--sy-text-soft);
}

.sy-upload-success {
  font-size: 12px;
  color: var(--sy-accent-deep);
}

.sy-upload-error {
  font-size: 12px;
  color: var(--sy-danger);
}

.sy-upload-close {
  border: 0;
  background: transparent;
  color: var(--sy-text-soft);
  cursor: pointer;
  font-size: 12px;
}

.sy-upload-close:hover {
  color: var(--sy-text);
}

.sy-editor-stage {
  flex: 1;
  min-height: 0;
  padding: 20px 20px 26px;
}

.sy-editor-shell.is-fullscreen .sy-editor-stage {
  padding: 12px 14px 16px;
}

.sy-editor-stage-inner {
  height: 100%;
  display: flex;
  justify-content: center;
}

.sy-split-shell {
  position: relative;
  height: 100%;
  width: min(1700px, 100%);
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 0.9fr);
  min-width: 0;
  min-height: 0;
  border-radius: 28px;
  overflow: hidden;
  border: 1px solid rgba(56, 71, 44, 0.08);
  background: rgba(255, 255, 255, 0.88);
  box-shadow:
    inset 0 1px 0 rgba(255, 255, 255, 0.62),
    0 24px 50px rgba(67, 84, 49, 0.1);
}

.sy-split-shell::before {
  content: "";
  position: absolute;
  inset: 0 0 auto 0;
  height: 52px;
  border-bottom: 1px solid var(--sy-line);
  background:
    linear-gradient(90deg, rgba(244, 248, 238, 0.98) 0%, rgba(244, 248, 238, 0.98) 49%, rgba(251, 252, 248, 0.98) 58%, rgba(250, 252, 246, 0.98) 100%);
  pointer-events: none;
  z-index: 0;
}

.sy-view-switch {
  position: absolute;
  top: 0;
  right: 0;
  z-index: 4;
  display: inline-flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  height: 52px;
  min-width: 280px;
  padding: 0 20px;
  background: linear-gradient(90deg, rgba(251, 252, 248, 0) 0%, rgba(251, 252, 248, 0.92) 18%, rgba(250, 252, 246, 0.98) 100%);
}

.sy-view-btn {
  height: 32px;
  padding: 0 12px;
  border: 1px solid transparent;
  border-radius: 999px;
  background: transparent;
  color: var(--sy-text-soft);
  font-size: 12px;
  font-weight: 700;
  cursor: pointer;
  transition:
    background-color 0.18s ease,
    border-color 0.18s ease,
    color 0.18s ease;
}

.sy-view-btn:hover {
  color: var(--sy-text);
  background: rgba(111, 154, 79, 0.1);
}

.sy-view-btn.active {
  border-color: rgba(111, 154, 79, 0.24);
  background: rgba(111, 154, 79, 0.16);
  color: #35501f;
}

.sy-split-shell.mode-source {
  grid-template-columns: minmax(0, 1fr);
}

.sy-split-shell.mode-source .sy-preview-pane {
  display: none;
}

.sy-split-shell.mode-preview {
  grid-template-columns: minmax(0, 1fr);
}

.sy-split-shell.mode-preview .sy-editor-pane {
  display: none;
}

.sy-split-shell.mode-preview .sy-preview-pane {
  border-left: none;
}

.sy-editor-pane,
.sy-preview-pane {
  min-width: 0;
  min-height: 0;
}

.sy-editor-host {
  height: 100%;
  width: 100%;
  min-height: 0;
}

.sy-preview-pane {
  position: relative;
  padding-top: 52px;
  border-left: 1px solid rgba(56, 71, 44, 0.08);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.96), rgba(250, 252, 246, 0.96));
}

.sy-preview-body {
  height: 100%;
  overflow: auto;
  padding: 20px 24px 30px;
}

:deep(.vditor) {
  height: 100%;
  border: none;
  background: transparent;
}

:deep(.vditor-toolbar) {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: center;
  min-height: 54px;
  padding: 0 18px 0 20px;
  border-bottom: none;
  background: transparent;
}

:deep(.vditor-toolbar__item) {
  float: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 54px;
  color: var(--sy-text-soft);
}

:deep(.vditor-toolbar__item .vditor-tooltipped) {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  padding: 0;
  border-radius: 9px;
}

:deep(.vditor-toolbar__item .vditor-tooltipped::before) {
  top: auto;
  right: auto;
  bottom: -5px;
  left: 50%;
  margin-right: 0;
  margin-left: -5px;
  color: rgba(33, 39, 28, 0.92);
  border-top-color: transparent;
  border-bottom-color: rgba(33, 39, 28, 0.92);
}

:deep(.vditor-toolbar__item .vditor-tooltipped::after) {
  top: calc(100% + 9px);
  right: auto;
  bottom: auto;
  left: 50%;
  margin-right: 0;
  margin-left: 0;
  padding: 6px 8px;
  border-radius: 8px;
  background: rgba(33, 39, 28, 0.92);
  color: #f7fbf3;
  font-size: 11px;
  line-height: 1.1;
  white-space: nowrap;
  box-shadow: 0 10px 22px rgba(28, 34, 24, 0.18);
  transform: translateX(-50%);
}

:deep(.vditor-toolbar__item .vditor-panel),
:deep(.vditor-toolbar__item .vditor-panel.vditor-panel--left),
:deep(.vditor-toolbar__item .vditor-hint) {
  top: calc(100% + 8px) !important;
  left: 0;
  right: auto;
  z-index: 8;
}

:deep(.vditor-toolbar__item .vditor-panel.vditor-panel--arrow::before),
:deep(.vditor-toolbar__item .vditor-panel.vditor-panel--left.vditor-panel--arrow::before),
:deep(.vditor-toolbar__item .vditor-hint.vditor-panel--arrow::before) {
  top: -14px;
  left: 18px;
  right: auto;
}

:deep(.vditor-toolbar__item .vditor-hint) {
  min-width: 188px;
  padding: 6px 0;
  border-radius: 12px;
  font-size: 13px;
  line-height: 1.35;
  box-shadow: 0 16px 30px rgba(34, 42, 28, 0.14);
}

:deep(.vditor-toolbar__item .vditor-hint button) {
  padding: 8px 14px;
  font-size: 13px;
  line-height: 1.35;
}

:deep(.vditor-toolbar__item .vditor-menu--current.vditor-tooltipped::before),
:deep(.vditor-toolbar__item .vditor-menu--current.vditor-tooltipped::after) {
  display: none !important;
}

:deep(.vditor-toolbar__item:first-child .vditor-tooltipped::before) {
  top: calc(100% + 4px);
  right: auto;
  bottom: auto;
  left: 12px;
  margin-left: 0;
  display: block;
}

:deep(.vditor-toolbar__item:first-child .vditor-tooltipped::after) {
  top: calc(100% + 9px);
  right: auto;
  bottom: auto;
  left: 0;
  transform: none;
}

:deep(.vditor-toolbar__item .vditor-tooltipped:hover),
:deep(.vditor-toolbar__item .vditor-tooltipped:focus-visible) {
  background: rgba(111, 154, 79, 0.12);
  color: var(--sy-text);
}

:deep(.vditor-toolbar__item svg) {
  width: 16px;
  height: 16px;
  transform-origin: center;
}

:deep(.vditor-toolbar__item [data-type='emoji'] svg) {
  transform: scale(0.94);
}

:deep(.vditor-toolbar__item [data-type='headings'] svg),
:deep(.vditor-toolbar__item [data-type='bold'] svg),
:deep(.vditor-toolbar__item [data-type='italic'] svg),
:deep(.vditor-toolbar__item [data-type='strike'] svg) {
  transform: scale(0.86);
}

:deep(.vditor-toolbar__item [data-type='link'] svg),
:deep(.vditor-toolbar__item [data-type='upload'] svg) {
  transform: scale(0.94);
}

:deep(.vditor-toolbar__item [data-type='list'] svg),
:deep(.vditor-toolbar__item [data-type='ordered-list'] svg),
:deep(.vditor-toolbar__item [data-type='check'] svg),
:deep(.vditor-toolbar__item [data-type='quote'] svg),
:deep(.vditor-toolbar__item [data-type='line'] svg),
:deep(.vditor-toolbar__item [data-type='code'] svg),
:deep(.vditor-toolbar__item [data-type='inline-code'] svg),
:deep(.vditor-toolbar__item [data-type='table'] svg),
:deep(.vditor-toolbar__item [data-type='undo'] svg),
:deep(.vditor-toolbar__item [data-type='redo'] svg),
:deep(.vditor-toolbar__item [data-type='sy-fullscreen'] svg) {
  transform: scale(1.04);
}

:deep(.vditor-toolbar__item [data-type='sy-fullscreen']) {
  width: 32px;
  height: 32px;
}

:deep(.vditor-toolbar__item [data-type='sy-fullscreen'] svg) {
  width: 15px;
  height: 15px;
  transform: none;
}

:deep(.vditor-toolbar__divider) {
  align-self: center;
  height: 24px;
  margin: 0 10px;
  border-left-color: rgba(88, 100, 75, 0.16);
}

:deep(.vditor-toolbar__item--current),
:deep(.vditor-toolbar__item--current:hover),
:deep(.vditor-toolbar__item--current .vditor-tooltipped),
:deep(.vditor-toolbar__item--current .vditor-tooltipped:hover) {
  background: rgba(111, 154, 79, 0.18);
  color: #35501f;
}

:deep(.vditor-resize) {
  display: none !important;
}

:deep(.vditor-content) {
  background: linear-gradient(180deg, rgba(249, 251, 245, 0.96), rgba(244, 248, 238, 0.88));
}

:deep(.vditor-sv) {
  padding: 28px 30px 72px;
  font-family: var(--mono);
  font-size: 14px;
  line-height: 1.75;
  color: #232a21;
  background: rgba(255, 255, 255, 0.74);
  tab-size: 4;
  -moz-tab-size: 4;
}

:deep(.vditor-sv:focus) {
  background: rgba(255, 255, 255, 0.86);
}

:deep(.vditor-counter) {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 36px;
  height: 28px;
  color: var(--sy-text-faint);
  background: rgba(255, 255, 255, 0.74);
  margin: 0 12px 0 auto;
  border-radius: 8px;
}

@media (max-width: 980px) {
  .sy-editor-header-inner {
    flex-direction: column;
    align-items: stretch;
  }

  .sy-editor-actions {
    justify-content: flex-start;
  }

  .sy-upload-banner-inner {
    flex-direction: column;
    align-items: stretch;
  }

  .sy-upload-side {
    min-width: 0;
  }

  .sy-split-shell {
    grid-template-columns: 1fr;
    grid-template-rows: minmax(420px, 1fr) minmax(280px, 0.74fr);
  }

  .sy-view-switch {
    min-width: 0;
    width: 100%;
    padding: 0 14px;
  }

  .sy-preview-pane {
    padding-top: 52px;
    border-left: none;
    border-top: 1px solid rgba(56, 71, 44, 0.08);
  }

  .sy-split-shell.mode-source,
  .sy-split-shell.mode-preview {
    grid-template-rows: minmax(0, 1fr);
  }

  .sy-split-shell.mode-preview .sy-preview-pane {
    border-top: none;
  }
}

@media (max-width: 768px) {
  .sy-editor-stage {
    padding: 12px;
  }

  .sy-editor-pane,
  .sy-preview-pane {
    border-radius: 18px;
  }

  :deep(.vditor-sv) {
    padding: 20px 18px 48px;
  }

  .sy-preview-body {
    padding: 16px 14px 18px;
  }

  .sy-view-switch {
    gap: 6px;
    height: 48px;
    padding: 0 10px;
  }

  .sy-view-btn {
    padding: 0 10px;
  }
}
</style>
