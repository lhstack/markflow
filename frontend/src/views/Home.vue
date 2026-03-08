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
    <AgentPanel
      :page-scope="agentPageScope"
      :project-id="projects.currentProject?.id ?? null"
      :project-name="projects.currentProject?.name || ''"
      :doc-id="docs.currentNode?.id ?? null"
      :doc-name="docs.currentNode?.name || ''"
      :doc-type="(docs.currentNode?.node_type as 'doc' | 'dir' | null) ?? null"
      :doc-content="docs.currentNode?.content || ''"
      :project-catalog="agentProjectCatalog"
      :current-node-catalog="agentNodeCatalog"
      @navigate="handleAgentNavigate"
    />

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
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
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
import AgentPanel from '@/components/AgentPanel.vue'
import request from '@/utils/request'
import { useSystemStore } from '@/stores/system'
import { describeAgentEditorBridge, getAgentEditorBridge } from '@/utils/agentEditorBridge'
import { registerAgentToolRuntime, unregisterAgentToolRuntime } from '@/utils/agentTools'
import {
  dispatchAgentWriterChunk,
  dispatchAgentWriterComplete,
  dispatchAgentWriterStart,
} from '@/utils/agentWriter'

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
const agentPageScope = computed<'overview' | 'editor' | 'dir'>(() => {
  if (showProjectOverview.value) return 'overview'
  if (docs.currentNode?.node_type === 'doc') return 'editor'
  return 'dir'
})
const agentProjectCatalog = computed(() =>
  projects.projects
    .slice(0, 24)
    .map((project) => project.name.trim())
    .filter(Boolean)
    .join('、')
)
const agentNodeCatalog = computed(() => {
  const flat: string[] = []
  const walk = (nodes: DocNode[], trail: string[] = []) => {
    for (const node of nodes) {
      const path = [...trail, node.name].join(' / ')
      flat.push(`${path}（${node.node_type === 'doc' ? '文档' : '目录'}）`)
      if (node.children?.length) walk(node.children, [...trail, node.name])
      if (flat.length >= 40) return
    }
  }
  walk(docs.tree)
  return flat.join('、')
})

function normalizeAgentName(value: string) {
  return value.trim().toLocaleLowerCase()
}

function findDocNodeByName(nodes: DocNode[], targetName: string): DocNode | null {
  for (const node of nodes) {
    if (normalizeAgentName(node.name) === targetName) return node
    if (node.children?.length) {
      const found = findDocNodeByName(node.children, targetName)
      if (found) return found
    }
  }
  return null
}

function findDocNodeByPath(nodes: DocNode[], rawPath: string): DocNode | null {
  const segments = rawPath
    .split('/')
    .map((segment) => normalizeAgentName(segment))
    .filter(Boolean)

  if (!segments.length) return null

  let currentNodes = nodes
  let currentNode: DocNode | null = null

  for (const segment of segments) {
    currentNode = currentNodes.find((node) => normalizeAgentName(node.name) === segment) || null
    if (!currentNode) return null
    currentNodes = currentNode.children || []
  }

  return currentNode
}

function normalizeToolString(value: unknown) {
  return typeof value === 'string' ? value.trim() : ''
}

function normalizeToolInteger(value: unknown): number | null {
  if (typeof value === 'number' && Number.isInteger(value)) return value
  if (typeof value !== 'string' || !value.trim()) return null
  const parsed = Number(value)
  return Number.isInteger(parsed) ? parsed : null
}

function expandToolArgs(args: Record<string, any>) {
  const params = args.params && typeof args.params === 'object' && !Array.isArray(args.params)
    ? args.params as Record<string, any>
    : {}
  return { ...params, ...args }
}

function flattenDocTree(
  nodes: DocNode[],
  trail: string[] = [],
  rows: Array<{ node: DocNode; path: string; depth: number }> = [],
) {
  for (const node of nodes) {
    const nextTrail = [...trail, node.name]
    rows.push({
      node,
      path: nextTrail.join('/'),
      depth: trail.length,
    })
    if (node.children?.length) {
      flattenDocTree(node.children, nextTrail, rows)
    }
  }
  return rows
}

function serializeProject(project: { id: number; name: string; description: string; background_image?: string | null; sort_order: number; created_at: string; updated_at: string }) {
  return {
    id: project.id,
    name: project.name,
    description: project.description,
    background_image: project.background_image || null,
    sort_order: project.sort_order,
    created_at: project.created_at,
    updated_at: project.updated_at,
  }
}

function serializeNodeEntry(entry: { node: DocNode; path: string; depth: number }) {
  return {
    id: entry.node.id,
    project_id: entry.node.project_id ?? null,
    parent_id: entry.node.parent_id ?? null,
    name: entry.node.name,
    node_type: entry.node.node_type,
    path: entry.path,
    depth: entry.depth,
    sort_order: entry.node.sort_order,
    child_count: entry.node.children?.length || 0,
    created_at: entry.node.created_at,
    updated_at: entry.node.updated_at,
  }
}

async function wait(ms: number) {
  await new Promise((resolve) => window.setTimeout(resolve, ms))
}

async function waitForEditorBridge(docId: number, timeoutMs = 4000) {
  const startedAt = Date.now()
  while (Date.now() - startedAt < timeoutMs) {
    const bridge = getAgentEditorBridge()
    if (bridge?.docId === docId) {
      return bridge
    }
    await nextTick()
    await wait(40)
  }
  return null
}

async function ensureProjectsLoaded(refresh = false) {
  if (!refresh && projects.projects.length) {
    return projects.projects
  }
  await projects.fetchProjects()
  return projects.projects
}

async function resolveProjectTarget(
  rawArgs: Record<string, any>,
  options: { required?: boolean; refresh?: boolean } = {},
) {
  const args = expandToolArgs(rawArgs)
  const refresh = options.refresh ?? Boolean(args.refresh)
  await ensureProjectsLoaded(refresh)

  const projectId = normalizeToolInteger(args.project_id ?? args.projectId)
  const projectName = normalizeToolString(args.project_name ?? args.projectName)

  let project = null
  if (projectId !== null) {
    project = projects.projects.find((item) => item.id === projectId) || null
  } else if (projectName) {
    const targetName = normalizeAgentName(projectName)
    project = projects.projects.find((item) => normalizeAgentName(item.name) === targetName) || null
  } else if (projects.currentProject) {
    project = projects.currentProject
  }

  if (!project && options.required !== false) {
    throw new Error(projectId !== null || projectName ? '未找到目标项目' : '缺少项目参数')
  }

  return project
}

async function fetchProjectTreeSnapshot(projectId: number, refresh = false) {
  if (!refresh && projects.currentProjectId === projectId && docs.tree.length) {
    return docs.tree
  }

  const data = await request.get('/docs', {
    params: { project_id: projectId },
  }) as { tree?: DocNode[] }

  const tree = Array.isArray(data.tree) ? data.tree : []
  if (projects.currentProjectId === projectId) {
    docs.tree = tree
  }
  return tree
}

async function resolveNodeTarget(
  rawArgs: Record<string, any>,
  options: { required?: boolean; requireProject?: boolean; refresh?: boolean } = {},
) {
  const args = expandToolArgs(rawArgs)
  const project = await resolveProjectTarget(args, {
    required: options.requireProject !== false,
    refresh: options.refresh ?? Boolean(args.refresh),
  })
  const nodeId = normalizeToolInteger(args.node_id ?? args.doc_id ?? args.parent_id ?? args.nodeId ?? args.docId ?? args.parentId)
  const nodePath = normalizeToolString(args.node_path ?? args.doc_path ?? args.parent_path ?? args.nodePath ?? args.docPath ?? args.parentPath)
  const nodeName = normalizeToolString(args.node_name ?? args.doc_name ?? args.parent_name ?? args.nodeName ?? args.docName ?? args.parentName)

  if (!project) {
    if (options.required === false) return null
    throw new Error('缺少项目参数')
  }

  const tree = await fetchProjectTreeSnapshot(project.id, options.refresh ?? Boolean(args.refresh))
  const rows = flattenDocTree(tree)

  let entry = null
  if (nodeId !== null) {
    entry = rows.find((item) => item.node.id === nodeId) || null
  } else if (nodePath) {
    const targetPath = nodePath
      .split('/')
      .map((segment: string) => normalizeAgentName(segment))
      .filter(Boolean)
      .join('/')
    entry = rows.find((item) =>
      item.path
        .split('/')
        .map((segment) => normalizeAgentName(segment))
        .join('/') === targetPath
    ) || null
  } else if (nodeName) {
    const targetName = normalizeAgentName(nodeName)
    entry = rows.find((item) => normalizeAgentName(item.node.name) === targetName) || null
  } else if (docs.currentNode && docs.currentNode.project_id === project.id) {
    entry = rows.find((item) => item.node.id === docs.currentNode?.id) || null
  }

  if (!entry && options.required !== false) {
    throw new Error(nodeId !== null || nodePath || nodeName ? '未找到目标节点' : '缺少节点参数')
  }

  return entry ? { project, tree, entry } : null
}

async function getCurrentPageState() {
  const currentProject = projects.currentProject
  const currentRows = flattenDocTree(docs.tree)
  const currentEntry = docs.currentNode
    ? currentRows.find((item) => item.node.id === docs.currentNode?.id) || null
    : null
  const editorBridge = describeAgentEditorBridge()
  const liveEditor = getAgentEditorBridge()
  const content = liveEditor?.getValue() || docs.currentNode?.content || ''

  return {
    route: {
      name: typeof route.name === 'string' ? route.name : '',
      path: route.path,
      full_path: route.fullPath,
      query: route.query,
    },
    page_scope: agentPageScope.value,
    page_state: showProjectOverview.value
      ? 'project_overview'
      : docs.currentNode?.node_type === 'doc'
        ? 'document_editor'
        : docs.currentNode?.node_type === 'dir'
          ? 'directory_detail'
          : 'project_workspace',
    project: currentProject ? serializeProject(currentProject) : null,
    current_node: currentEntry ? serializeNodeEntry(currentEntry) : null,
    editor: {
      ...editorBridge,
      content_length: content.length,
      content_preview: content.slice(0, 500),
    },
    visible: {
      project_count: projects.projects.length,
      current_project_root_count: docs.tree.length,
      current_project_node_count: currentRows.length,
      project_catalog: agentProjectCatalog.value,
      current_node_catalog: agentNodeCatalog.value,
    },
    capabilities: {
      can_open_project: true,
      can_open_node: Boolean(currentProject),
      can_move_node: Boolean(currentProject),
      can_write_document: docs.currentNode?.node_type === 'doc',
      can_execute_javascript: true,
    },
  }
}

function listPageRoutes() {
  return {
    current_route: {
      name: typeof route.name === 'string' ? route.name : '',
      path: route.path,
      full_path: route.fullPath,
    },
    routes: [
      {
        route: 'home.overview',
        path: '/',
        description: '项目概览页，展示当前用户的所有项目卡片。',
        params: [],
      },
      {
        route: 'home.project',
        path: '/?project={project_id}',
        description: '进入指定项目并展示左侧文档树，主区域停留在项目工作区。',
        params: ['project_id 或 project_name'],
      },
      {
        route: 'home.doc',
        path: '/?project={project_id}&doc={node_id}',
        description: '进入指定项目并打开某个 Markdown 文档编辑页。',
        params: ['project_id 或 project_name', 'node_id 或 node_path 或 node_name'],
      },
      {
        route: 'home.dir',
        path: '/?project={project_id}&doc={node_id}',
        description: '进入指定项目并打开某个目录详情页。',
        params: ['project_id 或 project_name', 'node_id 或 node_path 或 node_name'],
      },
      {
        route: 'login',
        path: '/login',
        description: '登录页。',
        params: [],
      },
      {
        route: 'register',
        path: '/register',
        description: '注册页。',
        params: [],
      },
      {
        route: 'login.2fa',
        path: '/login/2fa',
        description: '两步验证页。',
        params: [],
      },
      {
        route: 'share',
        path: '/s/{token}',
        description: '分享文档页。',
        params: ['share_token'],
      },
    ],
    usage_notes: [
      '如果目标是打开项目，优先调用 open_project。',
      '如果目标是打开文档或目录，优先调用 open_tree_node。',
    ],
  }
}

async function navigateToPage(rawArgs: Record<string, any>) {
  const args = expandToolArgs(rawArgs)
  const routeName = normalizeToolString(args.route || args.target)

  if (!routeName) {
    throw new Error('navigate_to_page 缺少 route 参数')
  }

  if (routeName === 'home.overview' || routeName === 'home' || routeName === 'overview') {
    backToOverview()
  } else if (routeName === 'home.project' || routeName === 'project') {
    const project = await resolveProjectTarget(args)
    if (!project) throw new Error('未找到目标项目')
    await enterProject(project.id)
  } else if (routeName === 'home.doc' || routeName === 'doc' || routeName === 'node') {
    const target = await resolveNodeTarget(args)
    if (!target) throw new Error('未找到目标节点')
    if (target.entry.node.node_type !== 'doc') {
      throw new Error('目标节点不是文档，请改用 home.dir 或 open_tree_node')
    }
    if (projects.currentProjectId !== target.project.id || showProjectOverview.value) {
      await enterProject(target.project.id)
    }
    await openDocNode(target.entry.node.id)
  } else if (routeName === 'home.dir' || routeName === 'dir') {
    const target = await resolveNodeTarget(args)
    if (!target) throw new Error('未找到目标节点')
    if (target.entry.node.node_type !== 'dir') {
      throw new Error('目标节点不是目录，请改用 home.doc 或 open_tree_node')
    }
    if (projects.currentProjectId !== target.project.id || showProjectOverview.value) {
      await enterProject(target.project.id)
    }
    await openDocNode(target.entry.node.id)
  } else if (routeName === 'login') {
    await router.push('/login')
  } else if (routeName === 'register') {
    await router.push('/register')
  } else if (routeName === 'login.2fa') {
    await router.push('/login/2fa')
  } else if (routeName === 'share') {
    const shareToken = normalizeToolString(args.share_token ?? args.token)
    if (!shareToken) {
      throw new Error('share 路由缺少 share_token')
    }
    await router.push(`/s/${shareToken}`)
  } else {
    throw new Error(`不支持的 route: ${routeName}`)
  }

  return {
    ok: true,
    route: routeName,
    state: await getCurrentPageState(),
  }
}

async function listProjectsTool(rawArgs: Record<string, any>) {
  await ensureProjectsLoaded(Boolean(expandToolArgs(rawArgs).refresh))
  return {
    projects: projects.projects.map((project) => serializeProject(project)),
    total: projects.projects.length,
    current_project_id: projects.currentProjectId,
  }
}

async function openProjectTool(rawArgs: Record<string, any>) {
  const args = expandToolArgs(rawArgs)
  const project = await resolveProjectTarget(args)
  if (!project) throw new Error('未找到目标项目')

  if (projects.currentProjectId !== project.id || showProjectOverview.value) {
    await enterProject(project.id)
  } else if (args.fetch_tree !== false && !docs.tree.length) {
    await docs.fetchTree(project.id)
  }

  return {
    opened: serializeProject(project),
    state: await getCurrentPageState(),
  }
}

async function createProjectTool(rawArgs: Record<string, any>) {
  const args = expandToolArgs(rawArgs)
  const name = normalizeToolString(args.name)
  if (!name) {
    throw new Error('create_project 缺少 name 参数')
  }

  const created = await projects.createProject({
    name,
    description: normalizeToolString(args.description),
    background_image: normalizeToolString(args.background_image),
  })

  if (args.open_after_create === false) {
    showProjectOverview.value = true
    docs.currentNode = null
  } else {
    await enterProject(created.id)
  }

  return {
    created: serializeProject(created),
    state: await getCurrentPageState(),
  }
}

async function deleteProjectsTool(rawArgs: Record<string, any>) {
  const args = expandToolArgs(rawArgs)
  await ensureProjectsLoaded(false)

  const idTargets = Array.isArray(args.project_ids)
    ? args.project_ids.map((item: unknown) => normalizeToolInteger(item)).filter((item: number | null): item is number => item !== null)
    : []
  const nameTargets = Array.isArray(args.project_names)
    ? args.project_names.map((item: unknown) => normalizeToolString(item)).filter(Boolean)
    : []

  const resolvedByName = nameTargets
    .map((name) => projects.projects.find((item) => normalizeAgentName(item.name) === normalizeAgentName(name)) || null)
    .filter((item): item is NonNullable<typeof item> => Boolean(item))
    .map((item) => item.id)

  const targetIds = Array.from(new Set([...idTargets, ...resolvedByName]))
  if (!targetIds.length) {
    throw new Error('delete_projects 至少需要 project_ids 或 project_names')
  }

  const deleted: Array<{ id: number; name: string }> = []
  const failed: Array<{ id: number; error: string }> = []
  const deletingCurrent = targetIds.includes(projects.currentProjectId || -1)

  for (const projectId of targetIds) {
    const project = projects.projects.find((item) => item.id === projectId)
    if (!project) {
      failed.push({ id: projectId, error: '项目不存在' })
      continue
    }

    try {
      await projects.deleteProject(projectId)
      deleted.push({ id: projectId, name: project.name })
    } catch (error: any) {
      failed.push({
        id: projectId,
        error: error?.response?.data?.error || error?.message || '删除项目失败',
      })
    }
  }

  if (deletingCurrent) {
    docs.currentNode = null
    if (!projects.currentProjectId) {
      docs.tree = []
      showProjectOverview.value = true
    } else if (!showProjectOverview.value) {
      await docs.fetchTree(projects.currentProjectId)
    }
  }

  return {
    deleted,
    failed,
    remaining_project_count: projects.projects.length,
    current_project_id: projects.currentProjectId,
  }
}

async function getProjectTreeTool(rawArgs: Record<string, any>) {
  const args = expandToolArgs(rawArgs)
  const project = await resolveProjectTarget(args)
  if (!project) throw new Error('未找到目标项目')

  const tree = await fetchProjectTreeSnapshot(project.id, Boolean(args.refresh))
  const rows = flattenDocTree(tree)

  return {
    project: serializeProject(project),
    stats: {
      total_nodes: rows.length,
      doc_count: rows.filter((item) => item.node.node_type === 'doc').length,
      dir_count: rows.filter((item) => item.node.node_type === 'dir').length,
    },
    tree,
    flat_nodes: rows.map((item) => serializeNodeEntry(item)),
  }
}

async function createTreeNodeTool(rawArgs: Record<string, any>) {
  const args = expandToolArgs(rawArgs)
  const project = await resolveProjectTarget(args)
  if (!project) throw new Error('未找到目标项目')

  const nodeType = normalizeToolString(args.node_type)
  if (nodeType !== 'doc' && nodeType !== 'dir') {
    throw new Error('node_type 只能是 doc 或 dir')
  }

  const name = normalizeToolString(args.name)
  if (!name) {
    throw new Error('create_tree_node 缺少 name 参数')
  }

  let parentId: number | undefined
  const hasParentLocator = args.parent_id !== undefined
    || args.parent_path !== undefined
    || args.parent_name !== undefined
    || args.parentId !== undefined
    || args.parentPath !== undefined
    || args.parentName !== undefined

  if (hasParentLocator) {
    const parentTarget = await resolveNodeTarget({
      ...args,
      node_id: args.parent_id ?? args.parentId,
      node_path: args.parent_path ?? args.parentPath,
      node_name: args.parent_name ?? args.parentName,
    })
    if (!parentTarget) {
      throw new Error('未找到目标父目录')
    }
    if (parentTarget.entry.node.node_type !== 'dir') {
      throw new Error('父节点必须是目录')
    }
    parentId = parentTarget.entry.node.id
  }

  const data = await request.post('/docs', {
    project_id: project.id,
    parent_id: parentId,
    name,
    node_type: nodeType,
    content: nodeType === 'doc' ? normalizeToolString(args.content) : undefined,
  }) as { node?: DocNode }
  const created = data.node
  if (!created) {
    throw new Error('创建节点失败')
  }

  if (args.open_after_create !== false) {
    if (projects.currentProjectId !== project.id || showProjectOverview.value) {
      await enterProject(project.id)
    } else {
      await docs.fetchTree(project.id)
    }
    await openDocNode(created.id)
  } else if (projects.currentProjectId === project.id && !showProjectOverview.value) {
    await docs.fetchTree(project.id)
  }

  const tree = await fetchProjectTreeSnapshot(project.id, true)
  const rows = flattenDocTree(tree)
  const createdEntry = rows.find((item) => item.node.id === created.id) || {
    node: created,
    path: created.name,
    depth: parentId ? 1 : 0,
  }

  return {
    created: serializeNodeEntry(createdEntry),
    state: await getCurrentPageState(),
  }
}

async function moveTreeNodeTool(rawArgs: Record<string, any>) {
  const args = expandToolArgs(rawArgs)
  const source = await resolveNodeTarget({
    ...args,
    node_id: args.node_id ?? args.doc_id ?? args.nodeId ?? args.docId,
    node_path: args.node_path ?? args.doc_path ?? args.nodePath ?? args.docPath,
    node_name: args.node_name ?? args.doc_name ?? args.nodeName ?? args.docName,
  })
  if (!source) {
    throw new Error('未找到要移动的节点')
  }

  const moveToRoot = args.to_root === true
  let parentId: number | null = null

  if (!moveToRoot) {
    const targetParent = await resolveNodeTarget({
      ...args,
      project_id: args.target_project_id ?? args.project_id ?? args.targetProjectId ?? args.projectId,
      project_name: args.target_project_name ?? args.project_name ?? args.targetProjectName ?? args.projectName,
      node_id: args.target_parent_id ?? args.parent_id ?? args.targetParentId ?? args.parentId,
      node_path: args.target_parent_path ?? args.parent_path ?? args.targetParentPath ?? args.parentPath,
      node_name: args.target_parent_name ?? args.parent_name ?? args.targetParentName ?? args.parentName,
    }, {
      required: false,
    })

    if (targetParent) {
      if (targetParent.project.id !== source.project.id) {
        throw new Error('当前仅支持在同一项目内移动节点')
      }
      if (targetParent.entry.node.node_type !== 'dir') {
        throw new Error('目标父节点必须是目录')
      }
      parentId = targetParent.entry.node.id
    } else if (
      args.target_parent_id !== undefined
      || args.parent_id !== undefined
      || args.target_parent_path !== undefined
      || args.parent_path !== undefined
      || args.target_parent_name !== undefined
      || args.parent_name !== undefined
    ) {
      throw new Error('未找到目标父目录')
    }
  }

  const tree = await fetchProjectTreeSnapshot(source.project.id, true)
  const siblings = flattenDocTree(tree)
    .filter((item) => (item.node.parent_id ?? null) === parentId && item.node.id !== source.entry.node.id)
    .sort((a, b) => a.node.sort_order - b.node.sort_order)

  const explicitSortOrder = normalizeToolInteger(args.sort_order ?? args.sortOrder)
  const sortOrder = explicitSortOrder ?? siblings.length

  await docs.moveNode(
    source.entry.node.id,
    parentId,
    sortOrder,
    source.project.id,
    true,
  )

  if (docs.currentNode?.id === source.entry.node.id) {
    await docs.fetchNode(source.entry.node.id)
  }

  const refreshedTree = await fetchProjectTreeSnapshot(source.project.id, true)
  const movedEntry = flattenDocTree(refreshedTree).find((item) => item.node.id === source.entry.node.id) || null

  return {
    moved: true,
    target_parent_id: parentId,
    sort_order: sortOrder,
    node: movedEntry ? serializeNodeEntry(movedEntry) : serializeNodeEntry(source.entry),
    state: await getCurrentPageState(),
  }
}

async function openTreeNodeTool(rawArgs: Record<string, any>) {
  const args = expandToolArgs(rawArgs)
  const target = await resolveNodeTarget(args)
  if (!target) throw new Error('未找到目标节点')

  if (projects.currentProjectId !== target.project.id || showProjectOverview.value) {
    await enterProject(target.project.id)
  }
  await openDocNode(target.entry.node.id)

  return {
    opened: serializeNodeEntry(target.entry),
    state: await getCurrentPageState(),
  }
}

async function writeDocumentTool(rawArgs: Record<string, any>) {
  const args = expandToolArgs(rawArgs)
  const content = typeof args.content === 'string' ? args.content : ''
  if (!content) {
    throw new Error('write_document 缺少 content 参数')
  }

  let target = null
  if (
    args.doc_id !== undefined
    || args.doc_path !== undefined
    || args.doc_name !== undefined
    || args.node_id !== undefined
    || args.node_path !== undefined
    || args.node_name !== undefined
    || args.project_id !== undefined
    || args.project_name !== undefined
  ) {
    target = await resolveNodeTarget({
      ...args,
      node_id: args.doc_id ?? args.node_id ?? args.docId ?? args.nodeId,
      node_path: args.doc_path ?? args.node_path ?? args.docPath ?? args.nodePath,
      node_name: args.doc_name ?? args.node_name ?? args.docName ?? args.nodeName,
    })
  } else if (docs.currentNode) {
    const rows = flattenDocTree(docs.tree)
    const entry = rows.find((item) => item.node.id === docs.currentNode?.id) || null
    if (projects.currentProject && entry) {
      target = { project: projects.currentProject, tree: docs.tree, entry }
    }
  }

  if (!target) {
    throw new Error('未找到要写入的文档')
  }
  if (target.entry.node.node_type !== 'doc') {
    throw new Error('write_document 只能写入文档，不能写入目录')
  }

  if (projects.currentProjectId !== target.project.id || docs.currentNode?.id !== target.entry.node.id || showProjectOverview.value) {
    if (projects.currentProjectId !== target.project.id || showProjectOverview.value) {
      await enterProject(target.project.id)
    }
    await openDocNode(target.entry.node.id)
  }

  const bridge = await waitForEditorBridge(target.entry.node.id)
  if (!bridge) {
    throw new Error('目标文档编辑器尚未完成初始化')
  }

  const mode = normalizeToolString(args.mode) === 'replace' ? 'replace' : 'append'
  const shouldSave = args.save === true

  dispatchAgentWriterStart({
    docId: target.entry.node.id,
    mode,
    save: shouldSave,
  })
  dispatchAgentWriterChunk({
    docId: target.entry.node.id,
    chunk: content,
  })
  dispatchAgentWriterComplete({
    docId: target.entry.node.id,
  })

  return {
    written: true,
    mode,
    save: shouldSave,
    target: serializeNodeEntry(target.entry),
    content_length: content.length,
  }
}

async function readDocumentTool(rawArgs: Record<string, any>) {
  const args = expandToolArgs(rawArgs)
  let target = null

  if (
    args.doc_id !== undefined
    || args.doc_path !== undefined
    || args.doc_name !== undefined
    || args.node_id !== undefined
    || args.node_path !== undefined
    || args.node_name !== undefined
    || args.project_id !== undefined
    || args.project_name !== undefined
  ) {
    target = await resolveNodeTarget({
      ...args,
      node_id: args.doc_id ?? args.node_id ?? args.docId ?? args.nodeId,
      node_path: args.doc_path ?? args.node_path ?? args.docPath ?? args.nodePath,
      node_name: args.doc_name ?? args.node_name ?? args.docName ?? args.nodeName,
    })
  } else if (docs.currentNode?.node_type === 'doc' && projects.currentProject) {
    const rows = flattenDocTree(docs.tree)
    const entry = rows.find((item) => item.node.id === docs.currentNode?.id) || null
    if (entry) {
      target = { project: projects.currentProject, tree: docs.tree, entry }
    }
  }

  if (!target) {
    throw new Error('未找到要读取的文档')
  }
  if (target.entry.node.node_type !== 'doc') {
    throw new Error('read_document 只能读取文档，不能读取目录')
  }

  const data = await request.get(`/docs/${target.entry.node.id}`) as { node?: DocNode }
  const node = data.node
  if (!node) {
    throw new Error('读取文档失败')
  }

  return {
    project: serializeProject(target.project),
    document: serializeNodeEntry(target.entry),
    content: node.content || '',
    content_length: (node.content || '').length,
  }
}

async function deleteTreeNodesTool(rawArgs: Record<string, any>) {
  const args = expandToolArgs(rawArgs)
  const project = await resolveProjectTarget(args)
  if (!project) throw new Error('未找到目标项目')

  const tree = await fetchProjectTreeSnapshot(project.id, false)
  const rows = flattenDocTree(tree)

  const nodeIds = Array.isArray(args.node_ids)
    ? args.node_ids.map((item: unknown) => normalizeToolInteger(item)).filter((item: number | null): item is number => item !== null)
    : []
  const nodePaths = Array.isArray(args.node_paths)
    ? args.node_paths.map((item: unknown) => normalizeToolString(item)).filter(Boolean)
    : []

  const resolvedByPath = nodePaths
    .map((rawPath) => {
      const normalizedPath = rawPath
        .split('/')
        .map((segment) => normalizeAgentName(segment))
        .filter(Boolean)
        .join('/')
      return rows.find((item) =>
        item.path
          .split('/')
          .map((segment) => normalizeAgentName(segment))
          .join('/') === normalizedPath
      ) || null
    })
    .filter((item): item is NonNullable<typeof item> => Boolean(item))
    .map((item) => item.node.id)

  const targetIds = Array.from(new Set([...nodeIds, ...resolvedByPath]))
  if (!targetIds.length) {
    throw new Error('delete_tree_nodes 至少需要 node_ids 或 node_paths')
  }

  const entries = targetIds
    .map((id) => rows.find((item) => item.node.id === id) || null)
    .filter((item): item is NonNullable<typeof item> => Boolean(item))
    .sort((a, b) => b.depth - a.depth)

  const deleted: Array<{ id: number; path: string }> = []
  const failed: Array<{ id: number; error: string }> = []
  const deletingCurrentNode = targetIds.includes(docs.currentNode?.id || -1)

  for (const entry of entries) {
    try {
      await request.delete(`/docs/${entry.node.id}`)
      deleted.push({ id: entry.node.id, path: entry.path })
    } catch (error: any) {
      failed.push({
        id: entry.node.id,
        error: error?.response?.data?.error || error?.message || '删除节点失败',
      })
    }
  }

  if (projects.currentProjectId === project.id) {
    await docs.fetchTree(project.id)
    if (deletingCurrentNode) {
      docs.currentNode = null
    }
  }

  return {
    deleted,
    failed,
    current_project_id: projects.currentProjectId,
    current_node_id: docs.currentNode?.id ?? null,
  }
}

function getMarkdownEditorRuntimeTool() {
  const bridge = describeAgentEditorBridge()
  const liveBridge = getAgentEditorBridge()
  const liveValue = liveBridge?.getValue() || ''

  return {
    ...bridge,
    current_document: docs.currentNode?.node_type === 'doc'
      ? {
        id: docs.currentNode.id,
        name: docs.currentNode.name,
        content_length: liveValue.length,
        content_preview: liveValue.slice(0, 500),
      }
      : null,
    globals: [
      {
        name: 'editor',
        description: '当前 Markdown 编辑器桥接对象，可在 execute_browser_javascript 中直接调用，也会挂到 window.editor。',
      },
      {
        name: 'markflow',
        description: 'MarkFlow 前端工具助手对象，可在 execute_browser_javascript 中直接调用，也会挂到 window.markflow。',
      },
    ],
    usage_notes: [
      '如果只是读取当前文档内容，优先使用 read_document。',
      '如果只是写入或改写文档，优先使用 write_document。',
      '只有在需要细粒度 DOM/编辑器动作时再使用 execute_browser_javascript。',
    ],
  }
}

function getBrowserRuntimeTool(rawArgs: Record<string, any>) {
  const args = expandToolArgs(rawArgs)
  const includeStorage = Boolean(args.include_storage)
  return {
    location: {
      href: window.location.href,
      origin: window.location.origin,
      pathname: window.location.pathname,
      search: window.location.search,
      hash: window.location.hash,
    },
    document: {
      title: document.title,
      ready_state: document.readyState,
    },
    history: {
      length: window.history.length,
    },
    navigator: {
      user_agent: window.navigator.userAgent,
      language: window.navigator.language,
      languages: window.navigator.languages,
      on_line: window.navigator.onLine,
    },
    viewport: {
      width: window.innerWidth,
      height: window.innerHeight,
      device_pixel_ratio: window.devicePixelRatio,
    },
    globals: [
      {
        name: 'window',
        description: '完整浏览器 window 对象。',
      },
      {
        name: 'document',
        description: 'DOM 文档对象，用于查询节点、读取文本、触发事件。',
      },
      {
        name: 'location/history/navigator',
        description: '浏览器路由、历史记录和环境信息对象。',
      },
      {
        name: 'localStorage/sessionStorage',
        description: '本地存储对象。',
      },
      {
        name: 'editor',
        description: '当前 Markdown 编辑器桥接对象。',
      },
      {
        name: 'markflow',
        description: 'MarkFlow 页面工具助手对象。',
      },
    ],
    storage: includeStorage
      ? {
        local_storage_keys: Object.keys(window.localStorage),
        session_storage_keys: Object.keys(window.sessionStorage),
      }
      : undefined,
  }
}

function installAgentToolRuntime() {
  registerAgentToolRuntime({
    getCurrentPageState,
    listPageRoutes,
    navigateToPage,
    listProjects: listProjectsTool,
    openProject: openProjectTool,
    createProject: createProjectTool,
    deleteProjects: deleteProjectsTool,
    getProjectTree: getProjectTreeTool,
    createTreeNode: createTreeNodeTool,
    moveTreeNode: moveTreeNodeTool,
    openTreeNode: openTreeNodeTool,
    writeDocument: writeDocumentTool,
    readDocument: readDocumentTool,
    deleteTreeNodes: deleteTreeNodesTool,
    getMarkdownEditorRuntime: getMarkdownEditorRuntimeTool,
    getBrowserRuntime: getBrowserRuntimeTool,
  })
}

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
  installAgentToolRuntime()
  await system.fetchPublicSettings().catch(() => {})
  await auth.refreshUser().catch(() => {})
  await restoreHomeState()
  restoringHomeState.value = false
  persistHomeCache()
  await syncHomeRoute()
})

onUnmounted(() => {
  unregisterAgentToolRuntime()
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

async function openDocNode(nodeId: number) {
  showProjectOverview.value = false
  await docs.fetchNode(nodeId)
}

async function handleAgentNavigate(target: { kind: 'overview' | 'project' | 'doc'; name?: string }) {
  if (target.kind === 'overview') {
    backToOverview()
    return
  }

  if (target.kind === 'project') {
    const targetName = normalizeAgentName(target.name || '')
    if (!targetName) return
    const project = projects.projects.find((item) => normalizeAgentName(item.name) === targetName)
    if (!project) {
      ElMessage.warning(`未找到项目“${target.name}”`)
      return
    }
    await enterProject(project.id)
    return
  }

  const targetName = normalizeAgentName(target.name || '')
  if (!targetName) return

  if (!projects.currentProjectId) {
    ElMessage.warning('当前没有已打开的项目，无法跳转到文档')
    return
  }

  if (!docs.tree.length) {
    await docs.fetchTree(projects.currentProjectId)
  }

  const node = findDocNodeByPath(docs.tree, target.name || '')
    || findDocNodeByName(docs.tree, targetName)
  if (!node) {
    ElMessage.warning(`当前项目中未找到“${target.name}”`)
    return
  }

  await openDocNode(node.id)
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
