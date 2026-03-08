<template>
  <section class="project-overview">
    <header class="overview-header">
      <div>
        <h2>项目概览</h2>
        <p>选择一个项目进入文档树</p>
      </div>
      <el-button type="primary" @click="openCreate">新建项目</el-button>
    </header>

    <div ref="gridContainer" class="overview-grid-wrap">
      <div v-if="pagedProjects.length" class="overview-grid">
        <article
          v-for="project in pagedProjects"
          :key="project.id"
          class="project-card"
          :class="{ 'is-active': project.id === props.activeProjectId }"
          :style="cardStyle(project.background_image)"
          @click="emit('select', project.id)"
        >
          <div class="card-overlay"></div>
          <div class="card-actions" @click.stop>
            <button class="ghost-btn" @click="openEdit(project)">编辑</button>
            <button class="ghost-btn ghost-btn-danger" @click="confirmDelete(project)">删除</button>
          </div>

          <div class="card-content">
            <h3 class="project-name">{{ project.name }}</h3>
            <div class="project-desc">{{ project.description || '暂无描述' }}</div>
          </div>
        </article>
      </div>

      <div v-else class="overview-empty">
        <span>还没有项目，先创建一个项目吧</span>
      </div>
    </div>

    <footer v-if="pageCount > 1" class="overview-footer">
      <el-pagination
        background
        small
        layout="prev, pager, next"
        :page-size="cardsPerPage"
        :total="props.projects.length"
        :current-page="currentPage"
        @current-change="onPageChange"
      />
    </footer>

    <el-dialog
      v-model="showDialog"
      :title="mode === 'create' ? '新建项目' : '编辑项目'"
      width="520px"
      append-to-body
      destroy-on-close
    >
      <div class="dialog-form">
        <el-input v-model="form.name" maxlength="64" placeholder="项目名称" />
        <el-input
          v-model="form.description"
          type="textarea"
          :rows="4"
          maxlength="2000"
          show-word-limit
          placeholder="项目描述"
        />
        <div class="bg-upload">
          <input
            ref="bgFileInput"
            type="file"
            accept="image/*"
            style="display:none"
            @change="handleBackgroundFileChange"
          />
          <div class="bg-preview" :class="{ empty: !form.background_image }" @click="triggerBackgroundUpload">
            <img v-if="form.background_image" :src="form.background_image" alt="背景图预览" />
            <div v-else class="bg-placeholder">点击上传背景图</div>
          </div>
          <div class="bg-upload-actions">
            <el-button :loading="uploadingBackground" @click="triggerBackgroundUpload">上传背景图</el-button>
            <el-button :disabled="!form.background_image || uploadingBackground" @click="clearBackgroundImage">移除</el-button>
            <span class="bg-file-name">{{ backgroundFileName || (form.background_image ? '已设置背景图' : '未选择文件') }}</span>
          </div>
          <el-progress
            v-if="backgroundUploadTask && backgroundUploadTask.status === 'uploading'"
            :percentage="backgroundUploadTask.progress"
            :stroke-width="8"
          />
          <p v-if="backgroundUploadTask && backgroundUploadTask.status === 'error'" class="bg-upload-error">
            {{ backgroundUploadTask.error || '上传失败' }}
          </p>
        </div>
      </div>
      <template #footer>
        <el-button @click="showDialog = false">取消</el-button>
        <el-button type="primary" :disabled="uploadingBackground" @click="submitProject">保存</el-button>
      </template>
    </el-dialog>
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import type { ProjectItem } from '@/stores/projects'
import { createManagedUploadTask, removeManagedUpload, type ManagedUploadTask } from '@/utils/managedUploads'
import { uploadImage } from '@/utils/uploads'
import { useSystemStore } from '@/stores/system'

const props = defineProps<{
  projects: ProjectItem[]
  activeProjectId?: number | null
}>()

const emit = defineEmits<{
  select: [projectId: number]
  create: [payload: { name: string; description: string; background_image: string }]
  update: [projectId: number, payload: { name: string; description: string; background_image: string }]
  delete: [projectId: number]
}>()
const system = useSystemStore()

const gridContainer = ref<HTMLElement | null>(null)
const cardsPerPage = ref(8)
const currentPage = ref(1)
const showDialog = ref(false)
const mode = ref<'create' | 'edit'>('create')
const editingProjectId = ref<number | null>(null)
const bgFileInput = ref<HTMLInputElement | null>(null)
const backgroundFileName = ref('')
const form = ref({ name: '', description: '', background_image: '' })
const uploadingBackground = ref(false)
const backgroundUploadTask = ref<ManagedUploadTask | null>(null)

const pageCount = computed(() => Math.max(1, Math.ceil(props.projects.length / cardsPerPage.value)))

const pagedProjects = computed(() => {
  const start = (currentPage.value - 1) * cardsPerPage.value
  return props.projects.slice(start, start + cardsPerPage.value)
})

watch(
  () => [props.projects.length, cardsPerPage.value],
  () => {
    if (currentPage.value > pageCount.value) {
      currentPage.value = pageCount.value
    }
  }
)

function onPageChange(page: number) {
  currentPage.value = page
}

function cardStyle(backgroundImage?: string | null) {
  if (backgroundImage && backgroundImage.trim()) {
    return { backgroundImage: `url(${backgroundImage})` }
  }
  return {
    backgroundImage:
      'linear-gradient(130deg, #eef3ff 0%, #f5faff 55%, #eefbf2 100%)',
  }
}

function openCreate() {
  mode.value = 'create'
  editingProjectId.value = null
  form.value = { name: '', description: '', background_image: '' }
  backgroundFileName.value = ''
  showDialog.value = true
}

function openEdit(project: ProjectItem) {
  mode.value = 'edit'
  editingProjectId.value = project.id
  form.value = {
    name: project.name,
    description: project.description || '',
    background_image: project.background_image || '',
  }
  backgroundFileName.value = project.background_image ? '已设置背景图' : ''
  showDialog.value = true
}

function triggerBackgroundUpload() {
  if (uploadingBackground.value) return
  bgFileInput.value?.click()
}

async function handleBackgroundFileChange(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0]
  if (!file) return
  if (!file.type.startsWith('image/')) {
    ElMessage.warning('请选择图片文件')
    return
  }
  if (file.size > system.uploadMaxBytes) {
    ElMessage.warning(`图片大小不能超过 ${system.uploadLimitLabel}`)
    return
  }

  uploadingBackground.value = true
  if (backgroundUploadTask.value) {
    removeManagedUpload(backgroundUploadTask.value.id)
  }
  backgroundUploadTask.value = createManagedUploadTask('project-background', file)
  try {
    form.value.background_image = await uploadImage(file, 'project-background', { task: backgroundUploadTask.value })
    backgroundFileName.value = file.name
    ElMessage.success('背景图上传成功')
  } catch (err: any) {
    ElMessage.error(err.response?.data?.error || '背景图上传失败')
  } finally {
    uploadingBackground.value = false
    if (bgFileInput.value) {
      bgFileInput.value.value = ''
    }
  }
}

function clearBackgroundImage() {
  form.value.background_image = ''
  backgroundFileName.value = ''
  if (bgFileInput.value) bgFileInput.value.value = ''
}

async function confirmDelete(project: ProjectItem) {
  try {
    await ElMessageBox.confirm(`确认删除项目「${project.name}」？`, '删除确认', {
      type: 'warning',
      confirmButtonText: '删除',
      cancelButtonText: '取消',
      confirmButtonClass: 'el-button--danger',
    })
    emit('delete', project.id)
  } catch {
    // ignore
  }
}

function submitProject() {
  const name = form.value.name.trim()
  if (!name) {
    ElMessage.warning('项目名称不能为空')
    return
  }

  const normalized = name.toLowerCase()
  const hasDuplicate = props.projects.some((project) => {
    if (mode.value === 'edit' && project.id === editingProjectId.value) return false
    return project.name.trim().toLowerCase() === normalized
  })
  if (hasDuplicate) {
    ElMessage.warning('项目名称已存在，请更换一个名称')
    return
  }

  const payload = {
    name,
    description: form.value.description,
    background_image: form.value.background_image,
  }

  if (mode.value === 'create') {
    emit('create', payload)
  } else if (editingProjectId.value) {
    emit('update', editingProjectId.value, payload)
  }

  showDialog.value = false
}

function recalcCardsPerPage() {
  const el = gridContainer.value
  if (!el) return

  const width = el.clientWidth
  const height = el.clientHeight
  if (!width || !height) return

  const columns = Math.max(1, Math.floor((width + 16) / (300 + 16)))
  const rows = Math.max(1, Math.floor((height + 16) / (200 + 16)))
  cardsPerPage.value = Math.max(1, columns * rows)
}

let resizeObserver: ResizeObserver | null = null

onMounted(() => {
  nextTick(recalcCardsPerPage)
  window.addEventListener('resize', recalcCardsPerPage)

  if (gridContainer.value) {
    resizeObserver = new ResizeObserver(recalcCardsPerPage)
    resizeObserver.observe(gridContainer.value)
  }
})

onUnmounted(() => {
  window.removeEventListener('resize', recalcCardsPerPage)
  resizeObserver?.disconnect()
  resizeObserver = null
})
</script>

<style scoped>
.project-overview {
  height: 100%;
  display: flex;
  flex-direction: column;
  padding: 18px;
  gap: 12px;
  overflow: hidden;
  background: #f3f5f9;
  color: #182033;
}

.overview-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
}

.overview-header h2 {
  margin: 0;
  font-size: 20px;
  color: #1f2b45;
}

.overview-header p {
  margin: 4px 0 0;
  color: #6c7891;
  font-size: 13px;
}

.overview-grid-wrap {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.overview-grid {
  height: 100%;
  display: grid;
  grid-template-columns: repeat(auto-fill, 300px);
  grid-auto-rows: 200px;
  justify-content: start;
  align-content: start;
  align-items: start;
  gap: 16px;
  overflow: auto;
  padding: 2px;
}

.project-card {
  position: relative;
  width: 300px;
  height: 200px;
  min-height: 200px;
  border-radius: 16px;
  overflow: hidden;
  cursor: pointer;
  border: 1px solid #d5dceb;
  background-position: center;
  background-size: cover;
  background-repeat: no-repeat;
  background-color: #f8fbff;
  transition: transform 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease, background-color 0.18s ease;
}

.project-card:hover {
  transform: translateY(-2px);
  border-color: #b8c9eb;
  box-shadow: 0 12px 26px rgba(41, 72, 130, 0.14);
}

.project-card.is-active {
  border-color: #6ea2ff;
  box-shadow: 0 0 0 1px rgba(110, 162, 255, 0.3), 0 12px 26px rgba(41, 72, 130, 0.14);
}

.card-overlay {
  position: absolute;
  inset: 0;
  background:
    linear-gradient(
      180deg,
      rgba(15, 30, 54, 0.26) 0%,
      rgba(28, 52, 87, 0.14) 30%,
      rgba(248, 251, 255, 0.86) 66%,
      rgba(250, 252, 255, 0.97) 100%
    );
  backdrop-filter: saturate(1.08);
}

.card-actions {
  position: absolute;
  top: 10px;
  right: 10px;
  display: flex;
  gap: 8px;
  z-index: 2;
}

.ghost-btn {
  border: 1px solid #d5dceb;
  background: rgba(255, 255, 255, 0.95);
  color: #2f3c56;
  font-size: 12px;
  border-radius: 999px;
  padding: 4px 10px;
  cursor: pointer;
}

.ghost-btn:hover {
  background: #f2f6ff;
}

.ghost-btn-danger {
  border-color: #f2bcc2;
  color: #bb4456;
}

.card-content {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 1;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  background: linear-gradient(
    180deg,
    rgba(251, 253, 255, 0) 0%,
    rgba(251, 253, 255, 0.78) 38%,
    rgba(251, 253, 255, 0.97) 100%
  );
}

.project-name {
  margin: 0;
  font-size: 18px;
  color: #1f2b45;
  text-shadow: 0 1px 0 rgba(255, 255, 255, 0.72);
}

.project-desc {
  max-height: 88px;
  overflow: auto;
  margin: 0;
  font-size: 13px;
  line-height: 1.48;
  color: #53617b;
  padding: 4px 8px 4px 0;
  text-shadow: 0 1px 0 rgba(255, 255, 255, 0.62);
}

.overview-empty {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  border: 1px dashed #d5dceb;
  border-radius: 16px;
  color: #6c7891;
  background: #ffffff;
}

.overview-footer {
  display: flex;
  justify-content: center;
}

.dialog-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.bg-upload {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.bg-preview {
  width: 100%;
  height: 136px;
  border-radius: 12px;
  overflow: hidden;
  border: 1px dashed #c9d4e9;
  cursor: pointer;
  background: #eef3ff;
}

.bg-preview img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.bg-preview.empty:hover {
  background: #e7efff;
}

.bg-placeholder {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #6c7891;
  font-size: 13px;
}

.bg-upload-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.bg-file-name {
  flex: 1;
  min-width: 0;
  margin-left: 6px;
  color: #6c7891;
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bg-upload-error {
  font-size: 12px;
  color: #dd4d4d;
}

@media (max-width: 768px) {
  .project-overview {
    padding: 12px;
  }

  .overview-grid {
    grid-template-columns: 1fr;
    grid-auto-rows: 200px;
  }

  .project-card {
    width: 100%;
  }
}
</style>
