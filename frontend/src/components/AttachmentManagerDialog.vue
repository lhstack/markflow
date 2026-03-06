<template>
  <el-dialog
    v-model="visible"
    title="附件管理"
    width="980px"
    append-to-body
    destroy-on-close
  >
    <div class="asset-shell">
      <div class="asset-toolbar">
        <el-input v-model="query" placeholder="搜索文件名" clearable />
        <el-select v-model="kindFilter" style="width: 180px">
          <el-option label="全部类型" value="all" />
          <el-option label="头像" value="avatar" />
          <el-option label="项目背景图" value="project-background" />
          <el-option label="编辑器图片" value="doc-image" />
          <el-option label="编辑器附件" value="doc-file" />
        </el-select>
        <el-button @click="loadUploads" :loading="loading">刷新</el-button>
      </div>

      <div v-if="filteredUploads.length" class="asset-list">
        <article v-for="asset in filteredUploads" :key="asset.id" class="asset-card">
          <div class="asset-preview">
            <img v-if="isImage(asset)" :src="asset.url" :alt="asset.original_name" />
            <div v-else class="asset-fallback">{{ extensionLabel(asset.original_name) }}</div>
          </div>

          <div class="asset-main">
            <div class="asset-top">
              <div class="asset-name">{{ asset.original_name }}</div>
              <div class="asset-kind">{{ kindLabel(asset.kind) }}</div>
            </div>
            <div class="asset-meta">
              <span>{{ formatSize(asset.size) }}</span>
              <span>更新于 {{ formatDate(asset.updated_at) }}</span>
            </div>
            <div class="asset-usage">
              <el-tag v-if="asset.usage.avatar" size="small" type="success">当前头像</el-tag>
              <el-tag v-if="asset.usage.project_refs" size="small" type="warning">
                {{ asset.usage.project_refs }} 个项目背景
              </el-tag>
              <el-tag v-if="asset.usage.doc_refs" size="small" type="info">
                {{ asset.usage.doc_refs }} 篇文档引用
              </el-tag>
              <el-tag v-if="!asset.usage.avatar && !asset.usage.project_refs && !asset.usage.doc_refs" size="small">
                未引用
              </el-tag>
            </div>
            <div class="asset-link">{{ asset.url }}</div>
          </div>

          <div class="asset-actions">
            <el-button text @click="copyUrl(asset.url)">复制链接</el-button>
            <el-button text @click="openReplace(asset)">替换</el-button>
            <el-button text type="danger" @click="removeAsset(asset)">删除</el-button>
          </div>
        </article>
      </div>

      <div v-else class="asset-empty">
        <span>{{ loading ? '加载中...' : '暂无附件' }}</span>
      </div>
    </div>

    <input
      ref="replaceInput"
      type="file"
      style="display:none"
      @change="handleReplaceChange"
    />
  </el-dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import request from '@/utils/request'
import { useAuthStore } from '@/stores/auth'
import { useProjectsStore } from '@/stores/projects'

type UploadKind = 'avatar' | 'project-background' | 'doc-image' | 'doc-file'

interface UploadUsage {
  avatar: boolean
  project_refs: number
  doc_refs: number
}

interface UploadItem {
  id: number
  kind: UploadKind
  original_name: string
  url: string
  content_type?: string | null
  size: number
  created_at: string
  updated_at: string
  usage: UploadUsage
}

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()

const auth = useAuthStore()
const projects = useProjectsStore()

const visible = ref(props.modelValue)
const loading = ref(false)
const uploads = ref<UploadItem[]>([])
const query = ref('')
const kindFilter = ref<'all' | UploadKind>('all')
const replaceTarget = ref<UploadItem | null>(null)
const replaceInput = ref<HTMLInputElement | null>(null)

watch(() => props.modelValue, (value) => {
  visible.value = value
  if (value) {
    void loadUploads()
  }
})

watch(visible, (value) => emit('update:modelValue', value))

const filteredUploads = computed(() => {
  const keyword = query.value.trim().toLowerCase()
  return uploads.value.filter((asset) => {
    if (kindFilter.value !== 'all' && asset.kind !== kindFilter.value) return false
    if (!keyword) return true
    return asset.original_name.toLowerCase().includes(keyword)
  })
})

function kindLabel(kind: UploadKind) {
  if (kind === 'avatar') return '头像'
  if (kind === 'project-background') return '项目背景图'
  if (kind === 'doc-image') return '编辑器图片'
  return '编辑器附件'
}

function extensionLabel(name: string) {
  const last = name.split('.').pop() || 'FILE'
  return last.slice(0, 6).toUpperCase()
}

function isImage(asset: UploadItem) {
  return asset.content_type?.startsWith('image/')
    || ['avatar', 'project-background', 'doc-image'].includes(asset.kind)
}

function formatSize(size: number) {
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  return `${(size / 1024 / 1024).toFixed(1)} MB`
}

function formatDate(value: string) {
  const date = new Date(value.endsWith('Z') ? value : `${value}Z`)
  const yyyy = date.getFullYear()
  const mm = `${date.getMonth() + 1}`.padStart(2, '0')
  const dd = `${date.getDate()}`.padStart(2, '0')
  const hh = `${date.getHours()}`.padStart(2, '0')
  const mi = `${date.getMinutes()}`.padStart(2, '0')
  return `${yyyy}/${mm}/${dd} ${hh}:${mi}`
}

async function loadUploads() {
  loading.value = true
  try {
    const data = (await request.get('/uploads')) as { uploads?: UploadItem[] }
    uploads.value = data.uploads || []
  } finally {
    loading.value = false
  }
}

async function refreshUserAndProjects() {
  try {
    const userData = (await request.get('/auth/me')) as { user: Record<string, unknown> }
    auth.updateUser(userData.user)
  } catch {
    // ignore
  }
  try {
    await projects.fetchProjects()
  } catch {
    // ignore
  }
}

async function copyUrl(url: string) {
  try {
    await navigator.clipboard.writeText(`${window.location.origin}${url}`)
    ElMessage.success('链接已复制')
  } catch {
    ElMessage.error('复制失败')
  }
}

function openReplace(asset: UploadItem) {
  replaceTarget.value = asset
  if (replaceInput.value) {
    replaceInput.value.accept = isImage(asset) ? 'image/*' : '*/*'
    replaceInput.value.value = ''
    replaceInput.value.click()
  }
}

async function handleReplaceChange(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0]
  if (!file || !replaceTarget.value) return

  const formData = new FormData()
  formData.append('kind', replaceTarget.value.kind)
  formData.append('file', file)

  try {
    await request.put(`/uploads/${replaceTarget.value.id}`, formData)
    await loadUploads()
    await refreshUserAndProjects()
    ElMessage.success('附件已替换')
  } catch (err: any) {
    ElMessage.error(err.response?.data?.error || '替换失败')
  } finally {
    replaceTarget.value = null
    if (replaceInput.value) replaceInput.value.value = ''
  }
}

async function removeAsset(asset: UploadItem) {
  try {
    await ElMessageBox.confirm(
      `确认删除附件「${asset.original_name}」？已插入文档的链接会失效。`,
      '删除附件',
      {
        type: 'warning',
        confirmButtonText: '删除',
        cancelButtonText: '取消',
        confirmButtonClass: 'el-button--danger',
      }
    )

    await request.delete(`/uploads/${asset.id}`)
    await loadUploads()
    await refreshUserAndProjects()
    ElMessage.success('附件已删除')
  } catch (err: any) {
    if (err !== 'cancel' && err !== 'close') {
      ElMessage.error(err.response?.data?.error || '删除失败')
    }
  }
}
</script>

<style scoped>
.asset-shell {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-height: 420px;
}

.asset-toolbar {
  display: flex;
  gap: 10px;
}

.asset-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-height: 62vh;
  overflow: auto;
}

.asset-card {
  display: grid;
  grid-template-columns: 88px minmax(0, 1fr) auto;
  gap: 14px;
  padding: 14px;
  border: 1px solid var(--border);
  border-radius: 14px;
  background: var(--bg2);
}

.asset-preview {
  width: 88px;
  height: 88px;
  border-radius: 12px;
  overflow: hidden;
  background: var(--bg3);
  border: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: center;
}

.asset-preview img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.asset-fallback {
  font-size: 16px;
  font-weight: 700;
  color: var(--text2);
}

.asset-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.asset-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.asset-name {
  font-size: 15px;
  font-weight: 700;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.asset-kind {
  font-size: 12px;
  color: var(--text3);
  white-space: nowrap;
}

.asset-meta {
  display: flex;
  gap: 10px;
  color: var(--text3);
  font-size: 12px;
  flex-wrap: wrap;
}

.asset-usage {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.asset-link {
  font-size: 12px;
  color: var(--blue);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.asset-actions {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  justify-content: center;
}

.asset-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px dashed var(--border);
  border-radius: 14px;
  color: var(--text3);
}

@media (max-width: 900px) {
  .asset-toolbar {
    flex-direction: column;
  }

  .asset-card {
    grid-template-columns: 72px 1fr;
  }

  .asset-actions {
    grid-column: 1 / -1;
    flex-direction: row;
    justify-content: flex-start;
  }

  .asset-preview {
    width: 72px;
    height: 72px;
  }
}
</style>
