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
        <button class="secondary-btn" :disabled="uploadingAsset" @click="triggerAttachmentUpload">
          {{ uploadingAsset ? '上传中...' : '附件' }}
        </button>
        <button class="secondary-btn" @click="emit('share', node)">分享</button>
        <button class="save-btn" :class="{ dirty: isDirty }" :disabled="saving" @click="save">
          <span>{{ saving ? '保存中...' : '保存' }}</span>
          <kbd>⌘S</kbd>
        </button>
      </div>
    </header>

    <div
      v-if="assetUploadTask"
      class="upload-banner"
      :class="{
        uploading: assetUploadTask.status === 'uploading',
        error: assetUploadTask.status === 'error',
      }"
    >
      <div class="upload-banner-meta">
        <span class="upload-banner-title">{{ uploadBannerTitle }}</span>
        <span class="upload-banner-file">{{ assetUploadTask.fileName }}</span>
      </div>
      <div class="upload-banner-body">
        <div v-if="assetUploadTask.status === 'uploading'" class="upload-banner-progress">
          <span>{{ assetUploadTask.progress }}%</span>
          <el-progress :percentage="assetUploadTask.progress" :stroke-width="7" :show-text="false" />
        </div>
        <span v-else-if="assetUploadTask.status === 'error'" class="upload-banner-error">
          {{ assetUploadTask.error || '上传失败' }}
        </span>
        <span v-else class="upload-banner-success">附件已插入文档</span>
        <button
          v-if="assetUploadTask.status !== 'uploading'"
          class="upload-banner-close"
          @click="clearAssetUploadTask"
        >
          关闭
        </button>
      </div>
    </div>

    <div class="editor-host">
      <VMdEditor
        ref="editorRef"
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
        @upload-image="handleUploadImage"
      />
    </div>

    <input
      ref="attachmentInput"
      type="file"
      style="display:none"
      @change="handleAttachmentChange"
    />
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
import { createManagedUploadTask, removeManagedUpload, type ManagedUploadTask } from '@/utils/managedUploads'
import { uploadFile, uploadImage } from '@/utils/uploads'

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
const draft = ref(props.node.content || '')
const viewMode = ref<'edit' | 'split' | 'preview'>('split')
const editorRef = ref<any>(null)
const attachmentInput = ref<HTMLInputElement | null>(null)
const uploadingAsset = ref(false)
const assetUploadTask = ref<ManagedUploadTask | null>(null)
let originalContent = props.node.content || ''

const editorMode = computed(() => (viewMode.value === 'split' ? 'editable' : viewMode.value))
const toneClass = computed(() => 'tone-light')
const wordCount = computed(() => {
  const text = draft.value
  const cjk = (text.match(/[\u4e00-\u9fff\u3040-\u30ff\uac00-\ud7ff]/g) || []).length
  const words = (text.match(/[a-zA-Z]+/g) || []).length
  return cjk + words
})

const leftToolbar = 'undo redo clear | h bold italic strikethrough quote | ul ol table hr | link image code'
const rightToolbar = 'fullscreen'
const disabledMenus: string[] = []
const codemirrorConfig = {
  indentUnit: 2,
  tabSize: 2,
  indentWithTabs: false,
  lineWrapping: true,
  scrollbarStyle: 'native',
}
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

function beginAssetUpload(kind: 'doc-image' | 'doc-file', file: File) {
  if (assetUploadTask.value) {
    removeManagedUpload(assetUploadTask.value.id)
  }
  assetUploadTask.value = createManagedUploadTask(kind, file)
  uploadingAsset.value = true
  return assetUploadTask.value
}

function clearFileInput(input: HTMLInputElement | null) {
  if (input) {
    input.value = ''
  }
}

function clearAssetUploadTask() {
  if (assetUploadTask.value) {
    removeManagedUpload(assetUploadTask.value.id)
    assetUploadTask.value = null
  }
}

function triggerAttachmentUpload() {
  if (uploadingAsset.value) return
  clearFileInput(attachmentInput.value)
  attachmentInput.value?.click()
}

function insertAttachmentLink(fileName: string, url: string) {
  const editor = editorRef.value
  if (editor?.insert) {
    editor.insert((selected: string) => ({
      text: selected ? `[${selected}](${url})` : `[${fileName}](${url})`,
      selected: null,
    }))
    return
  }

  const prefix = draft.value && !draft.value.endsWith('\n') ? '\n' : ''
  draft.value = `${draft.value}${prefix}[${fileName}](${url})`
}

async function handleUploadImage(_event: Event, insertImage: (config: { url: string; desc: string }) => void, files: File[]) {
  const file = files[0]
  if (!file || uploadingAsset.value) return
  if (!file.type.startsWith('image/')) {
    ElMessage.warning('请选择图片文件')
    return
  }
  if (file.size > 10 * 1024 * 1024) {
    ElMessage.warning('图片大小不能超过 10MB')
    return
  }

  const task = beginAssetUpload('doc-image', file)
  try {
    const url = await uploadImage(file, 'doc-image', { task })
    insertImage({ url, desc: file.name })
    ElMessage.success('图片已插入文档')
  } catch (err: any) {
    ElMessage.error(err.response?.data?.error || '图片上传失败')
  } finally {
    uploadingAsset.value = false
  }
}

async function handleAttachmentChange(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0]
  if (!file) return
  if (file.size > 20 * 1024 * 1024) {
    ElMessage.warning('附件大小不能超过 20MB')
    clearFileInput(attachmentInput.value)
    return
  }

  const task = beginAssetUpload('doc-file', file)
  try {
    const url = await uploadFile(file, 'doc-file', { task })
    insertAttachmentLink(file.name, url)
    ElMessage.success('附件已插入文档')
  } catch (err: any) {
    ElMessage.error(err.response?.data?.error || '附件上传失败')
  } finally {
    uploadingAsset.value = false
    clearFileInput(attachmentInput.value)
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
  window.addEventListener('beforeunload', handleBeforeUnload)
  window.addEventListener('keydown', handleSaveHotkey)
})

onUnmounted(() => {
  window.removeEventListener('beforeunload', handleBeforeUnload)
  window.removeEventListener('keydown', handleSaveHotkey)
  clearAssetUploadTask()
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

.secondary-btn:disabled {
  cursor: not-allowed;
  opacity: 0.7;
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

.upload-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--mde-border);
  background: color-mix(in srgb, var(--mde-accent) 7%, var(--mde-panel));
}

.upload-banner.error {
  background: rgba(214, 76, 76, 0.08);
}

.upload-banner-meta {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 10px;
}

.upload-banner-title {
  font-size: 12px;
  font-weight: 700;
  color: var(--mde-text);
}

.upload-banner-file {
  min-width: 0;
  font-size: 12px;
  color: var(--mde-text-soft);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.upload-banner-body {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 260px;
}

.upload-banner-progress {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  font-size: 12px;
  color: var(--mde-text-soft);
}

.upload-banner-success {
  font-size: 12px;
  color: #1e9352;
}

.upload-banner-error {
  font-size: 12px;
  color: #d64c4c;
}

.upload-banner-close {
  border: 0;
  background: transparent;
  color: var(--mde-text-soft);
  font-size: 12px;
  cursor: pointer;
}

.upload-banner-close:hover {
  color: var(--mde-text);
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

  .upload-banner {
    flex-direction: column;
    align-items: stretch;
  }

  .upload-banner-body {
    min-width: 0;
  }
}

@media (max-width: 768px) {
  .doc-meta {
    display: none;
  }
}
</style>
