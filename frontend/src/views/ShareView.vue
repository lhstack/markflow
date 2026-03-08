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
          <div v-if="shareSidebarOpen" class="share-tree-shell">
            <div class="sidebar-title">文档树</div>
            <div class="share-tree-scroll">
              <ShareTreeNav
                :nodes="content ? [content] : []"
                :selected-id="content?.id"
                :expanded-ids="readonlyExpandedIds"
                @select="selectDoc"
              />
            </div>
          </div>
        </aside>
        <main
          ref="shareContentRef"
          class="share-doc-content"
        >
          <article ref="previewArticleRef" class="share-paper">
            <div class="share-paper-head">
              <div class="share-paper-kicker">SHARED NOTE</div>
              <h1>{{ content?.name || '未命名文档' }}</h1>
            </div>
            <VditorPreview
              :key="currentPreviewIdentity"
              :render-key="currentPreviewIdentity"
              :clear-before-render="true"
              :markdown="content?.content || ''"
              @rendered="handlePreviewRendered"
            />
          </article>
          <aside
            v-if="showFloatingToc"
            class="floating-toc"
            :class="{ collapsed: !tocPanelOpen }"
          >
            <button
              v-if="!tocPanelOpen"
              class="floating-toc-tab"
              title="展开目录"
              aria-label="展开目录"
              @click="toggleTocPanel"
            >
              <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                <path d="M2 3.25c0-.414.336-.75.75-.75h10.5a.75.75 0 0 1 0 1.5H2.75A.75.75 0 0 1 2 3.25Zm0 4c0-.414.336-.75.75-.75h7.5a.75.75 0 0 1 0 1.5h-7.5A.75.75 0 0 1 2 7.25Zm0 4c0-.414.336-.75.75-.75h5.5a.75.75 0 0 1 0 1.5h-5.5A.75.75 0 0 1 2 11.25Zm10.47-2.78a.75.75 0 0 1 1.06 0l1.72 1.72a.75.75 0 0 1 0 1.06l-1.72 1.72a.75.75 0 1 1-1.06-1.06l.44-.44H10.5a.75.75 0 0 1 0-1.5h2.41l-.44-.44a.75.75 0 0 1 0-1.06Z"/>
              </svg>
              <span>目录</span>
            </button>
            <template v-else>
              <div class="floating-toc-head">
                <div>
                  <div class="floating-toc-kicker">Outline</div>
                  <div class="floating-toc-title">文档目录</div>
                </div>
                <button
                  class="floating-toc-toggle"
                  title="收起目录"
                  aria-label="收起目录"
                  @click="toggleTocPanel"
                >
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                    <path d="M9.78 3.22a.75.75 0 0 1 0 1.06L6.06 8l3.72 3.72a.75.75 0 1 1-1.06 1.06L4.47 8.53a.75.75 0 0 1 0-1.06l4.25-4.25a.75.75 0 0 1 1.06 0Z"/>
                  </svg>
                </button>
              </div>
              <div v-if="tocItems.length" class="floating-toc-list">
                <button
                  v-for="item in tocItems"
                  :key="item.id"
                  class="floating-toc-item"
                  :class="[`level-${item.level}`, { active: item.id === activeTocId || item.id === selectedTocId }]"
                  @click="scrollToHeading(item)"
                >
                  <span class="floating-toc-bullet"></span>
                  <span class="floating-toc-text">{{ item.text }}</span>
                </button>
              </div>
              <div v-else class="floating-toc-empty">
                {{ tocReady ? '当前预览内容里没有可识别的标题' : '正在识别目录...' }}
              </div>
            </template>
          </aside>
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
          <div v-if="shareSidebarOpen" class="share-tree-shell">
            <div class="sidebar-title">文档树</div>
            <div class="share-tree-search">
              <svg class="share-tree-search-icon" width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M10.68 11.74a6 6 0 0 1-7.922-8.982 6 6 0 0 1 8.982 7.922l3.04 3.04a.749.749 0 0 1-.326 1.275.749.749 0 0 1-.734-.215ZM11.5 7a4.499 4.499 0 1 0-8.997 0A4.499 4.499 0 0 0 11.5 7Z"/></svg>
              <input v-model="shareSearchQuery" class="share-tree-input" placeholder="搜索文档..." />
              <button v-if="shareSearchQuery" class="share-tree-clear" @click="shareSearchQuery = ''">×</button>
            </div>
            <div class="share-tree-scroll">
              <ShareTreeNav
                v-if="filteredShareNodes.length"
                :nodes="filteredShareNodes"
                :selected-id="selectedDoc?.id"
                :expanded-ids="expandedDirIds"
                :force-expand="Boolean(shareSearchQuery)"
                @select="selectDoc"
                @toggle-dir="toggleDirExpand"
              />
              <div v-else class="share-tree-empty">
                <svg width="28" height="28" viewBox="0 0 16 16" fill="var(--text3)"><path d="M10.68 11.74a6 6 0 0 1-7.922-8.982 6 6 0 0 1 8.982 7.922l3.04 3.04a.749.749 0 0 1-.326 1.275.749.749 0 0 1-.734-.215ZM11.5 7a4.499 4.499 0 1 0-8.997 0A4.499 4.499 0 0 0 11.5 7Z"/></svg>
                <span>未找到 "{{ shareSearchQuery }}"</span>
              </div>
            </div>
          </div>
        </aside>
        <main
          ref="shareContentRef"
          class="share-dir-content"
        >
          <article v-if="selectedDoc" ref="previewArticleRef" class="share-paper">
            <div class="share-paper-head">
              <div class="share-paper-kicker">SHARED NOTE</div>
              <h1>{{ selectedDoc.name }}</h1>
            </div>
            <VditorPreview
              :key="currentPreviewIdentity"
              :render-key="currentPreviewIdentity"
              :clear-before-render="true"
              :markdown="selectedDoc.content || ''"
              @rendered="handlePreviewRendered"
            />
          </article>
          <div v-else class="dir-placeholder">
            <el-icon><Folder /></el-icon>
            <p>从左侧选择文档查看</p>
          </div>
          <aside
            v-if="showFloatingToc"
            class="floating-toc"
            :class="{ collapsed: !tocPanelOpen }"
          >
            <button
              v-if="!tocPanelOpen"
              class="floating-toc-tab"
              title="展开目录"
              aria-label="展开目录"
              @click="toggleTocPanel"
            >
              <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                <path d="M2 3.25c0-.414.336-.75.75-.75h10.5a.75.75 0 0 1 0 1.5H2.75A.75.75 0 0 1 2 3.25Zm0 4c0-.414.336-.75.75-.75h7.5a.75.75 0 0 1 0 1.5h-7.5A.75.75 0 0 1 2 7.25Zm0 4c0-.414.336-.75.75-.75h5.5a.75.75 0 0 1 0 1.5h-5.5A.75.75 0 0 1 2 11.25Zm10.47-2.78a.75.75 0 0 1 1.06 0l1.72 1.72a.75.75 0 0 1 0 1.06l-1.72 1.72a.75.75 0 1 1-1.06-1.06l.44-.44H10.5a.75.75 0 0 1 0-1.5h2.41l-.44-.44a.75.75 0 0 1 0-1.06Z"/>
              </svg>
              <span>目录</span>
            </button>
            <template v-else>
              <div class="floating-toc-head">
                <div>
                  <div class="floating-toc-kicker">Outline</div>
                  <div class="floating-toc-title">文档目录</div>
                </div>
                <button
                  class="floating-toc-toggle"
                  title="收起目录"
                  aria-label="收起目录"
                  @click="toggleTocPanel"
                >
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                    <path d="M9.78 3.22a.75.75 0 0 1 0 1.06L6.06 8l3.72 3.72a.75.75 0 1 1-1.06 1.06L4.47 8.53a.75.75 0 0 1 0-1.06l4.25-4.25a.75.75 0 0 1 1.06 0Z"/>
                  </svg>
                </button>
              </div>
              <div v-if="tocItems.length" class="floating-toc-list">
                <button
                  v-for="item in tocItems"
                  :key="item.id"
                  class="floating-toc-item"
                  :class="[`level-${item.level}`, { active: item.id === activeTocId || item.id === selectedTocId }]"
                  @click="scrollToHeading(item)"
                >
                  <span class="floating-toc-bullet"></span>
                  <span class="floating-toc-text">{{ item.text }}</span>
                </button>
              </div>
              <div v-else class="floating-toc-empty">
                {{ tocReady ? '当前预览内容里没有可识别的标题' : '正在识别目录...' }}
              </div>
            </template>
          </aside>
        </main>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, defineComponent, h, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import request from '@/utils/request'
import { useRoute } from 'vue-router'
import VditorPreview from '@/components/VditorPreview.vue'
import TreeNodeGlyph from '@/components/TreeNodeGlyph.vue'

const route = useRoute()

type ShareState = 'loading' | 'password' | 'doc' | 'dir' | 'expired' | 'notfound'

interface ShareInfo {
  doc_name: string
  has_password: boolean
  expires_at?: string | null
}

interface ShareNode {
  id: number
  name: string
  node_type: 'doc' | 'dir'
  content?: string
  children?: ShareNode[]
}

interface TocItem {
  id: string
  text: string
  level: number
  order: number
}

interface PreviewRenderedPayload {
  key?: string | number
  headings: Array<{ text: string; level: number }>
}

const state = ref<ShareState>('loading')
const shareInfo = ref<ShareInfo | null>(null)
const content = ref<ShareNode | null>(null)
const password = ref('')
const verifying = ref(false)
const wrongPassword = ref(false)
const selectedDoc = ref<ShareNode | null>(null)
const persistedSelectedDocId = ref<number | null>(null)
const shareToken = String(route.params.token || '')
const shareSidebarOpen = ref(true)
const shareSearchQuery = ref('')
const expandedDirIds = ref<Set<number>>(new Set())
const shareDirStateInitialized = ref(false)
const hasDirExpansionPreference = ref(false)
const readonlyExpandedIds = new Set<number>()
const shareContentRef = ref<HTMLElement | null>(null)
const previewArticleRef = ref<HTMLElement | null>(null)
const tocPanelOpen = ref(true)
const tocItems = ref<TocItem[]>([])
const activeTocId = ref('')
const selectedTocId = ref('')
const tocReady = ref(false)
let tocRetryTimer: number | null = null

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
  persistedSelectedDocId.value = null
  try {
    const raw = localStorage.getItem(getShareUiKey(shareToken))
    if (!raw) return
    const parsed = JSON.parse(raw) as {
      sidebar_open?: boolean
      expanded_dir_ids?: number[]
      selected_doc_id?: number | string | null
      toc_open?: boolean
    }
    if (typeof parsed.sidebar_open === 'boolean') {
      shareSidebarOpen.value = parsed.sidebar_open
    }
    if (Array.isArray(parsed.expanded_dir_ids)) {
      expandedDirIds.value = new Set(parsed.expanded_dir_ids.filter((id) => typeof id === 'number'))
      hasDirExpansionPreference.value = true
    }
    if (
      typeof parsed.selected_doc_id === 'number' && Number.isInteger(parsed.selected_doc_id)
    ) {
      persistedSelectedDocId.value = parsed.selected_doc_id
    } else if (
      typeof parsed.selected_doc_id === 'string' &&
      parsed.selected_doc_id.trim() &&
      Number.isInteger(Number(parsed.selected_doc_id))
    ) {
      persistedSelectedDocId.value = Number(parsed.selected_doc_id)
    }
    if (typeof parsed.toc_open === 'boolean') {
      tocPanelOpen.value = parsed.toc_open
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
      selected_doc_id: selectedDoc.value?.id ?? persistedSelectedDocId.value ?? null,
      toc_open: tocPanelOpen.value,
    })
  )
}

function toggleShareSidebar() {
  shareSidebarOpen.value = !shareSidebarOpen.value
  persistShareUiState()
}

function slugifyHeading(text: string) {
  return text
    .trim()
    .toLowerCase()
    .replace(/[\s\W-]+/g, '-')
    .replace(/^-+|-+$/g, '') || 'section'
}

function updateActiveTocByScroll() {
  const container = shareContentRef.value
  const headings = getPreviewHeadings()
  if (!container || !headings.length || !tocItems.value.length) {
    activeTocId.value = ''
    return
  }

  const containerTop = container.getBoundingClientRect().top
  let current = headings[0]
  for (const heading of headings) {
    if (heading.getBoundingClientRect().top - containerTop <= 104) current = heading
    else break
  }
  activeTocId.value = current.id
  selectedTocId.value = current.id
}

function refreshTocFromPreview() {
  const headings = getPreviewHeadings()
  if (!headings.length) {
    tocItems.value = []
    activeTocId.value = ''
    selectedTocId.value = ''
    return false
  }

  const counts = new Map<string, number>()
  tocItems.value = headings.map((heading, index) => {
    const text = heading.textContent?.trim() || `标题 ${index + 1}`
    const level = Number(heading.tagName.slice(1)) || 1
    const baseId = slugifyHeading(text)
    const count = counts.get(baseId) || 0
    counts.set(baseId, count + 1)
    const nextId = count === 0 ? baseId : `${baseId}-${count + 1}`
    heading.id = nextId
    return { id: nextId, text, level, order: index }
  })
  if (!selectedTocId.value || !tocItems.value.some((item) => item.id === selectedTocId.value)) {
    selectedTocId.value = tocItems.value[0]?.id || ''
  }
  updateActiveTocByScroll()
  return true
}

function clearTocRetryTimer() {
  if (tocRetryTimer !== null) {
    window.clearTimeout(tocRetryTimer)
    tocRetryTimer = null
  }
}

function scheduleTocRefresh(attempt = 0) {
  clearTocRetryTimer()
  tocRetryTimer = window.setTimeout(() => {
    const found = refreshTocFromPreview()
    if (found || attempt >= 8) {
      tocReady.value = true
      return
    }
    scheduleTocRefresh(attempt + 1)
  }, 80)
}

function getPreviewHeadings() {
  const article = previewArticleRef.value
  if (!article) return [] as HTMLElement[]
  return Array.from(
    article.querySelectorAll<HTMLElement>('.vditor-reset h1, .vditor-reset h2, .vditor-reset h3, .vditor-reset h4, .vditor-reset h5, .vditor-reset h6')
  )
}

function scrollToHeading(item: TocItem) {
  const headings = getPreviewHeadings()
  const target = headings[item.order]
  if (!target) return
  selectedTocId.value = item.id
  target.scrollIntoView({
    behavior: 'smooth',
    block: 'start',
    inline: 'nearest',
  })
  activeTocId.value = item.id
}

function handlePreviewRendered(payload?: PreviewRenderedPayload) {
  void nextTick().then(() => {
    if (payload?.key !== undefined && payload.key !== currentPreviewIdentity.value) {
      return
    }
    if (payload?.headings) {
      const headings = getPreviewHeadings()
      const counts = new Map<string, number>()
      tocItems.value = headings.map((heading, index) => {
        const source = payload.headings[index]
        const text = source?.text || heading.textContent?.trim() || `标题 ${index + 1}`
        const level = source?.level || Number(heading.tagName.slice(1)) || 1
        const baseId = slugifyHeading(text)
        const count = counts.get(baseId) || 0
        counts.set(baseId, count + 1)
        const nextId = count === 0 ? baseId : `${baseId}-${count + 1}`
        heading.id = nextId
        return { id: nextId, text, level, order: index }
      })
      if (!selectedTocId.value || !tocItems.value.some((item) => item.id === selectedTocId.value)) {
        selectedTocId.value = tocItems.value[0]?.id || ''
      }
      tocReady.value = true
      clearTocRetryTimer()
      updateActiveTocByScroll()
      if (tocItems.value.length) return
    }
    const found = refreshTocFromPreview()
    if (found) {
      tocReady.value = true
      clearTocRetryTimer()
      return
    }
    tocReady.value = false
    scheduleTocRefresh()
  })
}

function toggleTocPanel() {
  tocPanelOpen.value = !tocPanelOpen.value
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
  if (doc.node_type === 'doc') {
    selectedDoc.value = { ...doc, content: selectedDoc.value?.id === doc.id ? selectedDoc.value.content : undefined }
    persistedSelectedDocId.value = doc.id
    persistShareUiState()
    void loadSharedDocContent(doc.id, password.value || undefined)
  }
}

function collectDirIds(nodes: ShareNode[], set: Set<number>) {
  for (const node of nodes) {
    if (node.node_type === 'dir') {
      set.add(node.id)
      if (node.children?.length) collectDirIds(node.children, set)
    }
  }
}

function syncExpandedDirState(nodes: ShareNode[]) {
  const currentDirIds = new Set<number>()
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

function toggleDirExpand(dirId: number) {
  const next = new Set(expandedDirIds.value)
  if (next.has(dirId)) next.delete(dirId)
  else next.add(dirId)
  expandedDirIds.value = next
  persistShareUiState()
}

function filterShareNodes(nodes: ShareNode[], q: string): ShareNode[] {
  if (!q) return nodes
  const lq = q.toLowerCase()

  return nodes.flatMap((node) => {
    const match = node.name.toLowerCase().includes(lq)
    const children = filterShareNodes(node.children || [], q)

    if (match) return [{ ...node, children: node.children || [] }]
    if (children.length) return [{ ...node, children }]
    return []
  })
}

function findShareNodeById(nodes: ShareNode[], id: number): ShareNode | null {
  for (const node of nodes) {
    if (node.id === id) return node
    const found = findShareNodeById(node.children || [], id)
    if (found) return found
  }
  return null
}

const filteredShareNodes = computed(() =>
  filterShareNodes(content.value?.children || [], shareSearchQuery.value.trim())
)

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
    const headers = buildShareHeaders(pw)

    const data = (await request.get(`/s/${route.params.token}/content`, { headers })) as { node: ShareNode }
    content.value = data.node

    if (data.node.node_type === 'dir') {
      state.value = 'dir'
      syncExpandedDirState(data.node.children || [])
      const preservedSelectedId = persistedSelectedDocId.value ?? selectedDoc.value?.id ?? null
      const nextSelected =
        (preservedSelectedId ? findShareNodeById(data.node.children || [], preservedSelectedId) : null) ||
        firstDocFromTree(data.node.children || [])
      selectedDoc.value = nextSelected ? { ...nextSelected, content: undefined } : null
      persistedSelectedDocId.value = nextSelected?.id ?? null
      persistShareUiState()
      if (nextSelected?.id) {
        await loadSharedDocContent(nextSelected.id, pw)
      }
    } else {
      selectedDoc.value = data.node
      persistedSelectedDocId.value = data.node.id
      persistShareUiState()
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

function buildShareHeaders(pw?: string) {
  const headers: Record<string, string> = {}
  if (pw) headers['X-Share-Password'] = pw
  return headers
}

async function loadSharedDocContent(docId: number, pw?: string) {
  try {
    const data = (await request.get(`/s/${route.params.token}/nodes/${docId}/content`, {
      headers: buildShareHeaders(pw),
    })) as { node: ShareNode }
    selectedDoc.value = data.node
    persistedSelectedDocId.value = data.node.id
    persistShareUiState()
  } catch (err: any) {
    if (err.response?.status === 410) {
      clearCachedPassword(shareToken)
      state.value = 'expired'
    } else if (err.response?.status === 401) {
      clearCachedPassword(shareToken)
      state.value = 'password'
    } else if (err.response?.status === 404) {
      selectedDoc.value = null
      persistedSelectedDocId.value = null
      persistShareUiState()
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
  shareContentRef.value?.addEventListener('scroll', updateActiveTocByScroll, { passive: true })
  await loadShareInfo()
})

onBeforeUnmount(() => {
  shareContentRef.value?.removeEventListener('scroll', updateActiveTocByScroll)
  clearTocRetryTimer()
})

watch(shareContentRef, (next, prev) => {
  prev?.removeEventListener('scroll', updateActiveTocByScroll)
  next?.addEventListener('scroll', updateActiveTocByScroll, { passive: true })
})

watch(() => state.value, () => {
  if (state.value !== 'doc' && state.value !== 'dir') {
    tocItems.value = []
    activeTocId.value = ''
    selectedTocId.value = ''
    tocReady.value = false
    clearTocRetryTimer()
  }
})

watch(() => selectedDoc.value?.id, () => {
  tocItems.value = []
  activeTocId.value = ''
  selectedTocId.value = ''
  tocReady.value = false
  clearTocRetryTimer()
})

const currentPreviewMarkdown = computed(() => {
  if (state.value === 'dir') return selectedDoc.value?.content || ''
  if (state.value === 'doc') return content.value?.content || ''
  return ''
})

const currentPreviewIdentity = computed(() => {
  if (state.value === 'dir') return `dir:${selectedDoc.value?.id ?? 'none'}`
  if (state.value === 'doc') return `doc:${content.value?.id ?? 'none'}`
  return 'none'
})

watch(currentPreviewMarkdown, () => {
  tocItems.value = []
  activeTocId.value = ''
  selectedTocId.value = ''
  tocReady.value = false
  clearTocRetryTimer()
})

const showFloatingToc = computed(() =>
  (state.value === 'doc' || state.value === 'dir') && currentPreviewMarkdown.value.trim().length > 0
)

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
          props.showSidebarToggle && h(
            'button',
            {
              class: 'sh-sidebar-toggle',
              title: props.sidebarOpen ? '收起目录' : '展开目录',
              'aria-label': props.sidebarOpen ? '收起目录' : '展开目录',
              onClick: () => emit('toggle-sidebar'),
            },
            h('svg', { width: '16', height: '16', viewBox: '0 0 16 16', fill: 'currentColor', 'aria-hidden': 'true' }, [
              h('path', {
                d: 'M1 2.75A.75.75 0 0 1 1.75 2h12.5a.75.75 0 0 1 0 1.5H1.75A.75.75 0 0 1 1 2.75Zm0 5A.75.75 0 0 1 1.75 7h12.5a.75.75 0 0 1 0 1.5H1.75A.75.75 0 0 1 1 7.75ZM1.75 12h12.5a.75.75 0 0 1 0 1.5H1.75a.75.75 0 0 1 0-1.5Z',
              }),
            ])
          ),
          h('a', { href: '/', class: 'sh-brand' }, [h(MarkFlowLogo), h('span', 'MarkFlow')]),
          props.docName && h('span', { class: 'sh-doc-name' }, props.docName),
        ]),
        h('div', { class: 'sh-right' }, [
          h('span', { class: 'sh-badge' }, '只读'),
          props.shareInfo?.expires_at && h('span', { class: 'sh-badge muted' }, formatExpiry(props.shareInfo.expires_at)),
        ]),
      ])
  },
})

const ShareTreeNav = defineComponent({
  name: 'ShareTreeNav',
  props: {
    nodes: { type: Array as () => ShareNode[], default: () => [] },
    selectedId: Number,
    expandedIds: { type: Object as () => Set<number>, required: true },
    forceExpand: { type: Boolean, default: false },
    depth: { type: Number, default: 0 },
  },
  emits: ['select', 'toggle-dir'],
  setup(props, { emit }) {
    function renderNodes(nodes: ShareNode[], depth = 0): any {
      return nodes.map((node) => {
        if (node.node_type === 'dir') {
          const expanded = props.expandedIds.has(node.id)
          return h('div', { class: 'nav-dir' }, [
            h(
              'div',
              {
                class: ['share-tree-node-row', 'is-dir', { 'is-selected': props.selectedId === node.id }],
                style: { paddingLeft: `${8 + depth * 14}px` },
                onClick: () => emit('toggle-dir', node.id),
              },
              [
                h(
                  'span',
                  { class: ['share-tree-expand', { expanded }] },
                  h('svg', { width: '10', height: '10', viewBox: '0 0 16 16', fill: 'currentColor' }, [
                    h('path', {
                      d: 'M6.22 3.22a.75.75 0 0 1 1.06 0l4.25 4.25a.75.75 0 0 1 0 1.06l-4.25 4.25a.751.751 0 0 1-1.042-.018.751.751 0 0 1-.018-1.042L9.94 8 6.22 4.28a.75.75 0 0 1 0-1.06Z',
                    }),
                  ])
                ),
                h('span', { class: 'share-tree-node-icon' }, [
                  h(TreeNodeGlyph, { kind: 'dir', expanded, active: props.selectedId === node.id, size: 18 }),
                ]),
                h('span', { class: 'share-tree-node-label' }, node.name),
              ]
            ),
            (props.forceExpand || expanded) && node.children?.length
              ? h('div', { class: 'share-tree-children' }, renderNodes(node.children, depth + 1))
              : null,
          ])
        }

        return h(
          'div',
          {
            class: ['share-tree-node-row', { 'is-selected': props.selectedId === node.id }],
            style: { paddingLeft: `${22 + depth * 14}px` },
            onClick: () => emit('select', node),
          },
          [
            h('span', { class: 'share-tree-node-icon' }, [
              h(TreeNodeGlyph, {
                kind: 'doc',
                expanded: false,
                active: props.selectedId === node.id,
                size: 18,
              }),
            ]),
            h('span', { class: 'share-tree-node-label' }, node.name),
          ]
        )
      })
    }

    return () => h('div', { class: 'share-tree-nav' }, renderNodes(props.nodes, props.depth))
  },
})
</script>

<style scoped>
.share-page {
  min-height: 100vh;
  background:
    radial-gradient(circle at top left, rgba(198, 220, 181, 0.22), transparent 28%),
    radial-gradient(circle at top right, rgba(240, 230, 190, 0.26), transparent 24%),
    linear-gradient(180deg, #f7f9f3 0%, #eef3e7 100%);
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
  background: rgba(255, 255, 255, 0.76);
  border: 1px solid rgba(68, 82, 54, 0.1);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 28px;
  color: var(--text3);
  box-shadow: 0 18px 36px rgba(53, 64, 40, 0.08);
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
  background: rgba(255, 255, 255, 0.84);
  border: 1px solid rgba(58, 72, 46, 0.08);
  border-radius: 26px;
  padding: 36px 32px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  box-shadow: 0 24px 50px rgba(52, 64, 38, 0.12);
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
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.84), rgba(248, 250, 242, 0.72));
  border-bottom: 1px solid rgba(36, 48, 27, 0.08);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 20px;
  position: sticky;
  top: 0;
  z-index: 100;
  backdrop-filter: blur(14px);
}

:deep(.sh-left) {
  display: flex;
  align-items: center;
  gap: 16px;
  min-width: 0;
}

:deep(.sh-sidebar-toggle) {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 38px;
  height: 38px;
  border: 1px solid rgba(72, 94, 58, 0.12);
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.84);
  color: var(--text2);
  cursor: pointer;
  flex-shrink: 0;
  transition:
    border-color 0.12s ease,
    background 0.12s ease,
    color 0.12s ease,
    transform 0.12s ease;
}

:deep(.sh-sidebar-toggle:hover) {
  color: var(--text);
  border-color: rgba(102, 143, 72, 0.32);
  background: rgba(237, 244, 228, 0.96);
}

:deep(.sh-sidebar-toggle:active) {
  transform: translateY(1px);
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
  border-left: 1px solid rgba(57, 72, 40, 0.1);
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

:deep(.sh-badge) {
  font-size: 11px;
  padding: 5px 10px;
  background: rgba(255, 255, 255, 0.72);
  border: 1px solid rgba(72, 94, 58, 0.1);
  border-radius: 20px;
  color: var(--text2);
}

:deep(.sh-badge.muted) {
  color: var(--text3);
}

.share-doc-layout,
.share-dir-layout {
  height: 100vh;
  overflow: hidden;
}

.share-doc-body,
.share-dir-body {
  height: calc(100vh - var(--header-height));
  display: grid;
  grid-template-columns: 260px 1fr;
  transition: grid-template-columns 0.18s ease;
}

.share-doc-body.is-sidebar-collapsed,
.share-dir-body.is-sidebar-collapsed {
  grid-template-columns: 0 1fr;
}

.share-doc-sidebar,
.share-dir-sidebar {
  background: linear-gradient(180deg, rgba(251, 253, 247, 0.8), rgba(242, 246, 236, 0.88));
  border-right: 1px solid rgba(42, 56, 31, 0.08);
  padding: 10px 8px 12px;
  min-width: 0;
  overflow: hidden;
  overflow-y: auto;
}

.share-doc-body.is-sidebar-collapsed .share-doc-sidebar,
.share-dir-body.is-sidebar-collapsed .share-dir-sidebar {
  padding: 0;
  border-right-color: transparent;
  pointer-events: none;
}

.sidebar-title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.7px;
  text-transform: uppercase;
  color: var(--text3);
  padding: 0 8px;
  margin-bottom: 12px;
}

.share-tree-shell {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.share-tree-search {
  position: relative;
  padding: 0 4px 8px;
  flex-shrink: 0;
}

.share-tree-search-icon {
  position: absolute;
  left: 14px;
  top: 50%;
  transform: translateY(-60%);
  color: var(--text3);
  pointer-events: none;
}

.share-tree-input {
  width: 100%;
  padding: 5px 24px 5px 26px;
  background: rgba(240, 244, 235, 0.88);
  border: 1px solid transparent;
  border-radius: 10px;
  color: var(--text2);
  font-size: 12px;
  outline: none;
  transition:
    border-color 0.15s ease,
    background 0.15s ease,
    color 0.15s ease;
}

.share-tree-input:focus {
  border-color: rgba(111, 154, 79, 0.22);
  background: rgba(248, 251, 243, 0.96);
  color: var(--text);
}

.share-tree-input::placeholder {
  color: var(--text3);
}

.share-tree-clear {
  position: absolute;
  right: 10px;
  top: 50%;
  transform: translateY(-60%);
  border: none;
  background: transparent;
  color: var(--text3);
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
  padding: 0 2px;
}

.share-tree-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 0 2px 6px;
}

.share-tree-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 36px 14px;
  color: var(--text3);
  font-size: 12px;
  text-align: center;
}

.share-doc-content,
.share-dir-content {
  position: relative;
  overflow-y: auto;
  padding: 24px 24px 40px;
}

.share-paper {
  width: min(1440px, 100%);
  margin: 0 auto;
  padding: 30px 40px 42px;
  border-radius: 30px;
  border: 1px solid rgba(48, 62, 35, 0.08);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.92), rgba(252, 253, 248, 0.94));
  box-shadow:
    0 24px 60px rgba(49, 64, 39, 0.12),
    inset 0 1px 0 rgba(255, 255, 255, 0.66);
}

.share-paper-head {
  padding-bottom: 18px;
  margin-bottom: 22px;
  border-bottom: 1px solid rgba(53, 67, 40, 0.08);
}

.share-paper-kicker {
  font-size: 11px;
  letter-spacing: 0.18em;
  color: #83907d;
}

.share-paper-head h1 {
  margin: 10px 0 0;
  font-size: 2rem;
  line-height: 1.1;
  color: #171b16;
  letter-spacing: -0.02em;
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

.floating-toc {
  position: fixed;
  top: calc(var(--header-height) + 22px);
  right: 18px;
  z-index: 90;
  width: 270px;
  max-height: calc(100vh - var(--header-height) - 40px);
  border: 1px solid rgba(50, 64, 38, 0.08);
  border-radius: 22px;
  background: rgba(252, 253, 248, 0.92);
  backdrop-filter: blur(16px);
  box-shadow:
    0 20px 44px rgba(49, 64, 39, 0.14),
    inset 0 1px 0 rgba(255, 255, 255, 0.7);
  overflow: hidden;
}

.floating-toc.collapsed {
  width: auto;
  border: none;
  background: transparent;
  box-shadow: none;
  backdrop-filter: none;
  overflow: visible;
}

.floating-toc-tab {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 42px;
  padding: 0 12px;
  border: 1px solid rgba(82, 110, 60, 0.16);
  border-radius: 999px;
  background: rgba(250, 252, 246, 0.96);
  color: #4b6135;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.04em;
  cursor: pointer;
  box-shadow: 0 16px 34px rgba(49, 64, 39, 0.16);
}

.floating-toc-tab:hover,
.floating-toc-toggle:hover {
  background: rgba(237, 244, 228, 0.98);
  color: #2f4a1a;
}

.floating-toc-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 16px 16px 12px;
  border-bottom: 1px solid rgba(53, 67, 40, 0.08);
}

.floating-toc-kicker {
  font-size: 10px;
  line-height: 1;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: #8b9784;
}

.floating-toc-title {
  margin-top: 6px;
  font-size: 15px;
  font-weight: 700;
  color: #1f2819;
}

.floating-toc-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: 1px solid rgba(82, 110, 60, 0.14);
  border-radius: 10px;
  background: rgba(247, 250, 242, 0.88);
  color: #5c6d4f;
  cursor: pointer;
}

.floating-toc-list {
  max-height: calc(100vh - var(--header-height) - 126px);
  overflow-y: auto;
  padding: 10px 10px 14px;
}

.floating-toc-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  min-height: 34px;
  padding: 7px 10px;
  border: none;
  border-radius: 12px;
  background: transparent;
  color: #53614a;
  text-align: left;
  cursor: pointer;
  transition:
    background-color 0.15s ease,
    color 0.15s ease,
    transform 0.15s ease;
}

.floating-toc-item:hover {
  background: rgba(111, 154, 79, 0.1);
  color: #2f4a1a;
}

.floating-toc-item.active {
  background: rgba(111, 154, 79, 0.16);
  color: #274412;
}

.floating-toc-item.level-2 {
  padding-left: 22px;
}

.floating-toc-item.level-3 {
  padding-left: 34px;
}

.floating-toc-item.level-4,
.floating-toc-item.level-5,
.floating-toc-item.level-6 {
  padding-left: 46px;
}

.floating-toc-bullet {
  width: 6px;
  height: 6px;
  border-radius: 999px;
  background: rgba(113, 145, 86, 0.44);
  flex-shrink: 0;
}

.floating-toc-item.active .floating-toc-bullet {
  background: #5d8c37;
  box-shadow: 0 0 0 4px rgba(111, 154, 79, 0.14);
}

.floating-toc-text {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  line-height: 1.35;
}

.floating-toc-empty {
  padding: 18px 16px 20px;
  font-size: 12px;
  color: var(--text3);
}

:deep(.share-tree-nav) {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

:deep(.nav-dir) {
  margin-bottom: 2px;
}

:deep(.share-tree-node-row) {
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 32px;
  padding-right: 8px;
  border-radius: 10px;
  cursor: pointer;
  color: var(--text);
  transition:
    background 0.12s ease,
    color 0.12s ease;
}

:deep(.share-tree-node-row:hover) {
  background: rgba(115, 153, 82, 0.12);
}

:deep(.share-tree-node-row.is-selected) {
  background: rgba(111, 154, 79, 0.18);
}

:deep(.share-tree-node-row.is-selected .share-tree-node-label) {
  color: #35501f;
  font-weight: 600;
}

:deep(.share-tree-expand) {
  width: 14px;
  height: 14px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--text3);
  flex-shrink: 0;
  transition: transform 0.12s ease;
}

:deep(.share-tree-expand.expanded) {
  transform: rotate(90deg);
}

:deep(.share-tree-node-icon) {
  width: 20px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

:deep(.share-tree-node-label) {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
  font-size: 13px;
}

:deep(.share-tree-children) {
  padding-left: 12px;
}

@media (max-width: 880px) {
  .share-doc-body,
  .share-dir-body {
    grid-template-columns: 1fr;
    grid-template-rows: auto 1fr;
  }

  .share-doc-body.is-sidebar-collapsed,
  .share-dir-body.is-sidebar-collapsed {
    grid-template-columns: 1fr;
    grid-template-rows: 0 1fr;
  }

  .share-doc-sidebar {
    max-height: 26vh;
    border-right: none;
    border-bottom: 1px solid rgba(42, 56, 31, 0.08);
  }

  .share-dir-sidebar {
    max-height: 36vh;
    border-right: none;
    border-bottom: 1px solid rgba(42, 56, 31, 0.08);
  }

  .share-doc-content,
  .share-dir-content {
    padding: 14px 14px 24px;
  }

  .share-paper {
    width: 100%;
    padding: 20px 18px 28px;
    border-radius: 22px;
  }

  .floating-toc {
    display: none;
  }

  :deep(.sh-doc-name) {
    display: none;
  }

  :deep(.sh-right) {
    gap: 6px;
  }
}
</style>
