<template>
  <div class="app-layout">
    <!-- Header -->
    <header class="app-header">
      <div class="header-left">
        <button
          v-if="showSidebar"
          class="sidebar-toggle"
          @click="sidebarOpen = !sidebarOpen"
          :title="sidebarOpen ? '收起侧边栏' : '展开侧边栏'"
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M1 2.75A.75.75 0 0 1 1.75 2h12.5a.75.75 0 0 1 0 1.5H1.75A.75.75 0 0 1 1 2.75Zm0 5A.75.75 0 0 1 1.75 7h12.5a.75.75 0 0 1 0 1.5H1.75A.75.75 0 0 1 1 7.75ZM1.75 12h12.5a.75.75 0 0 1 0 1.5H1.75a.75.75 0 0 1 0-1.5Z"/>
          </svg>
        </button>
        <div class="brand">
          <div class="brand-logo" aria-hidden="true">
            <svg viewBox="0 0 48 48" fill="none">
              <defs>
                <linearGradient id="mf-g1" x1="4" y1="44" x2="44" y2="4" gradientUnits="userSpaceOnUse">
                  <stop stop-color="#22C55E"/>
                  <stop offset="0.55" stop-color="#14B8A6"/>
                  <stop offset="1" stop-color="#3B82F6"/>
                </linearGradient>
              </defs>
              <rect x="2.5" y="2.5" width="43" height="43" rx="13" fill="url(#mf-g1)" />
              <rect x="2.5" y="2.5" width="43" height="43" rx="13" stroke="rgba(255,255,255,0.38)" />
              <path d="M11 32V16L20 25L28 17V32" stroke="white" stroke-width="2.9" stroke-linecap="round" stroke-linejoin="round"/>
              <path d="M30.5 18.5H37.5M30.5 24H35.5M30.5 29.5H37.5" stroke="white" stroke-width="2.2" stroke-linecap="round"/>
            </svg>
          </div>
          <div class="brand-text">
            <span class="brand-name">MarkFlow</span>
            <span class="brand-sub">Knowledge Mesh</span>
          </div>
        </div>
        <div class="breadcrumb" v-if="showProjectOverview">
          <span class="bc-sep">/</span>
          <span class="bc-item bc-current">项目概览</span>
        </div>
        <div class="breadcrumb" v-else-if="projects.currentProject">
          <span class="bc-sep">/</span>
          <span class="bc-item clickable" @click="backToOverview">项目概览</span>
          <span class="bc-sep">/</span>
          <span class="bc-item clickable" @click="docs.currentNode = null">{{ projects.currentProject.name }}</span>
          <template v-if="docs.currentNode">
            <span class="bc-sep">/</span>
            <span class="bc-item bc-current">{{ docs.currentNode.name }}</span>
          </template>
        </div>
      </div>

      <div class="header-right">
        <el-dropdown trigger="click" @command="handleUserCmd" placement="bottom-end">
          <div class="user-menu">
            <div class="user-avatar">
              <img v-if="auth.user?.avatar" :src="auth.user.avatar" />
              <span v-else>{{ auth.user?.username?.[0]?.toUpperCase() }}</span>
            </div>
            <span class="user-name">{{ auth.user?.username }}</span>
            <svg width="12" height="12" viewBox="0 0 16 16" fill="var(--text3)">
              <path d="M4.427 7.427l3.396 3.396a.25.25 0 0 0 .354 0l3.396-3.396A.25.25 0 0 0 11.396 7H4.604a.25.25 0 0 0-.177.427Z"/>
            </svg>
          </div>
          <template #dropdown>
            <el-dropdown-menu>
              <el-dropdown-item command="profile">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" style="flex-shrink:0"><path d="M10.561 8.073a6.005 6.005 0 0 1 3.432 5.142.75.75 0 1 1-1.498.07 4.5 4.5 0 0 0-8.99 0 .75.75 0 0 1-1.498-.07 6.004 6.004 0 0 1 3.431-5.142 3.999 3.999 0 1 1 5.123 0ZM10.5 5a2.5 2.5 0 1 0-5 0 2.5 2.5 0 0 0 5 0Z"/></svg>
                个人信息
              </el-dropdown-item>
              <el-dropdown-item command="password">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" style="flex-shrink:0"><path d="M6.5 7.5V6a3.5 3.5 0 0 1 7 0v1.5h.25a.75.75 0 0 1 .75.75v5.5a.75.75 0 0 1-.75.75h-7.5a.75.75 0 0 1-.75-.75v-5.5a.75.75 0 0 1 .75-.75Zm1.5-1.5v1.5h4V6a2 2 0 0 0-4 0Z"/></svg>
                修改密码
              </el-dropdown-item>
              <el-dropdown-item command="attachments">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" style="flex-shrink:0"><path d="M4.75 1A2.75 2.75 0 0 0 2 3.75v7.5A3.75 3.75 0 0 0 5.75 15h4.5A3.75 3.75 0 0 0 14 11.25v-6.5a2.75 2.75 0 0 0-5.5 0v5.75a1.25 1.25 0 0 0 2.5 0V5.75a.75.75 0 0 1 1.5 0v4.75a2.75 2.75 0 0 1-5.5 0V4.75a4.25 4.25 0 0 1 8.5 0v6.5A5.25 5.25 0 0 1 10.25 16h-4.5A5.25 5.25 0 0 1 .5 10.75v-7A2.75 2.75 0 0 1 3.25 1h1.5a.75.75 0 0 1 0 1.5Z"/></svg>
                附件管理
              </el-dropdown-item>
              <el-dropdown-item v-if="auth.user?.is_super_admin" command="system">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" style="flex-shrink:0"><path d="M7.53 1.106a.75.75 0 0 1 .94 0l1.02.82a1.5 1.5 0 0 0 1.222.294l1.283-.267a.75.75 0 0 1 .814.472l.485 1.22a1.5 1.5 0 0 0 .9.875l1.232.417a.75.75 0 0 1 .49.803l-.15 1.302a1.5 1.5 0 0 0 .403 1.19l.887.965a.75.75 0 0 1 0 1.012l-.887.965a1.5 1.5 0 0 0-.404 1.19l.151 1.302a.75.75 0 0 1-.49.803l-1.232.417a1.5 1.5 0 0 0-.9.875l-.485 1.22a.75.75 0 0 1-.814.472l-1.283-.267a1.5 1.5 0 0 0-1.222.294l-1.02.82a.75.75 0 0 1-.94 0l-1.02-.82a1.5 1.5 0 0 0-1.222-.294l-1.283.267a.75.75 0 0 1-.814-.472l-.485-1.22a1.5 1.5 0 0 0-.9-.875l-1.232-.417a.75.75 0 0 1-.49-.803l.15-1.302a1.5 1.5 0 0 0-.403-1.19l-.887-.965a.75.75 0 0 1 0-1.012l.887-.965a1.5 1.5 0 0 0 .404-1.19l-.151-1.302a.75.75 0 0 1 .49-.803l1.232-.417a1.5 1.5 0 0 0 .9-.875l.485-1.22a.75.75 0 0 1 .814-.472l1.283.267a1.5 1.5 0 0 0 1.222-.294Zm.47 4.144a2.75 2.75 0 1 0 0 5.5 2.75 2.75 0 0 0 0-5.5Z"/></svg>
                系统配置
              </el-dropdown-item>
              <el-dropdown-item v-if="auth.user?.is_super_admin" command="users">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" style="flex-shrink:0"><path d="M5.5 5a2.5 2.5 0 1 1 5 0 2.5 2.5 0 0 1-5 0ZM8 8.75A5.75 5.75 0 0 0 2.25 14.5a.75.75 0 0 0 1.5 0 4.25 4.25 0 0 1 8.5 0 .75.75 0 0 0 1.5 0A5.75 5.75 0 0 0 8 8.75Z"/></svg>
                用户管理
              </el-dropdown-item>
              <el-dropdown-item command="logout" divided>
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" style="flex-shrink:0"><path d="M2 2.75C2 1.784 2.784 1 3.75 1h4.5a.75.75 0 0 1 0 1.5h-4.5a.25.25 0 0 0-.25.25v10.5c0 .138.112.25.25.25h4.5a.75.75 0 0 1 0 1.5h-4.5A1.75 1.75 0 0 1 2 13.25Zm10.44 4.5-1.97-1.97a.749.749 0 0 1 .326-1.275.749.749 0 0 1 .734.215l3.25 3.25a.75.75 0 0 1 0 1.06l-3.25 3.25a.749.749 0 0 1-1.275-.326.749.749 0 0 1 .215-.734l1.97-1.97H6.75a.75.75 0 0 1 0-1.5Z"/></svg>
                退出登录
              </el-dropdown-item>
            </el-dropdown-menu>
          </template>
        </el-dropdown>
      </div>
    </header>

    <!-- Body -->
    <div class="app-body">
      <!-- Overlay for mobile -->
      <div v-if="mobileOpen && showSidebar" class="sidebar-mask" @click="mobileOpen = false" />

      <!-- Sidebar -->
      <aside v-if="showSidebar" class="sidebar" :class="{ 'sidebar-hidden': !sidebarOpen, 'sidebar-mobile': mobileOpen }">
        <DocTree
          :project-id="projects.currentProject?.id"
          :project-name="projects.currentProject?.name"
          @share="openShare"
        />
      </aside>

      <!-- Main -->
      <main class="main-area">
        <transition name="slide-up" mode="out-in">
          <ProjectOverview
            v-if="showProjectOverview"
            key="projects"
            :projects="projects.projects"
            :active-project-id="projects.currentProjectId"
            @select="enterProject"
            @create="createProject"
            @update="updateProject"
            @delete="deleteProject"
          />
          <WelcomeScreen v-else-if="!docs.currentNode" key="welcome" />
          <div v-else-if="docs.currentNode.node_type === 'dir'" class="main-scroll" key="dir">
            <DirStats :node="docs.currentNode" :stats="docs.currentStats" @share="openShare" @select="n => docs.fetchNode(n.id)" />
          </div>
          <MarkdownEditor v-else :node="docs.currentNode" key="editor" @share="openShare" />
        </transition>
      </main>
    </div>

    <!-- Dialogs -->
    <ProfileDialog v-model="showProfile" />
    <AttachmentManagerDialog v-model="showAttachments" />
    <SystemSettingsDialog v-model="showSystemSettings" />
    <UserManagementDialog v-model="showUserManagement" />
    <ShareDialog v-if="shareTarget" v-model="showShare" :node="shareTarget" />

    <!-- Password dialog -->
    <el-dialog v-model="showPwChange" title="修改密码" width="380px" append-to-body destroy-on-close>
      <div style="display:flex;flex-direction:column;gap:12px">
        <el-input v-model="pwForm.old" type="password" show-password placeholder="当前密码" />
        <el-input v-model="pwForm.new" type="password" show-password placeholder="新密码（至少6位）" />
        <el-input v-model="pwForm.confirm" type="password" show-password placeholder="确认新密码" @keydown.enter="changePw" />
      </div>
      <template #footer>
        <el-button @click="showPwChange = false">取消</el-button>
        <el-button type="primary" :loading="changingPw" @click="changePw">确认修改</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import { useAuthStore } from '@/stores/auth'
import { useDocsStore, type DocNode } from '@/stores/docs'
import { useProjectsStore } from '@/stores/projects'
import DocTree from '@/components/DocTree.vue'
import MarkdownEditor from '@/components/MarkdownEditor.vue'
import DirStats from '@/components/DirStats.vue'
import ShareDialog from '@/components/ShareDialog.vue'
import ProfileDialog from '@/components/ProfileDialog.vue'
import AttachmentManagerDialog from '@/components/AttachmentManagerDialog.vue'
import SystemSettingsDialog from '@/components/SystemSettingsDialog.vue'
import UserManagementDialog from '@/components/UserManagementDialog.vue'
import WelcomeScreen from '@/components/WelcomeScreen.vue'
import ProjectOverview from '@/components/ProjectOverview.vue'
import request from '@/utils/request'
import { useSystemStore } from '@/stores/system'

const router = useRouter()
const route = useRoute()
const auth = useAuthStore()
const docs = useDocsStore()
const projects = useProjectsStore()
const system = useSystemStore()

const HOME_SIDEBAR_KEY = 'markflow.home.sidebar_open'
const HOME_LAST_PROJECT_KEY = 'markflow.home.last_project'
const HOME_LAST_VIEW_KEY = 'markflow.home.last_view'
const HOME_LAST_NODE_PREFIX = 'markflow.home.last_node.'

function readBool(key: string, fallback: boolean): boolean {
  const raw = localStorage.getItem(key)
  if (raw === '1') return true
  if (raw === '0') return false
  return fallback
}

function getLastNodeKey(projectId: number) {
  return `${HOME_LAST_NODE_PREFIX}${projectId}`
}

function parseStoredId(value: unknown): number | null {
  if (typeof value === 'number' && Number.isInteger(value)) return value
  if (typeof value !== 'string' || !value.trim()) return null
  const parsed = Number(value)
  return Number.isInteger(parsed) ? parsed : null
}

function readLastNode(projectId: number): number | null {
  const value = localStorage.getItem(getLastNodeKey(projectId))
  return parseStoredId(value)
}

function writeLastNode(projectId: number, nodeId: number | null) {
  const key = getLastNodeKey(projectId)
  if (nodeId !== null) localStorage.setItem(key, String(nodeId))
  else localStorage.removeItem(key)
}

function normalizeQueryId(value: unknown): number | null {
  return parseStoredId(value)
}

function projectExists(projectId: number | null): projectId is number {
  return Boolean(projectId && projects.projects.some((project) => project.id === projectId))
}

const sidebarOpen = ref(readBool(HOME_SIDEBAR_KEY, true))
const mobileOpen = ref(false)
const showProjectOverview = ref(true)
const showProfile = ref(false)
const showAttachments = ref(false)
const showShare = ref(false)
const showPwChange = ref(false)
const showSystemSettings = ref(false)
const showUserManagement = ref(false)
const shareTarget = ref<DocNode | null>(null)
const changingPw = ref(false)
const pwForm = ref({ old: '', new: '', confirm: '' })
const restoringHomeState = ref(true)

const showSidebar = computed(() => !showProjectOverview.value && Boolean(projects.currentProject))

function persistHomeCache() {
  localStorage.setItem(HOME_SIDEBAR_KEY, sidebarOpen.value ? '1' : '0')
  localStorage.setItem(HOME_LAST_VIEW_KEY, showProjectOverview.value ? 'overview' : 'project')

  if (projects.currentProjectId) {
    localStorage.setItem(HOME_LAST_PROJECT_KEY, String(projects.currentProjectId))
    writeLastNode(projects.currentProjectId, docs.currentNode?.id || null)
  } else {
    localStorage.removeItem(HOME_LAST_PROJECT_KEY)
  }
}

function buildHomeQuery(): Record<string, string> {
  if (showProjectOverview.value) {
    return { view: 'overview' }
  }

  const query: Record<string, string> = {}
  if (projects.currentProjectId) query.project = String(projects.currentProjectId)
  if (docs.currentNode?.id) query.doc = String(docs.currentNode.id)
  return query
}

function sameQuery(a: Record<string, string>, b: Record<string, unknown>) {
  const aKeys = Object.keys(a)
  const bKeys = Object.keys(b).filter((key) => typeof b[key] === 'string')
  if (aKeys.length !== bKeys.length) return false
  return aKeys.every((key) => b[key] === a[key])
}

async function syncHomeRoute() {
  const nextQuery = buildHomeQuery()
  if (sameQuery(nextQuery, route.query as Record<string, unknown>)) return
  await router.replace({ path: '/', query: nextQuery })
}

async function restoreHomeState() {
  await projects.fetchProjects()

  if (!projects.projects.length) {
    projects.clearCurrentProject()
    showProjectOverview.value = true
    docs.tree = []
    docs.currentNode = null
    return
  }

  const queryProjectId = normalizeQueryId(route.query.project)
  const queryDocId = normalizeQueryId(route.query.doc)
  const queryView = typeof route.query.view === 'string' && route.query.view.trim() ? route.query.view : null
  const cachedProjectId = parseStoredId(localStorage.getItem(HOME_LAST_PROJECT_KEY))
  const cachedView = typeof localStorage.getItem(HOME_LAST_VIEW_KEY) === 'string'
    ? localStorage.getItem(HOME_LAST_VIEW_KEY)
    : null

  const targetProjectIdCandidate =
    queryProjectId
    || cachedProjectId
    || projects.currentProjectId
    || projects.projects[0]?.id

  const targetProjectId = projectExists(targetProjectIdCandidate)
    ? targetProjectIdCandidate
    : projects.projects[0].id

  projects.selectProject(targetProjectId)

  const shouldShowOverview = queryView === 'overview'
    || (
      !queryView
      && !queryProjectId
      && !queryDocId
      && cachedView === 'overview'
    )

  showProjectOverview.value = shouldShowOverview
  docs.tree = []
  docs.currentNode = null

  if (showProjectOverview.value) {
    return
  }

  await docs.fetchTree(targetProjectId)

  const cachedDocId = readLastNode(targetProjectId)
  const targetDocId = queryDocId || cachedDocId
  if (!targetDocId) return

  try {
    await docs.fetchNode(targetDocId)
  } catch {
    docs.currentNode = null
  }
}

onMounted(async () => {
  await system.fetchPublicSettings().catch(() => {})
  await auth.refreshUser().catch(() => {})
  await restoreHomeState()
  restoringHomeState.value = false
  persistHomeCache()
  await syncHomeRoute()
})

watch(sidebarOpen, (open) => {
  localStorage.setItem(HOME_SIDEBAR_KEY, open ? '1' : '0')
})

watch(
  [showProjectOverview, () => projects.currentProjectId, () => docs.currentNode?.id],
  () => {
    if (restoringHomeState.value) return
    persistHomeCache()
    void syncHomeRoute()
  }
)

function handleUserCmd(cmd: string) {
  if (cmd === 'profile') showProfile.value = true
  else if (cmd === 'password') showPwChange.value = true
  else if (cmd === 'attachments') showAttachments.value = true
  else if (cmd === 'system') showSystemSettings.value = true
  else if (cmd === 'users') showUserManagement.value = true
  else if (cmd === 'logout') {
    ElMessageBox.confirm('确认退出登录？', '提示', {
      type: 'warning', confirmButtonText: '退出', cancelButtonText: '取消'
    }).then(() => { auth.logout(); router.push('/login') }).catch(() => {})
  }
}

function openShare(node: DocNode) {
  shareTarget.value = node
  showShare.value = true
}

function backToOverview() {
  showProjectOverview.value = true
  docs.currentNode = null
  mobileOpen.value = false
}

async function enterProject(projectId: number) {
  projects.selectProject(projectId)
  showProjectOverview.value = false
  docs.currentNode = null
  await docs.fetchTree(projectId)
}

async function createProject(payload: { name: string; description: string; background_image: string }) {
  try {
    await projects.createProject(payload)
    showProjectOverview.value = true
    docs.currentNode = null
    ElMessage.success('项目创建成功')
  } catch (e: any) {
    ElMessage.error(e.response?.data?.error || '项目创建失败')
  }
}

async function updateProject(projectId: number, payload: { name: string; description: string; background_image: string }) {
  try {
    await projects.updateProject(projectId, payload)
    ElMessage.success('项目更新成功')
  } catch (e: any) {
    ElMessage.error(e.response?.data?.error || '项目更新失败')
  }
}

async function deleteProject(projectId: number) {
  const wasCurrent = projects.currentProjectId === projectId
  try {
    await projects.deleteProject(projectId)
    ElMessage.success('项目已删除')

    if (wasCurrent) {
      docs.currentNode = null
      if (!projects.currentProjectId) {
        docs.tree = []
        showProjectOverview.value = true
        return
      }
      if (!showProjectOverview.value) {
        await docs.fetchTree(projects.currentProjectId)
      }
    }
  } catch (e: any) {
    ElMessage.error(e.response?.data?.error || '项目删除失败')
  }
}

async function changePw() {
  if (!pwForm.value.old || !pwForm.value.new) { ElMessage.warning('请填写所有字段'); return }
  if (pwForm.value.new !== pwForm.value.confirm) { ElMessage.error('两次密码不一致'); return }
  changingPw.value = true
  try {
    await request.put('/auth/password', { old_password: pwForm.value.old, new_password: pwForm.value.new })
    ElMessage.success('密码修改成功')
    showPwChange.value = false
    pwForm.value = { old: '', new: '', confirm: '' }
  } catch (e: any) {
    ElMessage.error(e.response?.data?.error || '修改失败')
  } finally {
    changingPw.value = false
  }
}
</script>

<style scoped>
.app-layout {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}

/* ── Header ── */
.app-header {
  height: var(--header-height);
  background: linear-gradient(
    180deg,
    color-mix(in srgb, var(--bg2) 92%, transparent) 0%,
    color-mix(in srgb, var(--bg2) 78%, transparent) 100%
  );
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px 0 8px;
  flex-shrink: 0;
  gap: 8px;
  z-index: 100;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  overflow: hidden;
}

.sidebar-toggle {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--text2);
  border-radius: var(--r-sm);
  cursor: pointer;
  flex-shrink: 0;
  transition: all 0.12s;
}

.sidebar-toggle:hover {
  background: color-mix(in srgb, var(--blue) 14%, transparent);
  color: var(--text);
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
  padding: 0 6px 0 4px;
}

.brand-logo {
  width: 24px;
  height: 24px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  box-shadow: 0 6px 18px rgba(20, 184, 166, 0.25);
}

.brand-logo svg {
  width: 24px;
  height: 24px;
  display: block;
}

.brand-text {
  display: flex;
  flex-direction: column;
  line-height: 1.05;
}

.brand-name {
  font-size: 14px;
  font-weight: 700;
  color: var(--text);
  letter-spacing: -0.2px;
}

.brand-sub {
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.7px;
  text-transform: uppercase;
  color: color-mix(in srgb, var(--text3) 90%, transparent);
}

.breadcrumb {
  display: flex;
  align-items: center;
  gap: 4px;
  overflow: hidden;
}

.bc-sep {
  color: var(--text3);
  font-size: 14px;
  flex-shrink: 0;
}

.bc-item {
  font-size: 13px;
  color: var(--text2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 200px;
}

.bc-item.clickable {
  cursor: pointer;
  transition: color 0.12s;
}

.bc-item.clickable:hover { color: var(--blue); }
.bc-item.bc-current { color: var(--text); font-weight: 500; }

.header-right { flex-shrink: 0; }

.user-menu {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 8px;
  border-radius: var(--r-md);
  cursor: pointer;
  transition: background 0.12s;
}

.user-menu:hover { background: var(--bg3); }

.user-avatar {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  background: linear-gradient(135deg, var(--green), var(--blue));
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  font-weight: 700;
  color: #fff;
  flex-shrink: 0;
  overflow: hidden;
}

.user-avatar img { width: 100%; height: 100%; object-fit: cover; }

.user-name {
  font-size: 13px;
  color: var(--text);
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ── Body ── */
.app-body {
  flex: 1;
  display: flex;
  overflow: hidden;
  position: relative;
}

.sidebar {
  width: var(--sidebar-width);
  flex-shrink: 0;
  background: linear-gradient(
    180deg,
    color-mix(in srgb, var(--bg2) 94%, transparent) 0%,
    color-mix(in srgb, var(--bg2) 84%, var(--bg)) 100%
  );
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  transition: width 0.22s cubic-bezier(0.4,0,0.2,1),
              min-width 0.22s cubic-bezier(0.4,0,0.2,1),
              opacity 0.22s;
}

.sidebar.sidebar-hidden {
  width: 0 !important;
  min-width: 0 !important;
  opacity: 0;
  pointer-events: none;
}

.main-area {
  flex: 1;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.main-scroll {
  flex: 1;
  overflow-y: auto;
}

/* ── Mobile ── */
@media (max-width: 768px) {
  .user-name { display: none; }
  .bc-item { max-width: 120px; }
  .brand-sub { display: none; }
  .sidebar {
    position: absolute;
    top: 0;
    left: 0;
    height: 100%;
    z-index: 50;
    transform: translateX(-100%);
    transition: transform 0.25s cubic-bezier(0.4,0,0.2,1);
    width: var(--sidebar-width) !important;
    opacity: 1;
  }
  .sidebar.sidebar-mobile {
    transform: translateX(0);
    box-shadow: var(--shadow-lg);
  }
  .sidebar.sidebar-hidden { transform: translateX(-100%); }
  .sidebar-mask {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--bg) 72%, transparent);
    z-index: 40;
  }
}
</style>
