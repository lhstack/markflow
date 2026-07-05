// 聊天输入区附件管理 composable。
//
// 从 AgentPanel 抽出：持有 composer 附件列表、上传状态、图片预览状态，
// 负责文件类型判断、上传、粘贴、预览/下载。模型是否支持图片/文件由调用方
// 以 computed 传入。纯文件判断函数导出为模块级函数（无状态）。

import { computed, ref, type ComputedRef, type Ref } from 'vue'
import { ElMessage } from 'element-plus'

import request from '@/utils/request'
import { useSystemStore } from '@/stores/system'
import { genId } from '@/components/agent/id'
import type { AgentAttachment } from '@/components/agent/types'

// -------- 纯文件判断（无状态，可独立复用）--------

export function fileExtension(name: string): string {
  const normalized = name.trim().toLowerCase()
  const index = normalized.lastIndexOf('.')
  return index >= 0 ? normalized.slice(index + 1) : ''
}

export const SUPPORTED_DOCUMENT_MIME_TYPES = [
  'application/pdf',
  'text/plain',
  'text/markdown',
  'text/md',
  'text/rtf',
  'text/html',
  'text/css',
  'text/csv',
  'text/xml',
  'application/x-javascript',
  'text/x-javascript',
  'application/javascript',
  'text/javascript',
  'application/x-python',
  'text/x-python',
] as const

export const SUPPORTED_DOCUMENT_EXTENSIONS = [
  'pdf', 'txt', 'md', 'markdown', 'rtf', 'html', 'htm', 'css', 'csv', 'xml',
  'js', 'mjs', 'cjs', 'py', 'json', 'toml', 'yaml', 'yml', 'ini', 'cfg',
  'conf', 'env', 'properties', 'gradle', 'gitignore', 'gitattributes',
  'npmrc', 'yarnrc', 'editorconfig', 'vmoptions', 'log', 'sh', 'bash', 'zsh',
  'fish', 'sql', 'ts', 'tsx', 'jsx', 'java', 'kt', 'kts', 'go', 'rs', 'c',
  'cc', 'cpp', 'cxx', 'h', 'hpp', 'cs', 'php', 'rb', 'swift',
] as const

export function isSupportedImageFile(file: File): boolean {
  const type = file.type.toLowerCase()
  const ext = fileExtension(file.name)
  return [
    'image/png', 'image/jpeg', 'image/gif', 'image/webp',
    'image/heic', 'image/heif', 'image/svg+xml',
  ].includes(type) || ['png', 'jpg', 'jpeg', 'gif', 'webp', 'heic', 'heif', 'svg'].includes(ext)
}

export function isSupportedDocumentFile(file: File): boolean {
  const type = file.type.toLowerCase()
  const ext = fileExtension(file.name)
  return SUPPORTED_DOCUMENT_MIME_TYPES.includes(type as typeof SUPPORTED_DOCUMENT_MIME_TYPES[number])
    || type.startsWith('text/')
    || SUPPORTED_DOCUMENT_EXTENSIONS.includes(ext as typeof SUPPORTED_DOCUMENT_EXTENSIONS[number])
}

export async function isProbablyUtf8TextFile(file: File): Promise<boolean> {
  const sampleSize = Math.min(file.size, 8192)
  if (sampleSize <= 0) return true

  const bytes = new Uint8Array(await file.slice(0, sampleSize).arrayBuffer())
  if (!bytes.length) return true
  if (bytes.includes(0)) return false

  try {
    new TextDecoder('utf-8', { fatal: true }).decode(bytes)
  } catch {
    return false
  }

  let controlCharCount = 0
  for (const byte of bytes) {
    if (byte < 0x09 || (byte > 0x0d && byte < 0x20)) {
      controlCharCount += 1
    }
  }

  return controlCharCount / bytes.length < 0.02
}

export function formatAttachmentSize(size: number): string {
  if (!Number.isFinite(size) || size <= 0) return '未知大小'
  if (size < 1024) return `${size}B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(size >= 10 * 1024 ? 0 : 1)}KB`
  return `${(size / 1024 / 1024).toFixed(size >= 10 * 1024 * 1024 ? 0 : 1)}MB`
}

export function absoluteAttachmentUrl(attachment: AgentAttachment): string {
  if (!attachment.url) return ''
  if (/^https?:\/\//i.test(attachment.url)) return attachment.url
  return `${window.location.origin}${attachment.url}`
}

export function isImageAttachment(attachment: AgentAttachment): boolean {
  return attachment.kind === 'image' || attachment.contentType?.startsWith('image/') === true
}

export function isPdfAttachment(attachment: AgentAttachment): boolean {
  return attachment.contentType === 'application/pdf' || fileExtension(attachment.name) === 'pdf'
}

export function normalizeAttachment(raw: any): AgentAttachment | null {
  if (!raw || typeof raw !== 'object') return null
  const uploadId = Number(raw.uploadId ?? raw.upload_id)
  if (!Number.isFinite(uploadId) || uploadId <= 0) return null
  const kind = raw.kind === 'image' ? 'image' : raw.kind === 'document' ? 'document' : null
  if (!kind) return null
  const name = typeof raw.name === 'string' && raw.name.trim() ? raw.name.trim() : `附件-${uploadId}`
  const url = typeof raw.url === 'string' ? raw.url : ''
  const contentType = typeof raw.contentType === 'string'
    ? raw.contentType
    : typeof raw.content_type === 'string'
      ? raw.content_type
      : null
  const size = Number(raw.size)

  return {
    localId: typeof raw.localId === 'string' && raw.localId.trim() ? raw.localId.trim() : `upload-${uploadId}`,
    uploadId,
    kind,
    name,
    url,
    contentType,
    size: Number.isFinite(size) && size > 0 ? size : 0,
  }
}

function triggerAttachmentDownload(attachment: AgentAttachment): void {
  const url = absoluteAttachmentUrl(attachment)
  if (!url) return
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = attachment.name || ''
  anchor.rel = 'noopener noreferrer'
  document.body.appendChild(anchor)
  anchor.click()
  document.body.removeChild(anchor)
}

// -------- 有状态的 composer 附件管理 --------

export interface ComposerAttachmentsDeps {
  /** 当前模型是否支持图片输入。 */
  supportsImage: ComputedRef<boolean> | Ref<boolean>
  /** 当前模型是否支持文件输入。 */
  supportsDocument: ComputedRef<boolean> | Ref<boolean>
  /** 附件功能整体是否可用。 */
  attachmentEnabled: ComputedRef<boolean> | Ref<boolean>
  /** 是否正在流式生成（生成期间禁止操作附件）。 */
  streaming: Ref<boolean>
}

export function useComposerAttachments(deps: ComposerAttachmentsDeps) {
  const systemStore = useSystemStore()

  const composerAttachments = ref<AgentAttachment[]>([])
  const attachmentInputRef = ref<HTMLInputElement | null>(null)
  const attachmentUploadingCount = ref(0)
  const previewAttachmentUrl = ref('')
  const showAttachmentPreview = ref(false)

  const attachmentUploading = computed(() => attachmentUploadingCount.value > 0)

  const composerAttachmentAccept = computed(() => {
    const parts: string[] = []
    if (deps.supportsImage.value) {
      parts.push('image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/heic', 'image/heif', 'image/svg+xml')
    }
    if (deps.supportsDocument.value) {
      parts.push('text/*', ...SUPPORTED_DOCUMENT_EXTENSIONS.map((ext) => `.${ext}`))
    }
    return parts.join(',')
  })

  function openAttachment(attachment: AgentAttachment): void {
    const url = absoluteAttachmentUrl(attachment)
    if (!url) {
      ElMessage.warning('附件链接不可用')
      return
    }
    if (isImageAttachment(attachment)) {
      previewAttachmentUrl.value = url
      showAttachmentPreview.value = true
      return
    }
    if (isPdfAttachment(attachment)) {
      window.open(url, '_blank', 'noopener,noreferrer')
      return
    }
    triggerAttachmentDownload(attachment)
  }

  function clearComposerAttachments(): void {
    composerAttachments.value = []
    if (attachmentInputRef.value) {
      attachmentInputRef.value.value = ''
    }
  }

  function removeComposerAttachment(localId: string): void {
    composerAttachments.value = composerAttachments.value.filter((attachment) => attachment.localId !== localId)
    if (attachmentInputRef.value) {
      attachmentInputRef.value.value = ''
    }
  }

  function openAttachmentPicker(): void {
    if (!deps.attachmentEnabled.value || deps.streaming.value || attachmentUploading.value) return
    attachmentInputRef.value?.click()
  }

  async function uploadComposerAttachment(file: File): Promise<void> {
    if (file.size > systemStore.uploadMaxBytes) {
      ElMessage.warning(`文件 ${file.name} 超过 ${systemStore.uploadLimitLabel} 限制`)
      return
    }

    const allowImage = deps.supportsImage.value
    const allowDocument = deps.supportsDocument.value
    const isImage = isSupportedImageFile(file)
    const knownDocument = isSupportedDocumentFile(file)
    const inferredTextDocument = !isImage && !knownDocument && allowDocument
      ? await isProbablyUtf8TextFile(file)
      : false
    const isDocument = knownDocument || inferredTextDocument

    if (isImage && !allowImage) {
      ElMessage.warning(`当前模型没有开启图片输入，无法添加 ${file.name}`)
      return
    }
    if (!isImage && !isDocument) {
      ElMessage.warning(`文件 ${file.name} 暂不支持作为聊天附件`)
      return
    }
    if (!isImage && isDocument && !allowDocument) {
      ElMessage.warning(`当前模型没有开启文件输入，无法添加 ${file.name}`)
      return
    }

    const kind = isImage ? 'doc-image' : 'doc-file'
    const attachmentKind = isImage ? 'image' : 'document'
    const formData = new FormData()
    formData.append('kind', kind)
    formData.append('file', file)

    attachmentUploadingCount.value += 1
    try {
      const data = (await request.post('/uploads', formData)) as {
        upload?: {
          id: number
          url: string
          original_name: string
          content_type?: string | null
          size?: number
        }
      }
      const upload = data?.upload
      if (!upload?.id) {
        throw new Error('上传结果缺少附件 ID')
      }
      composerAttachments.value = [
        ...composerAttachments.value,
        {
          localId: genId(),
          uploadId: Number(upload.id),
          kind: attachmentKind,
          name: upload.original_name || file.name,
          url: upload.url || '',
          contentType: upload.content_type || file.type || null,
          size: Number(upload.size) || file.size || 0,
        },
      ]
    } finally {
      attachmentUploadingCount.value = Math.max(0, attachmentUploadingCount.value - 1)
      if (attachmentInputRef.value) {
        attachmentInputRef.value.value = ''
      }
    }
  }

  async function addComposerFiles(fileList: FileList | File[]): Promise<void> {
    const files = Array.from(fileList || [])
    if (!files.length) return
    for (const file of files) {
      try {
        await uploadComposerAttachment(file)
      } catch (error: any) {
        ElMessage.error(error.response?.data?.error || error.message || `上传 ${file.name} 失败`)
      }
    }
  }

  async function handleAttachmentInputChange(event: Event): Promise<void> {
    const target = event.target as HTMLInputElement | null
    if (!target?.files?.length) return
    await addComposerFiles(target.files)
  }

  async function handleComposerPaste(event: ClipboardEvent): Promise<void> {
    if (!deps.attachmentEnabled.value || deps.streaming.value) return
    const files = Array.from(event.clipboardData?.files || [])
    if (!files.length) return
    await addComposerFiles(files)
  }

  return {
    composerAttachments,
    attachmentInputRef,
    previewAttachmentUrl,
    showAttachmentPreview,
    attachmentUploading,
    composerAttachmentAccept,
    openAttachment,
    clearComposerAttachments,
    removeComposerAttachment,
    openAttachmentPicker,
    addComposerFiles,
    handleAttachmentInputChange,
    handleComposerPaste,
  }
}
