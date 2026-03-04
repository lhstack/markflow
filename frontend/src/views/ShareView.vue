<template>
  <div class="share-page">
    <div v-if="state === 'loading'" class="share-center">
      <el-icon class="spin-icon"><Loading /></el-icon>
      <p>加载中...</p>
    </div>

    <div v-else-if="state === 'expired'" class="share-center">
      <div class="state-icon danger">
        <el-icon><WarningFilled /></el-icon>
      </div>
      <h2>分享链接已过期</h2>
      <p>此链接已超过有效期，请联系分享者获取新链接。</p>
      <router-link to="/"><el-button type="primary">返回首页</el-button></router-link>
    </div>

    <div v-else-if="state === 'notfound'" class="share-center">
      <div class="state-icon">
        <el-icon><CircleClose /></el-icon>
      </div>
      <h2>链接不存在</h2>
      <p>该分享链接无效或已被删除。</p>
      <router-link to="/"><el-button type="primary">返回首页</el-button></router-link>
    </div>

    <div v-else-if="state === 'password'" class="share-center">
      <div class="pw-card">
        <div class="pw-logo">
          <MarkFlowLogo />
          <span>MarkFlow</span>
        </div>
        <div class="pw-icon"><el-icon><Lock /></el-icon></div>
        <h2>此内容需要访问密码</h2>
        <p class="pw-doc-name">{{ shareInfo?.doc_name }}</p>
        <el-input
          v-model="password"
          type="password"
          placeholder="请输入访问密码"
          size="large"
          show-password
          @keydown.enter="verifyPassword"
        />
        <p v-if="wrongPassword" class="pw-error">密码错误，请重试</p>
        <el-button type="primary" size="large" style="width: 100%" :loading="verifying" @click="verifyPassword">
          访问
        </el-button>
      </div>
    </div>

    <div v-else-if="state === 'doc'" class="share-doc-layout">
      <ShareHeader
        :doc-name="content?.name"
        :share-info="shareInfo"
        :sidebar-open="shareSidebarOpen"
        :show-sidebar-toggle="true"
        @toggle-sidebar="toggleShareSidebar"
      />
      <div class="share-doc-body" :class="{ 'is-sidebar-collapsed': !shareSidebarOpen }">
        <aside class="share-doc-sidebar">
          <div class="sidebar-title">文档目录</div>
          <div class="doc-side-card" v-if="shareSidebarOpen">
            <div class="doc-side-name">{{ content?.name || '未命名文档' }}</div>
          </div>
        </aside>
        <main class="share-doc-content">
          <div class="preview-shell">
            <VMdPreview class="share-preview" :text="content?.content || ''" />
          </div>
        </main>
      </div>
    </div>

    <div v-else-if="state === 'dir'" class="share-dir-layout">
      <ShareHeader
        :doc-name="content?.name"
        :share-info="shareInfo"
        :sidebar-open="shareSidebarOpen"
        :show-sidebar-toggle="true"
        @toggle-sidebar="toggleShareSidebar"
      />
      <div class="share-dir-body" :class="{ 'is-sidebar-collapsed': !shareSidebarOpen }">
        <aside class="share-dir-sidebar">
          <div class="sidebar-title">目录</div>
          <DirTreeNav
            v-if="shareSidebarOpen"
            :nodes="content?.children || []"
            :selected-id="selectedDoc?.id"
            :expanded-ids="expandedDirIds"
            @select="selectDoc"
            @toggle-dir="toggleDirExpand"
          />
        </aside>
        <main class="share-dir-content">
          <div v-if="selectedDoc" class="preview-shell">
            <VMdPreview class="share-preview" :text="selectedDoc.content || ''" />
          </div>
          <div v-else class="dir-placeholder">
            <el-icon><Folder /></el-icon>
            <p>从左侧选择文档查看</p>
          </div>
        </main>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { defineComponent, h, onMounted, ref } from 'vue'
import VMdPreview from '@kangc/v-md-editor/lib/preview'
import githubTheme from '@kangc/v-md-editor/lib/theme/github.js'
import hljs from 'highlight.js'
import '@kangc/v-md-editor/lib/style/preview.css'
import '@kangc/v-md-editor/lib/theme/style/github.css'
import request from '@/utils/request'
import { useRoute } from 'vue-router'

const preview = VMdPreview as any
if (!preview.__markflowConfigured) {
  preview.use(githubTheme, { Hljs: hljs })
  preview.__markflowConfigured = true
}

const route = useRoute()

type ShareState = 'loading' | 'password' | 'doc' | 'dir' | 'expired' | 'notfound'

interface ShareInfo {
  doc_name: string
  has_password: boolean
  expires_at?: string | null
}

interface ShareNode {
  id: string
  name: string
  node_type: 'doc' | 'dir'
  content?: string
  children?: ShareNode[]
}

const state = ref<ShareState>('loading')
const shareInfo = ref<ShareInfo | null>(null)
const content = ref<ShareNode | null>(null)
const password = ref('')
const verifying = ref(false)
const wrongPassword = ref(false)
const selectedDoc = ref<ShareNode | null>(null)
const shareToken = String(route.params.token || '')
const shareSidebarOpen = ref(true)
const expandedDirIds = ref<Set<string>>(new Set())
const shareDirStateInitialized = ref(false)
const hasDirExpansionPreference = ref(false)

const SHARE_PASSWORD_KEY_PREFIX = 'markflow.share.password.'
const SHARE_UI_KEY_PREFIX = 'markflow.share.ui.'

function getSharePasswordKey(token: string) {
  return `${SHARE_PASSWORD_KEY_PREFIX}${token}`
}

function getShareUiKey(token: string) {
  return `${SHARE_UI_KEY_PREFIX}${token}`
}

function readCachedPassword(token: string): string {
  return localStorage.getItem(getSharePasswordKey(token)) || ''
}

function cachePassword(token: string, pw: string) {
  localStorage.setItem(getSharePasswordKey(token), pw)
}

function clearCachedPassword(token: string) {
  localStorage.removeItem(getSharePasswordKey(token))
}

function loadShareUiState() {
  shareSidebarOpen.value = true
  expandedDirIds.value = new Set()
  hasDirExpansionPreference.value = false
  try {
    const raw = localStorage.getItem(getShareUiKey(shareToken))
    if (!raw) return
    const parsed = JSON.parse(raw) as { sidebar_open?: boolean; expanded_dir_ids?: string[] }
    if (typeof parsed.sidebar_open === 'boolean') {
      shareSidebarOpen.value = parsed.sidebar_open
    }
    if (Array.isArray(parsed.expanded_dir_ids)) {
      expandedDirIds.value = new Set(parsed.expanded_dir_ids.filter((id) => typeof id === 'string'))
      hasDirExpansionPreference.value = true
    }
  } catch {
    // ignore invalid cache
  }
}

function persistShareUiState() {
  localStorage.setItem(
    getShareUiKey(shareToken),
    JSON.stringify({
      sidebar_open: shareSidebarOpen.value,
      expanded_dir_ids: Array.from(expandedDirIds.value),
    })
  )
}

function toggleShareSidebar() {
  shareSidebarOpen.value = !shareSidebarOpen.value
  persistShareUiState()
}

function parseTime(value?: string | null): number {
  if (!value) return Number.NaN
  const direct = new Date(value).getTime()
  if (!Number.isNaN(direct)) return direct
  return new Date(`${value}Z`).getTime()
}

function isShareExpired(expiresAt?: string | null): boolean {
  if (!expiresAt) return false
  const ts = parseTime(expiresAt)
  if (Number.isNaN(ts)) return false
  return ts <= Date.now()
}

function selectDoc(doc: ShareNode) {
  selectedDoc.value = doc
}

function collectDirIds(nodes: ShareNode[], set: Set<string>) {
  for (const node of nodes) {
    if (node.node_type === 'dir') {
      set.add(node.id)
      if (node.children?.length) collectDirIds(node.children, set)
    }
  }
}

function syncExpandedDirState(nodes: ShareNode[]) {
  const currentDirIds = new Set<string>()
  collectDirIds(nodes, currentDirIds)

  if (!shareDirStateInitialized.value) {
    if (hasDirExpansionPreference.value) {
      expandedDirIds.value = new Set(
        Array.from(expandedDirIds.value).filter((id) => currentDirIds.has(id))
      )
    } else {
      expandedDirIds.value = currentDirIds
    }
    shareDirStateInitialized.value = true
    persistShareUiState()
    return
  }

  expandedDirIds.value = new Set(
    Array.from(expandedDirIds.value).filter((id) => currentDirIds.has(id))
  )
  persistShareUiState()
}

function toggleDirExpand(dirId: string) {
  const next = new Set(expandedDirIds.value)
  if (next.has(dirId)) next.delete(dirId)
  else next.add(dirId)
  expandedDirIds.value = next
  persistShareUiState()
}

function firstDocFromTree(nodes: ShareNode[]): ShareNode | null {
  for (const node of nodes) {
    if (node.node_type === 'doc') return node
    if (node.node_type === 'dir' && Array.isArray(node.children)) {
      const found = firstDocFromTree(node.children)
      if (found) return found
    }
  }
  return null
}

async function loadShareInfo() {
  try {
    const data = (await request.get(`/s/${route.params.token}`)) as { share: ShareInfo }
    shareInfo.value = data.share

    if (isShareExpired(data.share.expires_at)) {
      clearCachedPassword(shareToken)
      state.value = 'expired'
      return
    }

    if (data.share.has_password) {
      const cachedPassword = readCachedPassword(shareToken)
      if (cachedPassword) {
        password.value = cachedPassword
        await loadContent(cachedPassword)
        if (state.value === 'doc' || state.value === 'dir') return
      }
      state.value = 'password'
      return
    }

    clearCachedPassword(shareToken)
    await loadContent()
  } catch (err: any) {
    clearCachedPassword(shareToken)
    state.value = err.response?.status === 410 ? 'expired' : 'notfound'
  }
}

async function loadContent(pw?: string) {
  if (isShareExpired(shareInfo.value?.expires_at)) {
    clearCachedPassword(shareToken)
    state.value = 'expired'
    return
  }

  try {
    const headers: Record<string, string> = {}
    if (pw) headers['X-Share-Password'] = pw

    const data = (await request.get(`/s/${route.params.token}/content`, { headers })) as { node: ShareNode }
    content.value = data.node

    if (data.node.node_type === 'dir') {
      syncExpandedDirState(data.node.children || [])
      selectedDoc.value = firstDocFromTree(data.node.children || [])
      state.value = 'dir'
    } else {
      state.value = 'doc'
    }
  } catch (err: any) {
    if (err.response?.status === 410) {
      clearCachedPassword(shareToken)
      state.value = 'expired'
    } else if (err.response?.status === 401) {
      clearCachedPassword(shareToken)
      state.value = 'password'
    } else {
      state.value = 'notfound'
    }
  }
}

async function verifyPassword() {
  if (!password.value.trim()) return

  verifying.value = true
  wrongPassword.value = false

  try {
    await request.post(`/s/${route.params.token}/verify`, { password: password.value })
    await loadContent(password.value)
    if (state.value === 'doc' || state.value === 'dir') {
      cachePassword(shareToken, password.value)
    }
  } catch (err: any) {
    if (err.response?.status === 401) wrongPassword.value = true
    else if (err.response?.status === 410) {
      clearCachedPassword(shareToken)
      state.value = 'expired'
    } else {
      state.value = 'notfound'
    }
  } finally {
    verifying.value = false
  }
}

onMounted(async () => {
  loadShareUiState()
  await loadShareInfo()
})

const MarkFlowLogo = defineComponent({
  render() {
    return h('svg', { width: 32, height: 32, viewBox: '0 0 40 40', fill: 'none' }, [
      h('rect', { width: 40, height: 40, rx: 10, fill: '#2ea043' }),
      h('path', {
        d: 'M8 28 L8 12 L16 20 L24 12 L24 28',
        stroke: 'white',
        'stroke-width': '2.5',
        'stroke-linecap': 'round',
        'stroke-linejoin': 'round',
        fill: 'none',
      }),
      h('path', {
        d: 'M26 16 L32 16 M26 20 L30 20 M26 24 L32 24',
        stroke: 'white',
        'stroke-width': '2',
        'stroke-linecap': 'round',
      }),
    ])
  },
})

const ShareHeader = defineComponent({
  props: {
    docName: String,
    shareInfo: Object as () => ShareInfo | null,
    sidebarOpen: { type: Boolean, default: true },
    showSidebarToggle: { type: Boolean, default: false },
  },
  emits: ['toggle-sidebar'],
  setup(props, { emit }) {
    function formatExpiry(dt: string) {
      const d = new Date(dt)
      const yyyy = d.getFullYear()
      const MM = `${d.getMonth() + 1}`.padStart(2, '0')
      const dd = `${d.getDate()}`.padStart(2, '0')
      const hh = `${d.getHours()}`.padStart(2, '0')
      const mm = `${d.getMinutes()}`.padStart(2, '0')
      return `${yyyy}/${MM}/${dd} ${hh}:${mm} 到期`
    }

    return () =>
      h('header', { class: 'share-header' }, [
        h('div', { class: 'sh-left' }, [
          h('a', { href: '/', class: 'sh-brand' }, [h(MarkFlowLogo), h('span', 'MarkFlow')]),
          props.docName && h('span', { class: 'sh-doc-name' }, props.docName),
        ]),
        h('div', { class: 'sh-right' }, [
          props.showSidebarToggle && h(
            'button',
            {
              class: 'sh-toggle',
              title: props.sidebarOpen ? '收起目录' : '展开目录',
              onClick: () => emit('toggle-sidebar'),
            },
            props.sidebarOpen ? '收起目录' : '展开目录'
          ),
          h('span', { class: 'sh-badge' }, '只读'),
          props.shareInfo?.expires_at && h('span', { class: 'sh-badge muted' }, formatExpiry(props.shareInfo.expires_at)),
        ]),
      ])
  },
})

const DirTreeNav = defineComponent({
  name: 'DirTreeNav',
  props: {
    nodes: { type: Array as () => ShareNode[], default: () => [] },
    selectedId: String,
    expandedIds: { type: Object as () => Set<string>, required: true },
  },
  emits: ['select', 'toggle-dir'],
  setup(props, { emit }) {
    function renderNodes(nodes: ShareNode[]): any {
      return nodes.map((node) => {
        if (node.node_type === 'dir') {
          const expanded = props.expandedIds.has(node.id)
          return h('div', { class: 'nav-dir' }, [
            h(
              'div',
              {
                class: ['nav-dir-label', { 'nav-dir-label--expanded': expanded }],
                onClick: () => emit('toggle-dir', node.id),
              },
              [h('span', { class: ['nav-icon', { 'nav-icon-expanded': expanded }] }, '▸'), h('span', { class: 'nav-dir-name' }, node.name)]
            ),
            expanded && node.children?.length ? h('div', { class: 'nav-children' }, renderNodes(node.children)) : null,
          ])
        }

        return h(
          'div',
          {
            class: ['nav-doc', { 'nav-doc--active': props.selectedId === node.id }],
            onClick: () => emit('select', node),
          },
          [h('span', { class: 'nav-icon' }, '•'), h('span', { class: 'nav-doc-name' }, node.name)]
        )
      })
    }

    return () => h('div', { class: 'dir-tree-nav' }, renderNodes(props.nodes))
  },
})
</script>

<style scoped>
.share-page {
  min-height: 100vh;
  background: var(--bg);
  color: var(--text);
  font-family: var(--font);
}

.share-center {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 100vh;
  gap: 16px;
  padding: 40px 24px;
  text-align: center;
}

.spin-icon {
  font-size: 32px;
  color: var(--text3);
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.state-icon {
  width: 64px;
  height: 64px;
  border-radius: 50%;
  background: color-mix(in srgb, var(--bg3) 84%, transparent);
  border: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 28px;
  color: var(--text3);
}

.state-icon.danger {
  color: var(--red);
  border-color: rgba(248, 81, 73, 0.3);
  background: rgba(248, 81, 73, 0.06);
}

.share-center h2 {
  font-size: 20px;
  font-weight: 600;
  color: var(--text);
}

.share-center p {
  font-size: 14px;
  color: var(--text2);
  max-width: 380px;
}

.pw-card {
  width: 100%;
  max-width: 390px;
  background: var(--bg2);
  border: 1px solid var(--border);
  border-radius: var(--r-lg);
  padding: 36px 32px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
}

.pw-logo {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 16px;
  font-weight: 700;
  color: var(--text);
}

.pw-icon {
  font-size: 40px;
  color: var(--yellow);
  margin-top: 4px;
}

.pw-card h2 {
  font-size: 18px;
  font-weight: 600;
  color: var(--text);
}

.pw-doc-name {
  font-size: 13px;
  color: var(--text2);
  margin-top: -6px;
}

.pw-error {
  font-size: 13px;
  color: var(--red);
}

:deep(.share-header) {
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
  padding: 0 20px;
  position: sticky;
  top: 0;
  z-index: 100;
}

:deep(.sh-left) {
  display: flex;
  align-items: center;
  gap: 16px;
  min-width: 0;
}

:deep(.sh-brand) {
  display: flex;
  align-items: center;
  gap: 8px;
  text-decoration: none;
  font-weight: 700;
  font-size: 15px;
  color: var(--text);
}

:deep(.sh-doc-name) {
  font-size: 14px;
  font-weight: 500;
  color: var(--text);
  padding-left: 16px;
  border-left: 1px solid var(--border);
  max-width: 420px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

:deep(.sh-right) {
  display: flex;
  align-items: center;
  gap: 8px;
}

:deep(.sh-toggle) {
  border: 1px solid var(--border);
  background: var(--bg3);
  color: var(--text2);
  border-radius: 8px;
  font-size: 12px;
  line-height: 1;
  padding: 6px 10px;
  cursor: pointer;
  transition: all 0.12s;
}

:deep(.sh-toggle:hover) {
  color: var(--text);
  border-color: var(--blue);
  background: color-mix(in srgb, var(--blue) 12%, var(--bg3));
}

:deep(.sh-badge) {
  font-size: 11px;
  padding: 3px 8px;
  background: var(--bg3);
  border: 1px solid var(--border);
  border-radius: 20px;
  color: var(--text2);
}

:deep(.sh-badge.muted) {
  color: var(--text3);
}

.share-doc-layout {
  height: 100vh;
  overflow: hidden;
}

.share-doc-body {
  height: calc(100vh - var(--header-height));
  display: grid;
  grid-template-columns: 260px 1fr;
  transition: grid-template-columns 0.18s ease;
}

.share-doc-body.is-sidebar-collapsed {
  grid-template-columns: 0 1fr;
}

.share-doc-sidebar {
  background: linear-gradient(
    180deg,
    color-mix(in srgb, var(--bg2) 94%, transparent) 0%,
    color-mix(in srgb, var(--bg2) 84%, var(--bg)) 100%
  );
  border-right: 1px solid var(--border);
  padding: 12px 10px;
  min-width: 0;
  overflow: hidden;
  overflow-y: auto;
}

.doc-side-card {
  background: var(--bg3);
  border: 1px solid var(--border);
  border-radius: var(--r-md);
  padding: 10px 10px 9px;
}

.doc-side-name {
  color: var(--text);
  font-size: 13px;
  font-weight: 600;
  line-height: 1.3;
  word-break: break-word;
}

.share-doc-content {
  overflow-y: auto;
  padding: 18px 20px 28px;
}

.preview-shell {
  width: 100%;
}

:deep(.share-preview.v-md-editor-preview) {
  border: none;
  box-shadow: none;
  background: transparent;
}

:deep(.share-preview .github-markdown-body) {
  padding: 4px 2px 18px !important;
  background: transparent !important;
  color: var(--text) !important;
  max-width: none !important;
  margin: 0 !important;
}

:deep(.share-preview .github-markdown-body pre) {
  border-radius: 10px;
  border: 1px solid var(--border);
  padding: 12px 14px !important;
  background: var(--bg2);
}

:deep(.share-preview .github-markdown-body code) {
  font-family: var(--mono);
}

.share-dir-layout {
  height: 100vh;
  overflow: hidden;
}

.share-dir-body {
  height: calc(100vh - var(--header-height));
  display: grid;
  grid-template-columns: 260px 1fr;
  transition: grid-template-columns 0.18s ease;
}

.share-dir-body.is-sidebar-collapsed {
  grid-template-columns: 0 1fr;
}

.share-dir-sidebar {
  background: linear-gradient(
    180deg,
    color-mix(in srgb, var(--bg2) 94%, transparent) 0%,
    color-mix(in srgb, var(--bg2) 84%, var(--bg)) 100%
  );
  border-right: 1px solid var(--border);
  padding: 12px 10px;
  min-width: 0;
  overflow: hidden;
  overflow-y: auto;
}

.share-doc-body.is-sidebar-collapsed .share-doc-sidebar,
.share-dir-body.is-sidebar-collapsed .share-dir-sidebar {
  padding: 0;
  border-right-color: transparent;
}

.sidebar-title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.7px;
  text-transform: uppercase;
  color: var(--text3);
  padding: 0 6px;
  margin-bottom: 10px;
}

.share-dir-content {
  overflow-y: auto;
  padding: 18px 20px 28px;
}

.dir-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  gap: 12px;
  color: var(--text3);
  font-size: 14px;
}

.dir-placeholder .el-icon {
  font-size: 46px;
  opacity: 0.45;
}

:deep(.nav-dir) {
  margin-bottom: 2px;
}

:deep(.nav-dir-label) {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text2);
  border-radius: var(--r-sm);
  cursor: pointer;
  transition: background 0.12s;
}

:deep(.nav-dir-label:hover) {
  background: color-mix(in srgb, var(--blue) 10%, transparent);
}

:deep(.nav-dir-name) {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

:deep(.nav-children) {
  padding-left: 12px;
}

:deep(.nav-doc) {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  border-radius: var(--r-sm);
  font-size: 13px;
  cursor: pointer;
  color: var(--text);
  transition: background 0.12s;
  margin-bottom: 1px;
}

:deep(.nav-doc:hover) {
  background: color-mix(in srgb, var(--blue) 10%, transparent);
}

:deep(.nav-doc--active) {
  background: var(--green-dim) !important;
  color: var(--green3);
}

:deep(.nav-icon) {
  font-size: 12px;
  flex-shrink: 0;
  color: var(--text3);
  transition: transform 0.12s;
}

:deep(.nav-icon.nav-icon-expanded) {
  transform: rotate(90deg);
}

:deep(.nav-doc-name) {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@media (max-width: 880px) {
  .share-doc-body {
    grid-template-columns: 1fr;
    grid-template-rows: auto 1fr;
  }

  .share-doc-sidebar {
    max-height: 26vh;
    border-right: none;
    border-bottom: 1px solid var(--border);
  }

  .share-dir-body {
    grid-template-columns: 1fr;
    grid-template-rows: auto 1fr;
  }

  .share-dir-sidebar {
    max-height: 36vh;
    border-right: none;
    border-bottom: 1px solid var(--border);
  }

  :deep(.sh-doc-name) {
    display: none;
  }
}
</style>
