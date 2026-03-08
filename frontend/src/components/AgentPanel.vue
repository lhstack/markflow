<template>
  <div
    v-if="mounted"
    class="agent-panel"
    :class="{ collapsed }"
    :style="panelStyle"
  >
    <button
      v-if="collapsed"
      class="agent-fab"
      @mousedown.stop="startDrag"
      @click="handleFabClick"
    >
      AI 助手
    </button>

    <div v-else class="agent-shell">
      <header class="agent-header" @mousedown="startDrag">
        <div class="agent-title">
          <div class="agent-kicker">ASSISTANT</div>
          <div class="agent-heading">智能体</div>
        </div>
        <div class="agent-header-actions">
          <button class="header-btn" title="会话管理" @click.stop="sessionDrawerOpen = !sessionDrawerOpen">会话</button>
          <button class="header-btn" title="供应商管理" @click.stop="showProviderDialog = true">供应商</button>
          <button class="header-btn" title="清空当前会话" @click.stop="clearCurrentSession">清空</button>
          <button class="header-btn" title="收起" @click.stop="toggleCollapse(true)">收起</button>
        </div>
      </header>

      <div class="agent-body">
        <aside v-if="sessionDrawerOpen" class="agent-sessions">
          <div class="session-toolbar">
            <button class="session-create" @click="createSession">新会话</button>
          </div>
          <div class="session-list">
            <button
              v-for="session in sessions"
              :key="session.id"
              class="session-item"
              :class="{ active: session.id === currentSessionId }"
              @click="selectSession(session.id)"
            >
              <span class="session-item-title">{{ session.title }}</span>
              <span class="session-item-meta">{{ formatSessionTime(session.updatedAt) }}</span>
              <span class="session-item-delete" @click.stop="deleteSession(session.id)">删除</span>
            </button>
          </div>
        </aside>

        <section class="agent-main">
          <div class="agent-feed">
            <div ref="messagesRef" class="agent-messages">
              <template v-if="currentSession?.messages.length">
                <div
                  v-for="message in currentSession.messages"
                  :key="message.id"
                  class="agent-message"
                  :class="message.role"
                >
                  <div class="message-role">{{ message.role === 'user' ? '你' : 'AI' }}</div>
                  <pre class="message-content">{{ message.content || (streaming && message.role === 'assistant' ? '...' : '') }}</pre>
                </div>
              </template>
              <div v-else class="agent-empty">
                <div class="agent-empty-title">可以直接开始对话或写文档</div>
                <div class="agent-empty-desc">直接描述你的目标即可，模型会自己判断是答复问题、继续完善当前文档，还是整体重写文档。</div>
              </div>
            </div>
          </div>

          <div class="agent-composer">
            <div class="agent-mode-row">
              <div class="agent-mode-tip">{{ modeTip }}</div>
            </div>

            <textarea
              v-model="prompt"
              class="agent-textarea"
              :placeholder="textareaPlaceholder"
              :disabled="streaming"
              @keydown.enter.exact.prevent="sendMessage"
            />

            <div class="agent-actions">
              <button class="ghost-action" :disabled="streaming" @click="createSession">新会话</button>
              <button class="primary-action" :disabled="streaming" @click="sendMessage">
                {{ streaming ? '生成中...' : '发送' }}
              </button>
            </div>
          </div>
        </section>
      </div>
    </div>

    <el-dialog v-model="showProviderDialog" title="供应商配置" width="460px" append-to-body destroy-on-close>
      <div class="provider-form">
        <el-input v-model="providerForm.baseUrl" placeholder="Base URL，例如 https://api.openai.com/v1" />
        <el-input v-model="providerForm.model" placeholder="模型，例如 gpt-4o-mini" />
        <el-input v-model="providerForm.apiKey" type="password" show-password placeholder="API Key" />
        <div class="provider-hint">当前是本地配置，仅保存在浏览器里，用于快速验证交互效果。</div>
      </div>
      <template #footer>
        <el-button @click="showProviderDialog = false">取消</el-button>
        <el-button type="primary" @click="saveProvider">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'

import {
  dispatchAgentWriterChunk,
  dispatchAgentWriterComplete,
  dispatchAgentWriterStart,
  getAgentEditorSnapshot,
  type AgentWriterMode,
} from '@/utils/agentWriter'

type PageScope = 'overview' | 'editor' | 'dir'
type DocType = 'doc' | 'dir' | null
type SessionRole = 'user' | 'assistant'
type StreamAction = 'chat' | 'append' | 'replace'
type RouteKind = 'overview' | 'project' | 'doc'

interface AgentRouteTarget {
  kind: RouteKind
  name?: string
}

interface AgentMessage {
  id: string
  role: SessionRole
  content: string
}

interface AgentSession {
  id: string
  title: string
  messages: AgentMessage[]
  createdAt: number
  updatedAt: number
}

interface ProviderForm {
  baseUrl: string
  model: string
  apiKey: string
}

const props = defineProps<{
  pageScope: PageScope
  projectId: number | null
  projectName: string
  docId: number | null
  docName: string
  docType: DocType
  docContent: string
  projectCatalog: string
  currentNodeCatalog: string
}>()

const emit = defineEmits<{
  navigate: [target: AgentRouteTarget]
}>()

const PANEL_STATE_KEY = 'markflow.agent.panel.state'
const SESSIONS_KEY = 'markflow.agent.sessions'
const PROVIDER_KEY = 'markflow.agent.provider'
const VIEWPORT_MARGIN = 24
const FAB_WIDTH = 96
const FAB_HEIGHT = 48
const PANEL_WIDTH = 420
const PANEL_HEIGHT = 620

const mounted = ref(false)
const collapsed = ref(false)
const sessionDrawerOpen = ref(false)
const showProviderDialog = ref(false)
const streaming = ref(false)
const prompt = ref('')
const panelX = ref(0)
const panelY = ref(88)
const currentSessionId = ref<string>('')
const sessions = ref<AgentSession[]>([])
const messagesRef = ref<HTMLElement | null>(null)
const providerForm = ref<ProviderForm>({
  baseUrl: 'https://api.openai.com/v1',
  model: 'gpt-4o-mini',
  apiKey: '',
})

let dragOffsetX = 0
let dragOffsetY = 0
let dragging = false
let didDrag = false

const panelStyle = computed(() => ({
  transform: `translate(${panelX.value}px, ${panelY.value}px)`,
}))

const currentSession = computed(() => sessions.value.find((session) => session.id === currentSessionId.value) || null)

const modeTip = computed(() => {
  if (props.docType === 'doc') return '直接说你的目标即可，模型会自己决定是答复、续写还是重写当前文档。'
  return '当前不在具体文档页时，模型只进行对话回答，不会直接写入文档。'
})

const textareaPlaceholder = computed(() => {
  if (props.docType === 'doc') return '例如：继续完善这篇文档的部署说明，并补充安装、验证和常见问题'
  return '输入你的问题，按 Enter 发送'
})

function genId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`
}

function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw ? JSON.parse(raw) as T : fallback
  } catch {
    return fallback
  }
}

function maxPanelXFor(nextCollapsed: boolean) {
  const width = nextCollapsed ? FAB_WIDTH : PANEL_WIDTH
  return Math.max(VIEWPORT_MARGIN, window.innerWidth - width - VIEWPORT_MARGIN)
}

function maxPanelYFor(nextCollapsed: boolean) {
  const estimatedHeight = nextCollapsed
    ? FAB_HEIGHT
    : Math.min(PANEL_HEIGHT, Math.max(420, window.innerHeight - 112))
  return Math.max(VIEWPORT_MARGIN, window.innerHeight - estimatedHeight - VIEWPORT_MARGIN)
}

function clampPanelPosition(nextX: number, nextY: number, nextCollapsed: boolean) {
  return {
    x: Math.max(VIEWPORT_MARGIN, Math.min(maxPanelXFor(nextCollapsed), nextX)),
    y: Math.max(VIEWPORT_MARGIN, Math.min(maxPanelYFor(nextCollapsed), nextY)),
  }
}

function defaultPanelState(nextCollapsed = true) {
  const { x, y } = clampPanelPosition(
    window.innerWidth - (nextCollapsed ? FAB_WIDTH : PANEL_WIDTH) - VIEWPORT_MARGIN,
    window.innerHeight - (nextCollapsed ? FAB_HEIGHT : PANEL_HEIGHT) - VIEWPORT_MARGIN,
    nextCollapsed,
  )
  return {
    collapsed: nextCollapsed,
    panelX: x,
    panelY: y,
    currentSessionId: '',
    sessionDrawerOpen: false,
  }
}

function panelSize(nextCollapsed: boolean) {
  return {
    width: nextCollapsed ? FAB_WIDTH : PANEL_WIDTH,
    height: nextCollapsed ? FAB_HEIGHT : Math.min(PANEL_HEIGHT, Math.max(420, window.innerHeight - 112)),
  }
}

function toggleCollapse(nextCollapsed: boolean) {
  if (collapsed.value === nextCollapsed) return
  const currentSize = panelSize(collapsed.value)
  const anchorRight = window.innerWidth - (panelX.value + currentSize.width)
  const anchorBottom = window.innerHeight - (panelY.value + currentSize.height)
  const nextSize = panelSize(nextCollapsed)
  const nextPosition = clampPanelPosition(
    window.innerWidth - anchorRight - nextSize.width,
    window.innerHeight - anchorBottom - nextSize.height,
    nextCollapsed,
  )
  panelX.value = nextPosition.x
  panelY.value = nextPosition.y
  collapsed.value = nextCollapsed
}

function persistPanelState() {
  localStorage.setItem(
    PANEL_STATE_KEY,
    JSON.stringify({
      collapsed: collapsed.value,
      panelX: panelX.value,
      panelY: panelY.value,
      currentSessionId: currentSessionId.value,
      sessionDrawerOpen: sessionDrawerOpen.value,
    }),
  )
}

function persistSessions() {
  localStorage.setItem(SESSIONS_KEY, JSON.stringify(sessions.value))
}

function persistProvider() {
  localStorage.setItem(PROVIDER_KEY, JSON.stringify(providerForm.value))
}

function ensureSession(): AgentSession {
  let session = currentSession.value
  if (session) return session

  session = {
    id: genId(),
    title: '新会话',
    messages: [],
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }
  sessions.value.unshift(session)
  currentSessionId.value = session.id
  persistSessions()
  persistPanelState()
  return session
}

function createSession() {
  const session: AgentSession = {
    id: genId(),
    title: '新会话',
    messages: [],
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }
  sessions.value.unshift(session)
  currentSessionId.value = session.id
  sessionDrawerOpen.value = false
  prompt.value = ''
  persistSessions()
  persistPanelState()
}

function selectSession(sessionId: string) {
  currentSessionId.value = sessionId
  sessionDrawerOpen.value = false
  persistPanelState()
}

function deleteSession(sessionId: string) {
  sessions.value = sessions.value.filter((session) => session.id !== sessionId)
  if (currentSessionId.value === sessionId) {
    currentSessionId.value = sessions.value[0]?.id || ''
  }
  if (!sessions.value.length) {
    createSession()
    return
  }
  persistSessions()
  persistPanelState()
}

function clearCurrentSession() {
  const session = ensureSession()
  session.messages = []
  session.title = '新会话'
  session.updatedAt = Date.now()
  sessions.value = [...sessions.value]
  persistSessions()
}

function updateSessionMessage(sessionId: string, messageId: string, content: string) {
  const session = sessions.value.find((item) => item.id === sessionId)
  if (!session) return
  const message = session.messages.find((item) => item.id === messageId)
  if (!message) return
  message.content = content
  session.updatedAt = Date.now()
  sessions.value = [...sessions.value]
  persistSessions()
  void nextTick().then(() => {
    const el = messagesRef.value
    if (el) el.scrollTop = el.scrollHeight
  })
}

function formatSessionTime(timestamp: number) {
  const date = new Date(timestamp)
  const MM = `${date.getMonth() + 1}`.padStart(2, '0')
  const dd = `${date.getDate()}`.padStart(2, '0')
  const hh = `${date.getHours()}`.padStart(2, '0')
  const mm = `${date.getMinutes()}`.padStart(2, '0')
  return `${MM}/${dd} ${hh}:${mm}`
}

function startDrag(event: MouseEvent) {
  dragging = true
  didDrag = false
  dragOffsetX = event.clientX - panelX.value
  dragOffsetY = event.clientY - panelY.value
  window.addEventListener('mousemove', onDrag)
  window.addEventListener('mouseup', stopDrag)
}

function onDrag(event: MouseEvent) {
  if (!dragging) return
  didDrag = true
  const nextX = event.clientX - dragOffsetX
  const nextY = event.clientY - dragOffsetY
  const clamped = clampPanelPosition(nextX, nextY, collapsed.value)
  panelX.value = clamped.x
  panelY.value = clamped.y
}

function stopDrag() {
  dragging = false
  window.removeEventListener('mousemove', onDrag)
  window.removeEventListener('mouseup', stopDrag)
  persistPanelState()
}

function handleFabClick() {
  if (didDrag) {
    didDrag = false
    return
  }
  toggleCollapse(false)
}

function saveProvider() {
  if (!providerForm.value.apiKey.trim()) {
    ElMessage.warning('请填写 API Key')
    return
  }
  if (!providerForm.value.model.trim()) {
    ElMessage.warning('请填写模型名称')
    return
  }
  persistProvider()
  showProviderDialog.value = false
  ElMessage.success('供应商配置已保存')
}

function currentDocContent() {
  if (!props.docId) return ''
  return getAgentEditorSnapshot(props.docId) ?? props.docContent ?? ''
}

function buildRequestBody(messages: AgentMessage[]) {
  return {
    provider: {
      api_key: providerForm.value.apiKey.trim(),
      base_url: providerForm.value.baseUrl.trim(),
      model: providerForm.value.model.trim(),
    },
    messages: messages.map((message) => ({
      role: message.role,
      content: message.content,
    })),
    mode: 'auto',
    context: {
      page_scope: props.pageScope,
      project_name: props.projectName || null,
      doc_id: props.docId,
      doc_name: props.docName || null,
      doc_content: currentDocContent(),
      project_catalog: props.projectCatalog || null,
      current_node_catalog: props.currentNodeCatalog || null,
    },
  }
}

function parseRouteMarker(raw: string): AgentRouteTarget | null {
  const match = raw.match(/^\[\[ROUTE:(overview|project|doc)(?::([\s\S]*?))?\]\]$/)
  if (!match) return null

  const kind = match[1] as RouteKind
  const name = match[2]?.trim()
  if ((kind === 'project' || kind === 'doc') && !name) return null

  return {
    kind,
    name,
  }
}

function parseSseBlock(block: string) {
  let eventName = 'message'
  const dataParts: string[] = []

  for (const line of block.split('\n')) {
    if (line.startsWith('event:')) {
      eventName = line.slice(6).trim()
      continue
    }
    if (line.startsWith('data:')) {
      dataParts.push(line.slice(5).trim())
    }
  }

  if (!dataParts.length) return null
  const raw = dataParts.join('\n')
  try {
    return {
      event: eventName,
      data: JSON.parse(raw),
    }
  } catch {
    return {
      event: eventName,
      data: { value: raw },
    }
  }
}

async function sendMessage() {
  const text = prompt.value.trim()
  if (!text) return
  if (!providerForm.value.apiKey.trim()) {
    ElMessage.warning('请先配置供应商 API Key')
    showProviderDialog.value = true
    return
  }
  const session = ensureSession()
  const userMessage: AgentMessage = { id: genId(), role: 'user', content: text }
  const assistantMessage: AgentMessage = { id: genId(), role: 'assistant', content: '' }
  session.messages.push(userMessage, assistantMessage)
  session.updatedAt = Date.now()
  if (session.title === '新会话') {
    session.title = text.slice(0, 18)
  }
  sessions.value = [...sessions.value]
  persistSessions()
  prompt.value = ''
  streaming.value = true

  try {
    const token = localStorage.getItem('token')
    const response = await fetch('/api/agent/chat/stream', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify(buildRequestBody(session.messages)),
    })

    if (!response.ok) {
      const data = await response.json().catch(() => ({}))
      throw new Error(data.error || '智能体请求失败')
    }

    const reader = response.body?.getReader()
    if (!reader) throw new Error('流式响应不可用')

    const decoder = new TextDecoder('utf-8')
    let buffer = ''
    let routeAction: StreamAction | null = null
    let prefixProbe = ''
    let pendingRoute: AgentRouteTarget | null = null
    let writerStarted = false

    const routeChunk = (rawChunk: string) => {
      if (!rawChunk) return

      if (routeAction === 'chat') {
        const nextContent = `${assistantMessage.content}${rawChunk}`
        assistantMessage.content = nextContent
        updateSessionMessage(session.id, assistantMessage.id, nextContent)
        return
      }

      if (!writerStarted && props.docId) {
        const writerMode: AgentWriterMode = routeAction === 'replace' ? 'replace' : 'append'
        dispatchAgentWriterStart({ docId: props.docId, mode: writerMode })
        writerStarted = true
      }

      const nextContent = `${assistantMessage.content}${rawChunk}`
      assistantMessage.content = nextContent
      updateSessionMessage(session.id, assistantMessage.id, nextContent)
      if (props.docId) {
        dispatchAgentWriterChunk({ docId: props.docId, chunk: rawChunk })
      }
    }

    const handleAssistantChunk = (rawChunk: string) => {
      if (!rawChunk) return
      if (routeAction) {
        routeChunk(rawChunk)
        return
      }

      prefixProbe += rawChunk
      if (!prefixProbe.startsWith('[[')) {
        routeAction = 'chat'
        const flushed = prefixProbe
        prefixProbe = ''
        routeChunk(flushed)
        return
      }

      while (prefixProbe.startsWith('[[')) {
        const markerEnd = prefixProbe.indexOf(']]')
        if (markerEnd === -1) {
          if (prefixProbe.length > 120) {
            routeAction = 'chat'
            const flushed = prefixProbe
            prefixProbe = ''
            routeChunk(flushed)
          }
          return
        }

        const marker = prefixProbe.slice(0, markerEnd + 2)
        const rest = prefixProbe.slice(markerEnd + 2)

        if (marker.startsWith('[[ROUTE:')) {
          const target = parseRouteMarker(marker)
          if (target) {
            pendingRoute = target
            prefixProbe = rest.replace(/^\s+/, '')
            continue
          }
          routeAction = 'chat'
          const flushed = prefixProbe
          prefixProbe = ''
          routeChunk(flushed)
          return
        }

        if (marker.startsWith('[[ACTION:')) {
          prefixProbe = ''
          routeAction = marker === '[[ACTION:append]]'
            ? 'append'
            : marker === '[[ACTION:replace]]'
              ? 'replace'
              : 'chat'

          if (routeAction !== 'chat' && props.docType !== 'doc') {
            routeAction = 'chat'
          }

          if (pendingRoute) {
            emit('navigate', pendingRoute)
            pendingRoute = null
          }

          routeChunk(rest.replace(/^\s+/, ''))
          return
        }

        routeAction = 'chat'
        const flushed = prefixProbe
        prefixProbe = ''
        routeChunk(flushed)
        return
      }

      if (pendingRoute) {
        emit('navigate', pendingRoute)
        pendingRoute = null
      }

      routeAction = 'chat'
      const flushed = prefixProbe
      prefixProbe = ''
      routeChunk(flushed)
    }

    while (true) {
      const { value, done } = await reader.read()
      if (done) break

      buffer += decoder.decode(value, { stream: true })
      const blocks = buffer.split('\n\n')
      buffer = blocks.pop() || ''

      for (const block of blocks) {
        const parsed = parseSseBlock(block)
        if (!parsed) continue

        if (parsed.event === 'message.delta') {
          handleAssistantChunk(parsed.data.content || '')
        } else if (parsed.event === 'message.completed') {
          const completedContent = parsed.data.content || ''
          if (!assistantMessage.content && completedContent) {
            handleAssistantChunk(completedContent)
          }
          if (!routeAction && prefixProbe.trim()) {
            handleAssistantChunk(prefixProbe)
          }
          assistantMessage.content = assistantMessage.content || completedContent
          updateSessionMessage(session.id, assistantMessage.id, assistantMessage.content)
          if (routeAction && routeAction !== 'chat' && props.docId) {
            dispatchAgentWriterComplete({ docId: props.docId })
          }
        } else if (parsed.event === 'error') {
          throw new Error(parsed.data.error || '智能体流式请求失败')
        }
      }
    }
  } catch (error: any) {
    ElMessage.error(error.message || '智能体请求失败')
  } finally {
    streaming.value = false
    sessions.value = [...sessions.value]
    persistSessions()
  }
}

watch(
  () => currentSession.value?.messages.length,
  async () => {
    await nextTick()
    const el = messagesRef.value
    if (el) el.scrollTop = el.scrollHeight
  },
)

watch([collapsed, panelX, panelY, currentSessionId, sessionDrawerOpen], () => {
  const clamped = clampPanelPosition(panelX.value, panelY.value, collapsed.value)
  if (clamped.x !== panelX.value) panelX.value = clamped.x
  if (clamped.y !== panelY.value) panelY.value = clamped.y
  persistPanelState()
})

onMounted(() => {
  mounted.value = true

  const panelState = loadJson(PANEL_STATE_KEY, defaultPanelState(true))
  const savedSessions = loadJson<AgentSession[]>(SESSIONS_KEY, [])
  const savedProvider = loadJson<ProviderForm>(PROVIDER_KEY, providerForm.value)

  sessions.value = savedSessions
  providerForm.value = savedProvider
  collapsed.value = Boolean(panelState.collapsed)
  const initialPosition = clampPanelPosition(
    Number.isFinite(panelState.panelX) ? panelState.panelX : defaultPanelState(collapsed.value).panelX,
    Number.isFinite(panelState.panelY) ? panelState.panelY : defaultPanelState(collapsed.value).panelY,
    collapsed.value,
  )
  panelX.value = initialPosition.x
  panelY.value = initialPosition.y
  sessionDrawerOpen.value = Boolean(panelState.sessionDrawerOpen)

  if (!sessions.value.length) {
    createSession()
  } else {
    currentSessionId.value = panelState.currentSessionId && sessions.value.some((session) => session.id === panelState.currentSessionId)
      ? panelState.currentSessionId
      : sessions.value[0].id
  }
})

onUnmounted(() => {
  stopDrag()
})
</script>

<style scoped>
.agent-panel {
  position: fixed;
  top: 0;
  left: 0;
  z-index: 1800;
  width: 420px;
  max-width: calc(100vw - 24px);
}

.agent-panel.collapsed {
  width: auto;
}

.agent-fab {
  border: none;
  border-radius: 999px;
  padding: 12px 18px;
  background: linear-gradient(135deg, #6f9a4f, #537535);
  color: #f8fff1;
  font-size: 13px;
  font-weight: 700;
  box-shadow: 0 18px 36px rgba(83, 117, 53, 0.24);
  cursor: pointer;
}

.agent-shell {
  display: flex;
  flex-direction: column;
  height: min(620px, calc(100vh - 112px));
  min-height: 420px;
  max-height: calc(100vh - 112px);
  border-radius: 22px;
  overflow: hidden;
  border: 1px solid rgba(122, 147, 91, 0.22);
  background:
    radial-gradient(circle at top right, rgba(215, 227, 196, 0.72), transparent 28%),
    linear-gradient(180deg, rgba(251, 252, 247, 0.96), rgba(246, 248, 239, 0.94));
  backdrop-filter: blur(16px);
  box-shadow: 0 32px 72px rgba(67, 86, 50, 0.16);
}

.agent-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 14px 16px;
  border-bottom: 1px solid rgba(122, 147, 91, 0.16);
  cursor: move;
}

.agent-title {
  min-width: 0;
}

.agent-kicker {
  font-size: 10px;
  letter-spacing: 0.18em;
  color: #7e8977;
}

.agent-heading {
  margin-top: 4px;
  font-size: 20px;
  font-weight: 800;
  color: #1c241a;
}

.agent-header-actions {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.header-btn,
.session-create,
.ghost-action,
.primary-action {
  height: 34px;
  border-radius: 12px;
  padding: 0 12px;
  font-size: 12px;
  font-weight: 700;
  cursor: pointer;
}

.header-btn,
.session-create,
.ghost-action {
  border: 1px solid rgba(122, 147, 91, 0.24);
  background: rgba(255, 255, 255, 0.78);
  color: #607057;
}

.primary-action {
  border: none;
  background: linear-gradient(135deg, #6f9a4f, #537535);
  color: #f8fff1;
}

.agent-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.agent-sessions {
  width: 152px;
  flex-shrink: 0;
  border-right: 1px solid rgba(122, 147, 91, 0.14);
  background: rgba(240, 244, 232, 0.78);
  display: flex;
  flex-direction: column;
}

.session-toolbar {
  padding: 12px;
  border-bottom: 1px solid rgba(122, 147, 91, 0.14);
}

.session-create {
  width: 100%;
}

.session-list {
  flex: 1;
  overflow: auto;
  scrollbar-gutter: stable;
  scrollbar-width: thin;
  scrollbar-color: rgba(111, 154, 79, 0.55) rgba(122, 147, 91, 0.12);
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.session-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
  align-items: flex-start;
  padding: 10px;
  border-radius: 14px;
  border: 1px solid rgba(122, 147, 91, 0.12);
  background: rgba(255, 255, 255, 0.82);
  color: #51604a;
  cursor: pointer;
  text-align: left;
}

.session-item.active {
  border-color: rgba(111, 154, 79, 0.34);
  background: rgba(232, 240, 220, 0.92);
}

.session-item-title {
  width: 100%;
  font-size: 12px;
  font-weight: 700;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.session-item-meta,
.session-item-delete {
  font-size: 11px;
}

.session-item-delete {
  color: #b45f52;
}

.agent-main {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.agent-feed {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.agent-messages {
  flex: 1;
  min-height: 0;
  overflow: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
  scrollbar-width: thin;
  scrollbar-color: rgba(111, 154, 79, 0.58) rgba(122, 147, 91, 0.12);
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.session-list::-webkit-scrollbar,
.agent-messages::-webkit-scrollbar {
  width: 10px;
}

.session-list::-webkit-scrollbar-track,
.agent-messages::-webkit-scrollbar-track {
  background: rgba(122, 147, 91, 0.12);
  border-radius: 999px;
}

.session-list::-webkit-scrollbar-thumb,
.agent-messages::-webkit-scrollbar-thumb {
  background: rgba(111, 154, 79, 0.58);
  border-radius: 999px;
  border: 2px solid rgba(246, 248, 239, 0.9);
}

.session-list::-webkit-scrollbar-thumb:hover,
.agent-messages::-webkit-scrollbar-thumb:hover {
  background: rgba(83, 117, 53, 0.72);
}

.agent-empty {
  margin: auto 0;
  padding: 18px;
  border-radius: 18px;
  background: rgba(255, 255, 255, 0.72);
  border: 1px dashed rgba(122, 147, 91, 0.24);
  color: #708067;
}

.agent-empty-title {
  font-size: 14px;
  font-weight: 700;
  color: #24311f;
}

.agent-empty-desc {
  margin-top: 8px;
  font-size: 12px;
  line-height: 1.6;
}

.agent-message {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.agent-message.user {
  align-items: flex-end;
}

.agent-message.assistant {
  align-items: flex-start;
}

.message-role {
  font-size: 11px;
  font-weight: 700;
  color: #7a866f;
}

.message-content {
  margin: 0;
  width: 100%;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: inherit;
  font-size: 13px;
  line-height: 1.7;
  padding: 12px 14px;
  border-radius: 16px;
  background: rgba(255, 255, 255, 0.84);
  border: 1px solid rgba(122, 147, 91, 0.14);
  color: #24311f;
}

.agent-message.user .message-content {
  background: rgba(232, 240, 220, 0.9);
}

.agent-composer {
  flex-shrink: 0;
  border-top: 1px solid rgba(122, 147, 91, 0.12);
  padding: 14px 16px 16px;
  background: rgba(251, 252, 247, 0.94);
}

.agent-mode-row {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 12px;
}

.agent-mode-tip {
  font-size: 12px;
  color: #7b8771;
  line-height: 1.7;
}

.agent-textarea {
  width: 100%;
  margin-top: 12px;
  min-height: 104px;
  resize: none;
  border-radius: 16px;
  border: 1px solid rgba(122, 147, 91, 0.18);
  background: rgba(255, 255, 255, 0.92);
  color: #1d2719;
  padding: 12px 14px;
  font: inherit;
  line-height: 1.7;
}

.agent-textarea:focus {
  outline: none;
  border-color: rgba(111, 154, 79, 0.46);
  box-shadow: 0 0 0 3px rgba(111, 154, 79, 0.12);
}

.agent-actions {
  display: flex;
  justify-content: space-between;
  gap: 10px;
  margin-top: 12px;
}

.provider-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.provider-hint {
  font-size: 12px;
  color: #708067;
  line-height: 1.6;
}

@media (max-width: 1200px) {
  .agent-panel {
    width: min(400px, calc(100vw - 24px));
  }
}

@media (max-width: 720px) {
  .agent-panel {
    width: calc(100vw - 24px);
  }

  .agent-shell {
    height: min(540px, calc(100vh - 84px));
    min-height: 420px;
    max-height: calc(100vh - 84px);
  }

  .agent-body {
    flex-direction: column;
  }

  .agent-sessions {
    width: 100%;
    max-height: 180px;
    border-right: none;
    border-bottom: 1px solid rgba(122, 147, 91, 0.14);
  }
}
</style>
