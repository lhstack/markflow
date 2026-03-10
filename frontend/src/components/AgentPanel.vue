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
        <div class="agent-header-content">
          <div class="agent-meta-line">
            <span class="agent-meta-label">供应方：</span>
            <span class="agent-meta-value">{{ activeProvider?.name || '未配置供应商' }}</span>
            <span class="agent-meta-divider">|</span>
            <span class="agent-meta-label">Base URL:</span>
            <span class="agent-meta-value agent-meta-url">
              {{ activeProvider ? normalizedProviderBaseUrl(activeProvider.baseUrl) : DEFAULT_BASE_URL }}
            </span>
          </div>
          <div class="agent-toolbar-line" @mousedown.stop>
            <div class="agent-session-line">
              <span class="agent-meta-label">会话：</span>
              <el-select
                v-model="currentSessionId"
                class="agent-session-select"
                size="small"
                placeholder="选择会话"
              >
                <el-option
                  v-for="session in sessions"
                  :key="session.id"
                  :label="session.title"
                  :value="session.id"
                >
                  <div class="session-select-option">
                    <div class="session-select-info">
                      <span class="session-select-title">{{ session.title }}</span>
                      <span class="session-select-meta">{{ session.model || '未选模型' }}</span>
                    </div>
                    <div class="session-select-actions">
                      <span class="session-select-time">{{ formatSessionTime(session.updatedAt) }}</span>
                      <span
                        v-if="sessions.length > 1"
                        class="session-select-delete"
                        @click.stop.prevent="deleteSession(session.id)"
                      >
                        删除
                      </span>
                    </div>
                  </div>
                </el-option>
              </el-select>
            </div>

            <div class="agent-header-actions">
              <el-tooltip content="新建会话" placement="top">
                <el-button class="agent-header-icon" :icon="Plus" circle @click="createSession" />
              </el-tooltip>
              <el-tooltip content="供应商管理" placement="top">
                <el-button class="agent-header-icon" :icon="Setting" circle @click="showProviderDialog = true" />
              </el-tooltip>
              <button class="header-btn" title="清空当前会话" @click="clearCurrentSession">清空</button>
              <button class="header-btn" title="收起" @click="toggleCollapse(true)">收起</button>
            </div>
          </div>
        </div>
      </header>

      <section class="agent-main">
        <div ref="messagesRef" class="agent-messages">
          <template v-if="currentSession?.messages.length">
            <article
              v-for="message in currentSession.messages"
              :key="message.id"
              class="agent-message"
              :class="message.role"
            >
              <div class="message-role">{{ message.role === 'user' ? '用户' : '助手' }}</div>

              <details
                v-if="message.role === 'assistant' && displayedMessageReasoning(message)"
                class="message-reasoning"
              >
                <summary>推理</summary>
                <pre class="message-reasoning-content">{{ displayedMessageReasoning(message) }}</pre>
              </details>

              <pre class="message-content">{{ displayedMessageContent(message) || (streaming && message.role === 'assistant' ? '...' : '') }}</pre>
            </article>
          </template>

          <div v-else class="agent-empty">
            <div class="agent-empty-title">可以直接开始对话或写文档</div>
            <div class="agent-empty-desc">直接描述你的目标即可，模型会判断是答复、续写、改写，或在兼容接口上继续流式返回结果。</div>
          </div>
        </div>

        <div class="agent-composer">
          <div class="agent-mode-tip">{{ modeTip }}</div>
          <textarea
            v-model="prompt"
            class="agent-textarea"
            :placeholder="textareaPlaceholder"
            :disabled="streaming"
            @keydown="handleComposerKeydown"
          />

          <div class="agent-composer-footer">
            <div class="agent-shortcut-tip">Ctrl+Enter / Cmd+Enter 发送，Enter 换行</div>

            <div class="agent-bottom-bar">
              <div class="agent-controls">
                <div class="agent-inline-selects">
                  <div class="agent-select-row">
                    <span class="agent-control-label">供应方：</span>
                    <el-select
                      v-model="selectedProviderId"
                      class="agent-provider-select"
                      size="small"
                      placeholder="选择供应商"
                    >
                      <el-option
                        v-for="provider in providers"
                        :key="provider.id"
                        :label="provider.name"
                        :value="provider.id"
                      />
                    </el-select>
                  </div>

                  <div class="agent-select-row">
                    <span class="agent-control-label">模型：</span>
                    <el-select
                      v-model="currentSessionModel"
                      class="agent-model-select"
                      size="small"
                      filterable
                      allow-create
                      default-first-option
                      :reserve-keyword="false"
                      :disabled="!currentSession || !activeProvider"
                      placeholder="选择或添加模型"
                    >
                      <el-option
                        v-for="model in activeModelOptions"
                        :key="model"
                        :label="model"
                        :value="model"
                      />
                    </el-select>
                  </div>
                </div>
              </div>

              <div class="agent-action-row">
                <el-tooltip content="模型管理" placement="top">
                  <el-button
                    class="agent-icon-btn"
                    :icon="Setting"
                    :disabled="!activeProvider"
                    circle
                    @click="showModelDialog = true"
                  />
                </el-tooltip>
                <button class="primary-action" @click="streaming ? stopStreaming() : sendMessage()">
                  {{ streaming ? '停止' : '发送' }}
                </button>
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>

    <el-dialog v-model="showProviderDialog" class="agent-dialog provider-dialog" title="供应商配置" width="760px" append-to-body destroy-on-close>
      <div class="provider-manager">
        <aside class="provider-list-pane">
          <div class="provider-list">
            <button
              v-for="provider in providers"
              :key="provider.id"
              class="provider-item"
              :class="{ active: provider.id === providerDraft.id }"
              @click="editProvider(provider.id)"
            >
              <span class="provider-item-name">
                {{ provider.name }}
                <span v-if="provider.id === activeProviderId" class="provider-item-tag">已激活</span>
              </span>
              <span class="provider-item-meta">{{ normalizedProviderBaseUrl(provider.baseUrl) }}</span>
            </button>
          </div>
        </aside>

        <section class="provider-editor">
          <el-input v-model="providerDraft.name" placeholder="供应商名称，例如 OpenAI 官方" />
          <el-input v-model="providerDraft.baseUrl" placeholder="Base URL，例如 https://api.openai.com/v1" />
          <el-input v-model="providerDraft.apiKey" type="password" show-password placeholder="API Key" />
          <div class="provider-hint">供应商配置仅保存在当前浏览器。保存后请到“模型管理”里配置可选模型。</div>
        </section>
      </div>
      <template #footer>
        <el-button @click="showProviderDialog = false">关闭</el-button>
        <el-button @click="startCreateProvider">新增</el-button>
        <el-button :disabled="!providerDraft.id" @click="activateProvider(providerDraft.id)">设为激活</el-button>
        <el-button type="danger" plain :disabled="!providerDraft.id" @click="removeProvider(providerDraft.id)">删除</el-button>
        <el-button type="primary" @click="saveProviderDraft">保存</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showModelDialog" class="agent-dialog model-dialog" title="模型管理" width="760px" append-to-body destroy-on-close>
      <div v-if="activeProvider" class="model-manager">
        <div class="model-header">
          <div>
            <div class="model-header-title">{{ activeProvider.name }}</div>
            <div class="model-header-meta">{{ normalizedProviderBaseUrl(activeProvider.baseUrl) }}</div>
          </div>
          <el-button :loading="modelLoading" @click="fetchProviderModels">同步远端模型</el-button>
        </div>

        <div class="provider-hint">左侧是当前可用模型，右侧是 SDK 返回模型。勾选右侧模型后，当前会话选择框就能直接使用。</div>

        <div class="model-grid">
          <section class="model-pane">
            <div class="model-pane-head">
              <div class="model-section-title">当前模型列表</div>
            </div>

            <div class="model-pane-body model-pane-scroll">
              <div v-if="currentManagedModels.length" class="current-model-list">
                <div
                  v-for="entry in currentManagedModels"
                  :key="entry.id"
                  class="current-model-item"
                >
                  <div class="current-model-main">
                    <span class="current-model-name">{{ entry.id }}</span>
                    <span v-if="entry.isCustom" class="model-source-badge is-custom">自定义</span>
                    <span v-else-if="entry.isRemote" class="model-source-badge">SDK</span>
                  </div>
                  <div class="current-model-actions">
                    <el-button
                      link
                      type="danger"
                      @click="entry.isCustom ? removeCustomModel(entry.id) : toggleCustomModel(entry.id, false)"
                    >
                      移除
                    </el-button>
                  </div>
                </div>
              </div>
              <div v-else class="model-empty">还没有可用模型。</div>
            </div>

            <div class="model-pane-foot">
              <div class="model-custom-row">
                <el-input
                  v-model="customModelInput"
                  placeholder="新增自定义模型，例如 gpt-4.1 或 qwen-plus"
                  @keydown.enter.prevent="addCustomModel"
                />
                <el-button @click="addCustomModel">新增</el-button>
              </div>
            </div>
          </section>

          <section class="model-pane">
            <div class="model-pane-head model-pane-head-row">
              <div class="model-section-title">SDK 模型列表</div>
              <el-input
                v-model="modelSearchQuery"
                class="model-search-input"
                clearable
                placeholder="搜索模型 ID"
              />
            </div>

            <div class="model-pane-body model-pane-scroll">
              <el-checkbox-group v-model="modelDraft.enabledModels" class="model-check-list">
                <label
                  v-for="model in filteredRemoteModels"
                  :key="model"
                  class="model-check-item"
                >
                  <el-checkbox :value="model">{{ model }}</el-checkbox>
                </label>
              </el-checkbox-group>
              <div v-if="!modelDraft.remoteModels.length" class="model-empty">还没有拉取到远端模型。</div>
              <div v-else-if="!filteredRemoteModels.length" class="model-empty">没有匹配的模型。</div>
            </div>
          </section>
        </div>
      </div>

      <div v-else class="model-empty-state">
        <div class="model-empty">请先在供应商配置里新增并激活一个供应商。</div>
        <el-button type="primary" @click="openProviderManagerFromModelDialog">去配置供应商</el-button>
      </div>

      <template #footer>
        <el-button @click="showModelDialog = false">取消</el-button>
        <el-button type="primary" :disabled="!activeProvider" @click="saveModelDraft">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, triggerRef, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Plus, Setting } from '@element-plus/icons-vue'

import request from '@/utils/request'
import {
  executeAgentToolCalls,
  type AgentToolCall,
  type AgentToolOutputPayload,
} from '@/utils/agentTools'
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
type RequestRole = SessionRole | 'system'
type StreamAction = 'chat' | AgentWriterMode
type RouteKind = 'overview' | 'project' | 'doc'

interface AgentRouteTarget {
  kind: RouteKind
  name?: string
}

interface AgentMessage {
  id: string
  role: SessionRole
  content: string
  reasoning?: string
}

interface AgentRequestMessage {
  role: RequestRole
  content: string
}

interface AgentSession {
  id: string
  title: string
  messages: AgentMessage[]
  providerId: string | null
  model: string
  previousResponseId: string | null
  lastSyncedMessageCount: number
  createdAt: number
  updatedAt: number
}

interface AgentProvider {
  id: string
  name: string
  baseUrl: string
  hasApiKey: boolean
  remoteModels: string[]
  enabledModels: string[]
  customModels: string[]
  createdAt: number
  updatedAt: number
}

interface ProviderDraft {
  id: string | null
  name: string
  baseUrl: string
  apiKey: string
}

interface ModelDraft {
  remoteModels: string[]
  enabledModels: string[]
  customModels: string[]
}

interface ModelApiItem {
  id: string
  owned_by?: string
  created?: number
}

interface ProviderApiItem {
  id: number | string
  name: string
  base_url?: string
  remote_models?: string[]
  enabled_models?: string[]
  custom_models?: string[]
  is_active?: boolean
  has_api_key?: boolean
  created_at?: string
  updated_at?: string
}

interface ProviderListResponse {
  providers?: ProviderApiItem[]
  active_provider_id?: number | string | null
}

interface ProviderDetailResponse {
  id: number | string
  name: string
  base_url?: string
  api_key?: string
  remote_models?: string[]
  enabled_models?: string[]
  custom_models?: string[]
  is_active?: boolean
}

const REQUEST_RECENT_MESSAGE_COUNT = 8
const REQUEST_SUMMARY_TRIGGER_CHARS = 6000
const REQUEST_SUMMARY_MAX_ITEMS = 6
const REQUEST_SUMMARY_ITEM_CHARS = 240

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

const DEFAULT_BASE_URL = 'https://api.openai.com/v1'
const PANEL_STATE_KEY = 'markflow.agent.panel.state'
const SESSIONS_KEY = 'markflow.agent.sessions'
const VIEWPORT_MARGIN = 24
const FAB_WIDTH = 96
const FAB_HEIGHT = 48
const PANEL_WIDTH = 520
const PANEL_HEIGHT = 720

const mounted = ref(false)
const collapsed = ref(false)
const showProviderDialog = ref(false)
const showModelDialog = ref(false)
const streaming = ref(false)
const modelLoading = ref(false)
const prompt = ref('')
const panelX = ref(0)
const panelY = ref(88)
const expandedPanelX = ref(0)
const expandedPanelY = ref(88)
const collapsedPanelX = ref(0)
const collapsedPanelY = ref(88)
const currentSessionId = ref<string>('')
const activeProviderId = ref<string>('')
const sessions = ref<AgentSession[]>([])
const providers = ref<AgentProvider[]>([])
const streamingAssistantId = ref('')
const liveAssistantContent = ref('')
const liveAssistantReasoning = ref('')
const agentTransportMode = ref<'responses' | 'chat_fallback' | ''>('')
const messagesRef = ref<HTMLElement | null>(null)
const customModelInput = ref('')
const modelSearchQuery = ref('')
const providerDraft = ref<ProviderDraft>({
  id: null,
  name: '',
  baseUrl: DEFAULT_BASE_URL,
  apiKey: '',
})
const modelDraft = ref<ModelDraft>({
  remoteModels: [],
  enabledModels: [],
  customModels: [],
})

let dragOffsetX = 0
let dragOffsetY = 0
let dragging = false
let didDrag = false
let activeStreamController: AbortController | null = null

const panelStyle = computed(() => ({
  transform: `translate(${panelX.value}px, ${panelY.value}px)`,
}))

const currentSession = computed(() => sessions.value.find((session) => session.id === currentSessionId.value) || null)
const activeProvider = computed(() => providers.value.find((provider) => provider.id === activeProviderId.value) || null)
const selectedProviderId = computed({
  get: () => activeProviderId.value,
  set: (value: string) => {
    activateProvider(value)
  },
})
const activeModelOptions = computed(() => enabledModelsForProvider(activeProvider.value))
const currentSessionModel = computed({
  get: () => currentSession.value?.model || '',
  set: (value: string) => {
    const session = currentSession.value
    if (!session) return
    const normalized = value.trim()
    const modelChanged = session.model !== normalized
    ensureModelAvailableForActiveProvider(normalized)
    session.providerId = activeProvider.value?.id || null
    session.model = normalized
    if (modelChanged) {
      session.previousResponseId = null
      session.lastSyncedMessageCount = 0
    }
    sessions.value = [...sessions.value]
    persistSessions()
  },
})
const activeModelKey = computed(() => activeModelOptions.value.join('|'))
const filteredRemoteModels = computed(() => {
  const keyword = modelSearchQuery.value.trim().toLowerCase()
  if (!keyword) return modelDraft.value.remoteModels
  return modelDraft.value.remoteModels.filter((model) => model.toLowerCase().includes(keyword))
})
const currentManagedModels = computed(() =>
  uniqueStrings([...modelDraft.value.enabledModels]).map((model) => ({
    id: model,
    isCustom: modelDraft.value.customModels.includes(model),
    isRemote: modelDraft.value.remoteModels.includes(model),
  })),
)

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

function uniqueStrings(values: unknown[]): string[] {
  const normalized = values
    .map((value) => typeof value === 'string' ? value.trim() : '')
    .filter(Boolean)
  return Array.from(new Set(normalized))
}

function normalizedProviderBaseUrl(value?: string | null) {
  const trimmed = typeof value === 'string' ? value.trim() : ''
  return (trimmed || DEFAULT_BASE_URL).replace(/\/+$/, '')
}

function enabledModelsForProvider(provider: AgentProvider | null) {
  return provider ? uniqueStrings(provider.enabledModels) : []
}

function fallbackModelForProvider(provider: AgentProvider | null) {
  if (!provider) return ''
  const enabled = enabledModelsForProvider(provider)
  if (enabled.length) return enabled[0]
  return uniqueStrings([...provider.customModels, ...provider.remoteModels])[0] || ''
}

function ensureModelAvailableForActiveProvider(model: string) {
  if (!model) return
  const provider = activeProvider.value
  if (!provider) return

  let changed = false

  if (!provider.customModels.includes(model) && !provider.remoteModels.includes(model)) {
    provider.customModels = uniqueStrings([...provider.customModels, model])
    changed = true
  }
  if (!provider.enabledModels.includes(model)) {
    provider.enabledModels = uniqueStrings([...provider.enabledModels, model])
    changed = true
  }

  if (changed) {
    provider.updatedAt = Date.now()
    providers.value = [...providers.value]
    void saveProviderConfig(provider)
  }
}

async function saveProviderConfig(provider: AgentProvider) {
  const data = await request.post('/agent/providers', {
    id: Number(provider.id),
    name: provider.name,
    base_url: provider.baseUrl,
    api_key: '',
    remote_models: provider.remoteModels,
    enabled_models: provider.enabledModels,
    custom_models: provider.customModels,
  }) as ProviderListResponse

  const normalizedProviders = (data.providers || [])
    .map((item) => normalizeProvider(item))
    .filter((item): item is AgentProvider => Boolean(item))

  providers.value = normalizedProviders
  activeProviderId.value = `${data.active_provider_id ?? normalizedProviders.find((item) => item.id === activeProviderId.value)?.id ?? normalizedProviders[0]?.id ?? ''}`
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
  const expandedPosition = clampPanelPosition(
    window.innerWidth - PANEL_WIDTH - VIEWPORT_MARGIN,
    window.innerHeight - PANEL_HEIGHT - VIEWPORT_MARGIN,
    false,
  )
  const collapsedPosition = clampPanelPosition(
    window.innerWidth - FAB_WIDTH - VIEWPORT_MARGIN,
    window.innerHeight - FAB_HEIGHT - VIEWPORT_MARGIN,
    true,
  )
  return {
    collapsed: nextCollapsed,
    panelX: nextCollapsed ? collapsedPosition.x : expandedPosition.x,
    panelY: nextCollapsed ? collapsedPosition.y : expandedPosition.y,
    expandedPanelX: expandedPosition.x,
    expandedPanelY: expandedPosition.y,
    collapsedPanelX: collapsedPosition.x,
    collapsedPanelY: collapsedPosition.y,
    currentSessionId: '',
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
  if (collapsed.value) {
    collapsedPanelX.value = panelX.value
    collapsedPanelY.value = panelY.value
  } else {
    expandedPanelX.value = panelX.value
    expandedPanelY.value = panelY.value
  }

  const nextPosition = clampPanelPosition(
    nextCollapsed ? collapsedPanelX.value : expandedPanelX.value,
    nextCollapsed ? collapsedPanelY.value : expandedPanelY.value,
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
      expandedPanelX: expandedPanelX.value,
      expandedPanelY: expandedPanelY.value,
      collapsedPanelX: collapsedPanelX.value,
      collapsedPanelY: collapsedPanelY.value,
      currentSessionId: currentSessionId.value,
    }),
  )
}

function persistSessions() {
  localStorage.setItem(SESSIONS_KEY, JSON.stringify(sessions.value))
}

function createProviderDraft(seed = ''): ProviderDraft {
  return {
    id: null,
    name: seed || `供应商 ${providers.value.length + 1}`,
    baseUrl: DEFAULT_BASE_URL,
    apiKey: '',
  }
}

function normalizeProvider(raw: any): AgentProvider | null {
  if (!raw || typeof raw !== 'object') return null

  const id = `${raw.id ?? ''}`.trim() || genId()
  const name = typeof raw.name === 'string' && raw.name.trim() ? raw.name.trim() : '未命名供应商'
  const remoteModels = uniqueStrings(Array.isArray(raw.remote_models ?? raw.remoteModels) ? (raw.remote_models ?? raw.remoteModels) : [])
  const customModels = uniqueStrings(Array.isArray(raw.custom_models ?? raw.customModels) ? (raw.custom_models ?? raw.customModels) : [])
  const enabledModels = uniqueStrings(Array.isArray(raw.enabled_models ?? raw.enabledModels) ? (raw.enabled_models ?? raw.enabledModels) : [])

  return {
    id,
    name,
    baseUrl: normalizedProviderBaseUrl(raw.base_url ?? raw.baseUrl),
    hasApiKey: Boolean(raw.has_api_key ?? raw.hasApiKey),
    remoteModels,
    enabledModels,
    customModels,
    createdAt: raw.created_at ? new Date(raw.created_at).getTime() : Number.isFinite(raw.createdAt) ? raw.createdAt : Date.now(),
    updatedAt: raw.updated_at ? new Date(raw.updated_at).getTime() : Number.isFinite(raw.updatedAt) ? raw.updatedAt : Date.now(),
  }
}

async function refreshProvidersState() {
  const data = await request.get('/agent/providers') as ProviderListResponse
  const normalizedProviders = (data.providers || [])
    .map((provider) => normalizeProvider(provider))
    .filter((provider): provider is AgentProvider => Boolean(provider))

  providers.value = normalizedProviders
  activeProviderId.value = normalizedProviders.some((provider) => provider.id === `${data.active_provider_id ?? ''}`)
    ? `${data.active_provider_id ?? ''}`
    : normalizedProviders.find((provider) => provider.id === activeProviderId.value)?.id || normalizedProviders[0]?.id || ''
}

function normalizeMessage(raw: any): AgentMessage | null {
  if (!raw || typeof raw !== 'object') return null
  const role = raw.role === 'assistant' ? 'assistant' : raw.role === 'user' ? 'user' : null
  if (!role) return null

  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : genId(),
    role,
    content: typeof raw.content === 'string' ? raw.content : '',
    reasoning: typeof raw.reasoning === 'string' ? raw.reasoning : '',
  }
}

function normalizeSession(raw: any, provider: AgentProvider | null): AgentSession | null {
  if (!raw || typeof raw !== 'object') return null
  const messages = Array.isArray(raw.messages)
    ? raw.messages
      .map((message: any) => normalizeMessage(message))
      .filter((message: AgentMessage | null): message is AgentMessage => Boolean(message))
    : []

  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : genId(),
    title: typeof raw.title === 'string' && raw.title.trim() ? raw.title.trim() : '新会话',
    messages,
    providerId: typeof raw.providerId === 'string' && raw.providerId.trim() ? raw.providerId.trim() : provider?.id || null,
    model: typeof raw.model === 'string' ? raw.model.trim() : fallbackModelForProvider(provider),
    previousResponseId: typeof raw.previousResponseId === 'string' && raw.previousResponseId.trim() ? raw.previousResponseId.trim() : null,
    lastSyncedMessageCount: Number.isInteger(raw.lastSyncedMessageCount) ? Math.max(0, raw.lastSyncedMessageCount) : 0,
    createdAt: Number.isFinite(raw.createdAt) ? raw.createdAt : Date.now(),
    updatedAt: Number.isFinite(raw.updatedAt) ? raw.updatedAt : Date.now(),
  }
}

function loadSessions(provider: AgentProvider | null) {
  return loadJson<any[]>(SESSIONS_KEY, [])
    .map((session) => normalizeSession(session, provider))
    .filter((session): session is AgentSession => Boolean(session))
}

function ensureProviderDraftLoaded() {
  if (providerDraft.value.id && providers.value.some((provider) => provider.id === providerDraft.value.id)) return
  if (activeProvider.value) {
    editProvider(activeProvider.value.id)
    return
  }
  if (providers.value[0]) {
    editProvider(providers.value[0].id)
    return
  }
  providerDraft.value = createProviderDraft()
}

function resetModelDraft() {
  const provider = activeProvider.value
  if (!provider) {
    modelDraft.value = { remoteModels: [], enabledModels: [], customModels: [] }
    return
  }

  modelDraft.value = {
    remoteModels: [...provider.remoteModels],
    enabledModels: [...provider.enabledModels],
    customModels: [...provider.customModels],
  }
}

function ensureSession(): AgentSession {
  let session = currentSession.value
  if (session) return session

  session = {
    id: genId(),
    title: '新会话',
    messages: [],
    providerId: activeProvider.value?.id || null,
    model: fallbackModelForProvider(activeProvider.value),
    previousResponseId: null,
    lastSyncedMessageCount: 0,
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
    providerId: activeProvider.value?.id || null,
    model: fallbackModelForProvider(activeProvider.value),
    previousResponseId: null,
    lastSyncedMessageCount: 0,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }
  sessions.value.unshift(session)
  currentSessionId.value = session.id
  prompt.value = ''
  persistSessions()
  persistPanelState()
  scrollMessagesToBottom()
}

function selectSession(sessionId: string) {
  currentSessionId.value = sessionId
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
  session.previousResponseId = null
  session.lastSyncedMessageCount = 0
  session.updatedAt = Date.now()
  sessions.value = [...sessions.value]
  persistSessions()
}

function updateSessionMessage(
  sessionId: string,
  messageId: string,
  patch: Partial<Pick<AgentMessage, 'content' | 'reasoning'>>,
  options: { persist?: boolean } = {},
) {
  const session = sessions.value.find((item) => item.id === sessionId)
  if (!session) return
  const message = session.messages.find((item) => item.id === messageId)
  if (!message) return
  if (typeof patch.content === 'string') {
    message.content = patch.content
  }
  if (typeof patch.reasoning === 'string') {
    message.reasoning = patch.reasoning
  }
  session.updatedAt = Date.now()
  if (options.persist !== false) {
    sessions.value = [...sessions.value]
    persistSessions()
  } else {
    triggerRef(sessions)
  }
  scrollMessagesToBottom()
}

function isStreamingAssistantMessage(message: AgentMessage) {
  return streaming.value && message.role === 'assistant' && message.id === streamingAssistantId.value
}

function displayedMessageContent(message: AgentMessage) {
  return isStreamingAssistantMessage(message) ? liveAssistantContent.value : message.content
}

function displayedMessageReasoning(message: AgentMessage) {
  return isStreamingAssistantMessage(message) ? liveAssistantReasoning.value : (message.reasoning || '')
}

function syncSessionWithActiveProvider(session: AgentSession | null) {
  if (!session) return

  const provider = activeProvider.value
  const providerId = provider?.id || null
  const allowedModels = enabledModelsForProvider(provider)
  let changed = false

  if (session.providerId !== providerId) {
    session.providerId = providerId
    session.previousResponseId = null
    session.lastSyncedMessageCount = 0
    changed = true
  }

  if (!allowedModels.length) {
    if (session.model) {
      session.model = ''
      changed = true
    }
  } else if (!allowedModels.includes(session.model)) {
    session.model = allowedModels[0]
    changed = true
  }

  if (changed) {
    sessions.value = [...sessions.value]
    persistSessions()
  }
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
  if (collapsed.value) {
    collapsedPanelX.value = clamped.x
    collapsedPanelY.value = clamped.y
  } else {
    expandedPanelX.value = clamped.x
    expandedPanelY.value = clamped.y
  }
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

async function editProvider(providerId: string) {
  const provider = providers.value.find((item) => item.id === providerId)
  if (!provider) return
  try {
    const detail = await request.get(`/agent/providers/${providerId}`) as ProviderDetailResponse
    providerDraft.value = {
      id: `${detail.id}`,
      name: detail.name || provider.name,
      baseUrl: normalizedProviderBaseUrl(detail.base_url || provider.baseUrl),
      apiKey: detail.api_key || '',
    }
  } catch (error: any) {
    providerDraft.value = {
      id: provider.id,
      name: provider.name,
      baseUrl: provider.baseUrl,
      apiKey: '',
    }
    ElMessage.error(error.response?.data?.error || error.message || '读取供应商详情失败')
  }
}

function startCreateProvider() {
  providerDraft.value = createProviderDraft()
}

async function saveProviderDraft() {
  const name = providerDraft.value.name.trim()
  const apiKey = providerDraft.value.apiKey.trim()
  const editingProvider = providerDraft.value.id
    ? providers.value.find((item) => item.id === providerDraft.value.id)
    : null

  if (!name) {
    ElMessage.warning('请填写供应商名称')
    return
  }
  if (!apiKey && !editingProvider?.hasApiKey) {
    ElMessage.warning('请填写 API Key')
    return
  }

  try {
    const data = await request.post('/agent/providers', {
      id: providerDraft.value.id ? Number(providerDraft.value.id) : null,
      name,
      base_url: normalizedProviderBaseUrl(providerDraft.value.baseUrl),
      api_key: apiKey,
      remote_models: editingProvider?.remoteModels || [],
      enabled_models: editingProvider?.enabledModels || [],
      custom_models: editingProvider?.customModels || [],
    }) as ProviderListResponse

    providers.value = (data.providers || [])
      .map((provider) => normalizeProvider(provider))
      .filter((provider): provider is AgentProvider => Boolean(provider))
    activeProviderId.value = `${data.active_provider_id ?? providers.value[0]?.id ?? ''}`
    ensureProviderDraftLoaded()
    syncSessionWithActiveProvider(currentSession.value)
    ElMessage.success('供应商配置已保存')
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '保存供应商失败')
  }
}

async function activateProvider(providerId: string | null) {
  if (!providerId) return
  if (!providers.value.some((provider) => provider.id === providerId)) return
  try {
    const data = await request.post(`/agent/providers/${providerId}/activate`) as ProviderListResponse
    providers.value = (data.providers || [])
      .map((provider) => normalizeProvider(provider))
      .filter((provider): provider is AgentProvider => Boolean(provider))
    activeProviderId.value = `${data.active_provider_id ?? providerId}`
    syncSessionWithActiveProvider(currentSession.value)
    ElMessage.success('已切换激活供应商')
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '切换供应商失败')
  }
}

async function removeProvider(providerId: string | null) {
  if (!providerId) return
  try {
    const data = await request.delete(`/agent/providers/${providerId}`) as ProviderListResponse
    providers.value = (data.providers || [])
      .map((provider) => normalizeProvider(provider))
      .filter((provider): provider is AgentProvider => Boolean(provider))
    activeProviderId.value = `${data.active_provider_id ?? providers.value[0]?.id ?? ''}`
    ensureProviderDraftLoaded()
    if (!providers.value.length) {
      providerDraft.value = createProviderDraft()
      showModelDialog.value = false
    }
    syncSessionWithActiveProvider(currentSession.value)
    ElMessage.success('供应商已删除')
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '删除供应商失败')
  }
}

async function fetchProviderModels() {
  const provider = activeProvider.value
  if (!provider) {
    ElMessage.warning('请先激活一个供应商')
    return
  }
  if (!provider.hasApiKey) {
    ElMessage.warning('当前供应商缺少 API Key')
    return
  }

  modelLoading.value = true
  try {
    const data = await request.post('/agent/models', {
      provider_id: Number(provider.id),
    }) as { models?: ModelApiItem[] }

    const ids = uniqueStrings((data.models || []).map((item) => item.id))
    modelDraft.value.remoteModels = ids
    ElMessage.success(`已同步 ${ids.length} 个模型`)
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '获取模型列表失败')
  } finally {
    modelLoading.value = false
  }
}

function addCustomModel() {
  const name = customModelInput.value.trim()
  if (!name) return

  modelDraft.value.customModels = uniqueStrings([...modelDraft.value.customModels, name])
  modelDraft.value.enabledModels = uniqueStrings([...modelDraft.value.enabledModels, name])
  customModelInput.value = ''
}

function removeCustomModel(model: string) {
  modelDraft.value.customModels = modelDraft.value.customModels.filter((item) => item !== model)
  modelDraft.value.enabledModels = modelDraft.value.enabledModels.filter((item) => item !== model)
}

function toggleCustomModel(model: string, checked: string | number | boolean) {
  if (checked) {
    modelDraft.value.enabledModels = uniqueStrings([...modelDraft.value.enabledModels, model])
  } else {
    modelDraft.value.enabledModels = modelDraft.value.enabledModels.filter((item) => item !== model)
  }
}

async function saveModelDraft() {
  const provider = activeProvider.value
  if (!provider) {
    ElMessage.warning('请先激活一个供应商')
    return
  }

  provider.remoteModels = uniqueStrings(modelDraft.value.remoteModels)
  provider.customModels = uniqueStrings(modelDraft.value.customModels)
  provider.enabledModels = uniqueStrings([
    ...modelDraft.value.enabledModels.filter((model) =>
      provider.remoteModels.includes(model) || provider.customModels.includes(model),
    ),
  ])
  provider.updatedAt = Date.now()

  try {
    await saveProviderConfig(provider)
    syncSessionWithActiveProvider(currentSession.value)
    showModelDialog.value = false
    ElMessage.success('模型配置已保存')
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '保存模型配置失败')
  }
}

function openProviderManagerFromModelDialog() {
  showModelDialog.value = false
  showProviderDialog.value = true
}

function currentDocContent() {
  if (!props.docId) return ''
  return getAgentEditorSnapshot(props.docId) ?? props.docContent ?? ''
}

function compactMessageText(content: string, maxChars = REQUEST_SUMMARY_ITEM_CHARS) {
  const compact = content.replace(/\s+/g, ' ').trim()
  if (!compact) return ''
  if (compact.length <= maxChars) return compact
  return `${compact.slice(0, maxChars)}...`
}

function buildHistorySummary(messages: AgentMessage[]) {
  const userItems: string[] = []
  const assistantItems: string[] = []

  for (const message of messages) {
    const compact = compactMessageText(message.content)
    if (!compact) continue

    if (message.role === 'user') {
      if (userItems.length < REQUEST_SUMMARY_MAX_ITEMS) {
        userItems.push(`- ${compact}`)
      }
      continue
    }

    if (assistantItems.length < REQUEST_SUMMARY_MAX_ITEMS) {
      assistantItems.push(`- ${compact}`)
    }
  }

  if (!userItems.length && !assistantItems.length) return ''

  const parts = ['以下是当前会话中较早消息的摘要，请基于此继续对话，不要假设摘要之外的旧细节仍然准确。']
  if (userItems.length) {
    parts.push('较早的用户诉求与补充：')
    parts.push(...userItems)
  }
  if (assistantItems.length) {
    parts.push('较早的助手答复与已完成事项：')
    parts.push(...assistantItems)
  }
  return parts.join('\n')
}

function buildConversationMessages(messages: AgentMessage[]): AgentRequestMessage[] {
  const nonEmptyMessages = messages
    .map((message) => ({
      ...message,
      content: message.content.trim(),
    }))
    .filter((message) => Boolean(message.content))
  const normalized = nonEmptyMessages.map((message) => ({
    role: message.role,
    content: message.content,
  }))

  if (!normalized.length) return []

  const totalChars = normalized.reduce((sum, message) => sum + message.content.length, 0)
  if (normalized.length <= REQUEST_RECENT_MESSAGE_COUNT || totalChars <= REQUEST_SUMMARY_TRIGGER_CHARS) {
    return normalized
  }

  const recentMessages = normalized.slice(-REQUEST_RECENT_MESSAGE_COUNT)
  const olderMessages = nonEmptyMessages.slice(0, Math.max(0, normalized.length - REQUEST_RECENT_MESSAGE_COUNT))
  const summary = buildHistorySummary(olderMessages)

  return summary
    ? [{ role: 'system', content: summary }, ...recentMessages]
    : recentMessages
}

function buildRequestBody(
  messages: AgentRequestMessage[],
  provider: AgentProvider,
  model: string,
  options: {
    previousResponseId?: string | null
    toolOutputs?: AgentToolOutputPayload[] | null
  } = {},
) {
  return {
    provider: {
      provider_id: Number(provider.id),
      model,
    },
    messages: messages.map((message) => ({
      role: message.role,
      content: message.content,
    })),
    mode: props.docType === 'doc' ? 'auto' : 'chat',
    context: {
      page_scope: props.pageScope,
      project_name: props.projectName || null,
      doc_id: props.docId,
      doc_name: props.docName || null,
      doc_content: currentDocContent(),
      project_catalog: props.projectCatalog || null,
      current_node_catalog: props.currentNodeCatalog || null,
    },
    previous_response_id: options.previousResponseId || null,
    tool_outputs: options.toolOutputs || null,
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

function scrollMessagesToBottom() {
  void nextTick().then(() => {
    window.requestAnimationFrame(() => {
      const el = messagesRef.value
      if (!el) return
      el.scrollTop = el.scrollHeight
    })
  })
}

function handleComposerKeydown(event: KeyboardEvent) {
  if (event.key !== 'Enter') return
  if (event.isComposing) return

  if (event.ctrlKey || event.metaKey) {
    event.preventDefault()
    void sendMessage()
  }
}

function stopStreaming() {
  activeStreamController?.abort()
}

async function sendMessage() {
  if (streaming.value) return
  const text = prompt.value.trim()
  if (!text) return

  const provider = activeProvider.value
  if (!provider) {
    ElMessage.warning('请先配置并激活一个供应商')
    showProviderDialog.value = true
    return
  }
  if (!provider.hasApiKey) {
    ElMessage.warning('当前激活供应商缺少 API Key')
    showProviderDialog.value = true
    return
  }

  const session = ensureSession()
  syncSessionWithActiveProvider(session)

  if (!session.model.trim()) {
    ElMessage.warning('请先在模型管理中配置可选模型')
    showModelDialog.value = true
    return
  }

  const userMessage: AgentMessage = { id: genId(), role: 'user', content: text }
  const assistantMessage: AgentMessage = { id: genId(), role: 'assistant', content: '', reasoning: '' }
  session.messages.push(userMessage, assistantMessage)
  session.updatedAt = Date.now()
  session.providerId = provider.id

  if (session.title === '新会话') {
    session.title = text.slice(0, 18)
  }

  sessions.value = [...sessions.value]
  persistSessions()
  scrollMessagesToBottom()
  prompt.value = ''
  streaming.value = true
  streamingAssistantId.value = assistantMessage.id
  liveAssistantContent.value = ''
  liveAssistantReasoning.value = ''
  let routeAction: StreamAction | null = null
  let prefixProbe = ''
  let pendingRoute: AgentRouteTarget | null = null
  let writerStarted = false
  let renderBuffer = ''
  let inThinkBlock = false
  let streamFailed = false
  let pendingToolOutputs: AgentToolOutputPayload[] | null = null
  let streamAborted = false
  let previousResponseId: string | null = session.previousResponseId
  let rawAssistantContent = ''
  let completedAssistantContent = ''
  let wroteDocument = false
  let consumeAssistantText = (_rawChunk: string, _force = false) => {}
  let handleAssistantChunk = (_rawChunk: string) => {}
  let routeChunk = (_rawChunk: string, _force = false) => {}
  let recoverTrailingActionMarker = () => {}
  let buildVisibleAssistantContent = (_source = rawAssistantContent, _streamMode = false) => _source
  let appendUnsavedDraftNotice = () => {}

  try {
    const abortController = new AbortController()
    activeStreamController = abortController

    const appendAssistantContent = (content: string) => {
      if (!content) return

      if (routeAction && routeAction !== 'chat' && props.docId) {
        if (!writerStarted) {
          const writerMode: AgentWriterMode = routeAction
          dispatchAgentWriterStart({ docId: props.docId, mode: writerMode, save: false })
          writerStarted = true
        }
        wroteDocument = true
        dispatchAgentWriterChunk({ docId: props.docId, chunk: content })
        return
      }

      liveAssistantContent.value = `${liveAssistantContent.value}${content}`
      scrollMessagesToBottom()
    }

    const appendAssistantReasoning = (delta: string) => {
      if (!delta) return
      liveAssistantReasoning.value = `${liveAssistantReasoning.value}${delta}`
      scrollMessagesToBottom()
    }

    buildVisibleAssistantContent = (source = rawAssistantContent, streamMode = false) => {
      let visible = source
        .replace(/\s*\[\[ACTION:(append|replace|rewrite_section|replace_block)\]\][\s\S]*?\[\[\/ACTION\]\]\s*/gi, '\n')

      if (streamMode) {
        const upperVisible = visible.toUpperCase()
        const openAppendIndex = upperVisible.lastIndexOf('[[ACTION:APPEND]]')
        const openReplaceIndex = upperVisible.lastIndexOf('[[ACTION:REPLACE]]')
        const openRewriteSectionIndex = upperVisible.lastIndexOf('[[ACTION:REWRITE_SECTION]]')
        const openReplaceBlockIndex = upperVisible.lastIndexOf('[[ACTION:REPLACE_BLOCK]]')
        const openIndex = Math.max(openAppendIndex, openReplaceIndex, openRewriteSectionIndex, openReplaceBlockIndex)
        if (openIndex !== -1) {
          const closeIndex = upperVisible.lastIndexOf('[[/ACTION]]')
          if (openIndex > closeIndex) {
            visible = visible.slice(0, openIndex)
          }
        }
      }

      return visible
        .replace(/[ \t]+\n/g, '\n')
        .replace(/\n{3,}/g, '\n\n')
        .trim()
    }

    appendUnsavedDraftNotice = () => {
      if (!wroteDocument || streamAborted || streamFailed) return
      if (/(保存|save)/i.test(liveAssistantContent.value)) return
      const prefix = liveAssistantContent.value.trim() ? '\n\n' : ''
      liveAssistantContent.value = `${liveAssistantContent.value}${prefix}内容已写入当前文档草稿，尚未保存。是否现在保存？`
    }

    const appendCompletedTail = (completedContent: string) => {
      if (!completedContent) return
      if (!rawAssistantContent) {
        rawAssistantContent = completedContent
        handleAssistantChunk(completedContent)
        return
      }
      if (completedContent === rawAssistantContent) return

      if (completedContent.startsWith(rawAssistantContent)) {
        const tail = completedContent.slice(rawAssistantContent.length)
        if (!tail) return
        rawAssistantContent += tail
        handleAssistantChunk(tail)
        return
      }

      // Some providers only give a partial delta stream and then a final full
      // message. Rebuild the visible chat content from the final message and
      // only replay the unseen suffix into the editor/chat router.
      const overlapLength = (() => {
        const max = Math.min(rawAssistantContent.length, completedContent.length)
        for (let length = max; length > 0; length -= 1) {
          if (rawAssistantContent.endsWith(completedContent.slice(0, length))) {
            return length
          }
        }
        return 0
      })()
      const tail = completedContent.slice(overlapLength)
      const actionCloseMarker = '[[/ACTION]]'
      const existingCloseIndex = rawAssistantContent.toUpperCase().indexOf(actionCloseMarker)
      const completedHasActionBlock = /\[\[ACTION:(append|replace|rewrite_section|replace_block)\]\]/i.test(completedContent)
      const mergedCompletedContent = (() => {
        if (!completedHasActionBlock) return completedContent
        if (existingCloseIndex === -1) return completedContent
        const existingTail = rawAssistantContent.slice(existingCloseIndex + actionCloseMarker.length)
        if (!existingTail.trim()) return completedContent
        return completedContent.includes(existingTail)
          ? completedContent
          : `${completedContent}${existingTail}`
      })()
      rawAssistantContent = mergedCompletedContent
      if (tail) {
        handleAssistantChunk(tail)
      }
    }

    recoverTrailingActionMarker = () => {
      if (routeAction && routeAction !== 'chat') return
      if (props.docType !== 'doc' || !props.docId) return

      const wrappedMatch = rawAssistantContent.match(/^\s*\[\[ACTION:(append|replace|rewrite_section|replace_block)\]\]\s*([\s\S]*?)\s*\[\[\/ACTION\]\]\s*$/i)
      if (wrappedMatch) {
        const mode = wrappedMatch[1].toLowerCase() as AgentWriterMode
        const body = wrappedMatch[2] || ''
        liveAssistantContent.value = ''
        routeAction = mode
        dispatchAgentWriterStart({ docId: props.docId, mode, save: false })
        writerStarted = true
        if (body) {
          dispatchAgentWriterChunk({ docId: props.docId, chunk: body })
        }
      }
    }

    const flushAssistantRenderBuffer = (force = false) => {
      const openTag = '<think>'
      const closeTag = '</think>'

      while (renderBuffer) {
        if (inThinkBlock) {
          const closeIndex = renderBuffer.indexOf(closeTag)
          if (closeIndex !== -1) {
            appendAssistantReasoning(renderBuffer.slice(0, closeIndex))
            renderBuffer = renderBuffer.slice(closeIndex + closeTag.length)
            inThinkBlock = false
            continue
          }

          const safeLength = force ? renderBuffer.length : Math.max(0, renderBuffer.length - closeTag.length + 1)
          if (!safeLength) return
          appendAssistantReasoning(renderBuffer.slice(0, safeLength))
          renderBuffer = renderBuffer.slice(safeLength)
          return
        }

        const openIndex = renderBuffer.indexOf(openTag)
        if (openIndex !== -1) {
          appendAssistantContent(renderBuffer.slice(0, openIndex))
          renderBuffer = renderBuffer.slice(openIndex + openTag.length)
          inThinkBlock = true
          continue
        }

        const safeLength = force ? renderBuffer.length : Math.max(0, renderBuffer.length - openTag.length + 1)
        if (!safeLength) return
        appendAssistantContent(renderBuffer.slice(0, safeLength))
        renderBuffer = renderBuffer.slice(safeLength)
        return
      }
    }

    consumeAssistantText = (rawChunk: string, force = false) => {
      if (rawChunk) {
        renderBuffer += rawChunk
      }
      flushAssistantRenderBuffer(force)
    }

    const actionCloseMarker = '[[/ACTION]]'
    const actionOpenMarkers: Array<{ marker: string; mode: AgentWriterMode }> = [
      { marker: '[[ACTION:append]]', mode: 'append' },
      { marker: '[[ACTION:replace]]', mode: 'replace' },
      { marker: '[[ACTION:rewrite_section]]', mode: 'rewrite_section' },
      { marker: '[[ACTION:replace_block]]', mode: 'replace_block' },
    ]
    const actionMarkerLookbehind = Math.max(
      actionCloseMarker.length,
      ...actionOpenMarkers.map((item) => item.marker.length),
    )
    let actionProbe = ''

    const completeWriterBlock = () => {
      if (routeAction && routeAction !== 'chat' && props.docId && writerStarted) {
        dispatchAgentWriterComplete({ docId: props.docId })
        writerStarted = false
      }
      routeAction = 'chat'
    }

    const findNextActionOpen = (input: string) => {
      const upperInput = input.toUpperCase()
      const normalizedOpenMarkers = actionOpenMarkers.map((item) => ({
        ...item,
        upperMarker: item.marker.toUpperCase(),
      }))
      let best: { index: number; marker: string; mode: AgentWriterMode } | null = null
      for (const item of normalizedOpenMarkers) {
        const index = upperInput.indexOf(item.upperMarker)
        if (index === -1) continue
        if (!best || index < best.index) {
          best = { index, marker: item.upperMarker, mode: item.mode }
        }
      }
      return best
    }

    routeChunk = (rawChunk: string, force = false) => {
      if (rawChunk) {
        actionProbe += rawChunk
      }

      while (actionProbe) {
        if (routeAction && routeAction !== 'chat') {
          const closeIndex = actionProbe.toUpperCase().indexOf(actionCloseMarker)
          if (closeIndex !== -1) {
            const beforeClose = actionProbe.slice(0, closeIndex)
            if (beforeClose) {
              consumeAssistantText(beforeClose, true)
            }
            actionProbe = actionProbe.slice(closeIndex + actionCloseMarker.length)
            completeWriterBlock()
            continue
          }

          const safeLength = force
            ? actionProbe.length
            : Math.max(0, actionProbe.length - actionCloseMarker.length + 1)
          if (!safeLength) return

          consumeAssistantText(actionProbe.slice(0, safeLength), force)
          actionProbe = actionProbe.slice(safeLength)
          return
        }

        const nextOpen = findNextActionOpen(actionProbe)
        if (nextOpen) {
          const beforeOpen = actionProbe.slice(0, nextOpen.index)
          if (beforeOpen) {
            routeAction = 'chat'
            consumeAssistantText(beforeOpen, true)
          }
          actionProbe = actionProbe.slice(nextOpen.index + nextOpen.marker.length)
          routeAction = props.docType === 'doc' ? nextOpen.mode : 'chat'
          continue
        }

        const safeLength = force
          ? actionProbe.length
          : Math.max(0, actionProbe.length - actionMarkerLookbehind + 1)
        if (!safeLength) return

        routeAction = 'chat'
        consumeAssistantText(actionProbe.slice(0, safeLength), force)
        actionProbe = actionProbe.slice(safeLength)
        return
      }
    }

    handleAssistantChunk = (rawChunk: string) => {
      if (!rawChunk) return
      if (routeAction) {
        routeChunk(rawChunk)
        return
      }

      prefixProbe += rawChunk
      const trimmedProbe = prefixProbe.trimStart()
      if (!trimmedProbe) return
      if (trimmedProbe !== prefixProbe) {
        prefixProbe = trimmedProbe
      }

      if (prefixProbe === '[') {
        return
      }

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

        const upperMarker = marker.toUpperCase()

        if (upperMarker.startsWith('[[ROUTE:')) {
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

        if (upperMarker.startsWith('[[ACTION:')) {
          prefixProbe = ''
          routeAction = upperMarker === '[[ACTION:APPEND]]'
            ? 'append'
            : upperMarker === '[[ACTION:REPLACE]]'
              ? 'replace'
              : upperMarker === '[[ACTION:REWRITE_SECTION]]'
                ? 'rewrite_section'
                : upperMarker === '[[ACTION:REPLACE_BLOCK]]'
                  ? 'replace_block'
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
      assistantMessage.content = liveAssistantContent.value
      assistantMessage.reasoning = liveAssistantReasoning.value
      const requestMessages = buildConversationMessages(session.messages)
      const token = localStorage.getItem('token')
      const response = await fetch('/api/agent/chat/stream', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
        signal: abortController.signal,
        body: JSON.stringify(buildRequestBody(requestMessages, provider, session.model.trim(), {
          previousResponseId,
          toolOutputs: pendingToolOutputs,
        })),
      })

      pendingToolOutputs = null

      if (!response.ok) {
        const data = await response.json().catch(() => ({}))
        throw new Error(data.error || '智能体请求失败')
      }

      const reader = response.body?.getReader()
      if (!reader) throw new Error('流式响应不可用')

      const decoder = new TextDecoder('utf-8')
      let buffer = ''
      let cycleReceivedDelta = false
      let requiredToolCalls: AgentToolCall[] = []
      let toolResponseId = ''
      let streamDone = false

      const processParsedEvent = (parsed: ReturnType<typeof parseSseBlock>) => {
        if (!parsed) return

        if (parsed.event === 'message.delta') {
          cycleReceivedDelta = true
          const content = parsed.data.content || ''
          rawAssistantContent += content
          handleAssistantChunk(content)
          liveAssistantContent.value = buildVisibleAssistantContent(rawAssistantContent, true)
        } else if (parsed.event === 'reasoning.delta') {
          const delta = parsed.data.delta || parsed.data.content || ''
          if (delta) {
            appendAssistantReasoning(delta)
          }
        } else if (parsed.event === 'message.completed') {
          const completedContent = parsed.data.content || ''
          if (completedContent) {
            completedAssistantContent = completedContent
          }
          const responseId = typeof parsed.data.response_id === 'string' && parsed.data.response_id.trim()
            ? parsed.data.response_id.trim()
            : null
          if (responseId) {
            previousResponseId = responseId
          }
          appendCompletedTail(completedContent)
          liveAssistantContent.value = buildVisibleAssistantContent(completedAssistantContent || rawAssistantContent)
          streamDone = true
        } else if (parsed.event === 'agent.transport') {
          const mode = parsed.data.mode === 'chat_fallback' ? 'chat_fallback' : 'responses'
          agentTransportMode.value = mode
        } else if (parsed.event === 'tool.calls.required') {
          toolResponseId = typeof parsed.data.response_id === 'string' ? parsed.data.response_id : ''
          if (toolResponseId.trim()) {
            previousResponseId = toolResponseId.trim()
          }
          requiredToolCalls = Array.isArray(parsed.data.calls) ? parsed.data.calls : []
        } else if (parsed.event === 'done') {
          streamDone = true
        } else if (parsed.event === 'error') {
          throw new Error(parsed.data.error || '智能体流式请求失败')
        }
      }

      while (true) {
        const { value, done } = await reader.read()
        if (done) {
          break
        }

        buffer += decoder.decode(value, { stream: true })
        const blocks = buffer.split(/\r?\n\r?\n/)
        buffer = blocks.pop() || ''

        for (const block of blocks) {
          processParsedEvent(parseSseBlock(block))
          if (streamDone) break
        }

        if (streamDone) {
          break
        }
      }

      if (buffer.trim()) {
        processParsedEvent(parseSseBlock(buffer))
      }

      if (!requiredToolCalls.length) {
        break
      }

      if (!toolResponseId) {
        throw new Error('模型请求了工具调用，但缺少 response_id')
      }

      pendingToolOutputs = await executeAgentToolCalls(requiredToolCalls)
    }
  } catch (error: any) {
    if (error?.name === 'AbortError') {
      streamAborted = true
      ElMessage.info('已停止生成')
    } else {
      streamFailed = true
      ElMessage.error(error.message || '智能体请求失败')
    }
  } finally {
    activeStreamController = null
    if (!streamFailed) {
      try {
        if (!routeAction && prefixProbe.trim()) {
          handleAssistantChunk(prefixProbe)
        }
        routeChunk('', true)
        consumeAssistantText('', true)
        recoverTrailingActionMarker()
        const finalizedSource = completedAssistantContent || rawAssistantContent
        liveAssistantContent.value = buildVisibleAssistantContent(finalizedSource)
        appendUnsavedDraftNotice()
        if (routeAction && routeAction !== 'chat' && props.docId && (!streamAborted || writerStarted)) {
          dispatchAgentWriterComplete({ docId: props.docId })
        }
      } catch (finalizeError) {
        console.error('agent stream finalize failed', finalizeError)
      }
    }
    assistantMessage.content = liveAssistantContent.value
    assistantMessage.reasoning = liveAssistantReasoning.value
    session.previousResponseId = previousResponseId
    session.lastSyncedMessageCount = 0
    session.updatedAt = Date.now()
    streaming.value = false
    streamingAssistantId.value = ''
    liveAssistantContent.value = ''
    liveAssistantReasoning.value = ''
    sessions.value = [...sessions.value]
    persistSessions()
  }
}

watch(
  () => currentSession.value?.messages.length,
  () => {
    scrollMessagesToBottom()
  },
)

watch(currentSessionId, () => {
  scrollMessagesToBottom()
})

watch([collapsed, panelX, panelY, currentSessionId], () => {
  const clamped = clampPanelPosition(panelX.value, panelY.value, collapsed.value)
  if (clamped.x !== panelX.value) panelX.value = clamped.x
  if (clamped.y !== panelY.value) panelY.value = clamped.y
  if (collapsed.value) {
    collapsedPanelX.value = panelX.value
    collapsedPanelY.value = panelY.value
  } else {
    expandedPanelX.value = panelX.value
    expandedPanelY.value = panelY.value
  }
  persistPanelState()
})

watch([activeProviderId, currentSessionId, activeModelKey], () => {
  syncSessionWithActiveProvider(currentSession.value)
})

watch(showProviderDialog, (visible) => {
  if (!visible) return
  ensureProviderDraftLoaded()
})

watch(showModelDialog, (visible) => {
  customModelInput.value = ''
  modelSearchQuery.value = ''
  if (!visible) return
  resetModelDraft()
})

onMounted(async () => {
  mounted.value = true

  const panelState = loadJson(PANEL_STATE_KEY, defaultPanelState(true))
  const defaultExpanded = clampPanelPosition(
    Number.isFinite(panelState.expandedPanelX) ? panelState.expandedPanelX : defaultPanelState(false).panelX,
    Number.isFinite(panelState.expandedPanelY) ? panelState.expandedPanelY : defaultPanelState(false).panelY,
    false,
  )
  const defaultCollapsed = clampPanelPosition(
    Number.isFinite(panelState.collapsedPanelX) ? panelState.collapsedPanelX : defaultPanelState(true).panelX,
    Number.isFinite(panelState.collapsedPanelY) ? panelState.collapsedPanelY : defaultPanelState(true).panelY,
    true,
  )
  collapsed.value = Boolean(panelState.collapsed)
  expandedPanelX.value = defaultExpanded.x
  expandedPanelY.value = defaultExpanded.y
  collapsedPanelX.value = defaultCollapsed.x
  collapsedPanelY.value = defaultCollapsed.y
  const initialPosition = collapsed.value ? defaultCollapsed : defaultExpanded
  panelX.value = initialPosition.x
  panelY.value = initialPosition.y

  try {
    await refreshProvidersState()
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '加载供应商失败')
  }
  sessions.value = loadSessions(activeProvider.value)

  if (!sessions.value.length) {
    createSession()
  } else {
    currentSessionId.value = panelState.currentSessionId && sessions.value.some((session) => session.id === panelState.currentSessionId)
      ? panelState.currentSessionId
      : sessions.value[0].id
  }

  ensureProviderDraftLoaded()
  syncSessionWithActiveProvider(currentSession.value)
  persistSessions()
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
  width: 520px;
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
  height: min(720px, calc(100vh - 96px));
  min-height: 460px;
  max-height: calc(100vh - 96px);
  border-radius: 20px;
  overflow: hidden;
  border: 1px solid rgba(122, 147, 91, 0.22);
  background:
    radial-gradient(circle at top right, rgba(215, 227, 196, 0.72), transparent 28%),
    linear-gradient(180deg, rgba(251, 252, 247, 0.96), rgba(246, 248, 239, 0.94));
  backdrop-filter: blur(16px);
  box-shadow: 0 32px 72px rgba(67, 86, 50, 0.16);
}

.agent-header {
  display: block;
  padding: 14px 16px 12px;
  border-bottom: 1px solid rgba(122, 147, 91, 0.16);
  cursor: move;
}

.agent-header-content {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.agent-meta-line,
.agent-session-line {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.agent-meta-line {
  flex: 1;
  font-size: 12px;
  color: #708067;
}

.agent-meta-label {
  color: #607057;
  flex-shrink: 0;
}

.agent-meta-value {
  color: #24311f;
  min-width: 0;
}

.agent-meta-url {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.agent-toolbar-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.agent-meta-divider {
  color: #9ca994;
}

.agent-header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  flex-wrap: nowrap;
}

.agent-session-line {
  flex: 1;
}

.agent-session-select {
  width: min(240px, 100%);
}

.agent-header-icon,
.agent-icon-btn {
  --el-button-bg-color: rgba(255, 255, 255, 0.78);
  --el-button-border-color: rgba(122, 147, 91, 0.24);
  --el-button-text-color: #607057;
  --el-button-hover-bg-color: rgba(232, 240, 220, 0.92);
  --el-button-hover-border-color: rgba(111, 154, 79, 0.34);
  --el-button-hover-text-color: #24311f;
  --el-button-disabled-bg-color: rgba(255, 255, 255, 0.68);
  --el-button-disabled-border-color: rgba(122, 147, 91, 0.14);
  --el-button-disabled-text-color: rgba(96, 112, 87, 0.45);
}

.header-btn,
.session-create,
.ghost-action,
.primary-action {
  height: 32px;
  border-radius: 10px;
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
  min-width: 72px;
  border: none;
  background: linear-gradient(135deg, #5d7f3f, #87a948);
  color: #f8fce9;
}

.header-btn:hover,
.session-create:hover,
.ghost-action:hover {
  background: rgba(232, 240, 220, 0.92);
}

.agent-main {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
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
  gap: 18px;
}

.agent-messages::-webkit-scrollbar,
.provider-list::-webkit-scrollbar,
.model-pane-scroll::-webkit-scrollbar {
  width: 10px;
}

.agent-messages::-webkit-scrollbar-track,
.provider-list::-webkit-scrollbar-track,
.model-pane-scroll::-webkit-scrollbar-track {
  background: rgba(122, 147, 91, 0.12);
  border-radius: 999px;
}

.agent-messages::-webkit-scrollbar-thumb,
.provider-list::-webkit-scrollbar-thumb,
.model-pane-scroll::-webkit-scrollbar-thumb {
  background: rgba(111, 154, 79, 0.58);
  border-radius: 999px;
  border: 2px solid rgba(246, 248, 239, 0.9);
}

.agent-empty {
  margin: auto 0;
  padding: 18px 20px;
  border-radius: 16px;
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

.agent-mode-tip-warning {
  color: #a0552a;
}

.agent-message {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.message-role {
  font-size: 14px;
  font-weight: 700;
  color: #24311f;
}

.message-content {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: inherit;
  font-size: 15px;
  line-height: 1.75;
  color: #24311f;
  background: transparent;
}

.message-reasoning {
  width: 100%;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.84);
  border: 1px solid rgba(122, 147, 91, 0.14);
  overflow: hidden;
}

.message-reasoning summary {
  list-style: none;
  cursor: pointer;
  user-select: none;
  padding: 10px 12px;
  font-size: 13px;
  font-weight: 700;
  color: #607057;
}

.message-reasoning summary::-webkit-details-marker {
  display: none;
}

.message-reasoning summary::before {
  content: '▸';
  margin-right: 8px;
  color: #6f9a4f;
}

.message-reasoning[open] summary::before {
  content: '▾';
}

.message-reasoning-content {
  margin: 0;
  padding: 0 12px 12px;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: inherit;
  font-size: 13px;
  line-height: 1.7;
  color: #708067;
}

.agent-composer {
  flex-shrink: 0;
  border-top: 1px solid rgba(122, 147, 91, 0.12);
  padding: 14px 16px 16px;
  background: rgba(251, 252, 247, 0.94);
}

.agent-mode-tip {
  font-size: 12px;
  color: #7b8771;
  line-height: 1.7;
}

.agent-textarea {
  width: 100%;
  margin-top: 12px;
  min-height: 126px;
  resize: none;
  border-radius: 16px;
  border: 1px solid rgba(122, 147, 91, 0.18);
  background: rgba(255, 255, 255, 0.92);
  color: #1d2719;
  padding: 12px 14px;
  font: inherit;
  line-height: 1.7;
}

.agent-textarea::placeholder {
  color: #87937e;
}

.agent-textarea:focus {
  outline: none;
  border-color: rgba(111, 154, 79, 0.46);
  box-shadow: 0 0 0 3px rgba(111, 154, 79, 0.12);
}

.agent-composer-footer {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 10px;
  margin-top: 12px;
}

.agent-shortcut-tip {
  font-size: 12px;
  color: #7b8771;
  line-height: 1.6;
}

.agent-bottom-bar {
  display: flex;
  align-items: center;
  gap: 12px;
}

.agent-controls {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.agent-inline-selects {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 10px;
}

.agent-select-row {
  display: grid;
  grid-template-columns: 48px minmax(0, 1fr);
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.agent-control-label {
  font-size: 12px;
  color: #607057;
  text-align: right;
}

.agent-provider-select,
.agent-model-select,
.agent-session-select {
  flex: 1;
  min-width: 0;
}

.agent-action-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.session-select-option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.session-select-info,
.session-select-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.session-select-info {
  flex: 1;
}

.session-select-title,
.session-select-meta,
.session-select-time {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.session-select-meta {
  color: #7f8b76;
  font-size: 12px;
}

.session-select-time {
  color: #8a957f;
  font-size: 11px;
}

.session-select-delete {
  flex-shrink: 0;
  color: #b45f52;
  font-size: 11px;
  cursor: pointer;
}

.provider-manager {
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
  gap: 16px;
}

.provider-list-pane {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.provider-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-height: 360px;
  overflow: auto;
  scrollbar-width: thin;
}

.provider-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  align-items: flex-start;
  padding: 12px;
  border-radius: 14px;
  border: 1px solid rgba(122, 147, 91, 0.14);
  background: rgba(248, 250, 242, 0.9);
  color: #51604a;
  text-align: left;
  cursor: pointer;
}

.provider-item.active {
  border-color: rgba(111, 154, 79, 0.38);
  background: rgba(232, 240, 220, 0.92);
}

.provider-item-name {
  font-size: 13px;
  font-weight: 700;
  color: #24311f;
}

.provider-item-tag {
  margin-left: 6px;
  display: inline-flex;
  align-items: center;
  padding: 1px 6px;
  border-radius: 999px;
  background: rgba(83, 117, 53, 0.14);
  color: #537535;
  font-size: 10px;
  font-weight: 700;
}

.provider-item-meta {
  font-size: 11px;
  line-height: 1.5;
  color: #708067;
  word-break: break-word;
}

.provider-editor {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.provider-hint {
  font-size: 12px;
  color: #708067;
  line-height: 1.6;
}

.model-manager {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.model-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 14px;
}

.model-pane {
  display: flex;
  flex-direction: column;
  min-height: 0;
  border-radius: 16px;
  border: 1px solid rgba(122, 147, 91, 0.14);
  background: rgba(248, 250, 242, 0.78);
}

.model-pane-head {
  padding: 12px 12px 10px;
  border-bottom: 1px solid rgba(122, 147, 91, 0.1);
}

.model-pane-head-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 10px;
}

.model-pane-body {
  padding: 12px;
}

.model-pane-foot {
  padding: 0 12px 12px;
}

.model-pane-scroll {
  min-height: 320px;
  max-height: 320px;
  overflow: auto;
  scrollbar-width: thin;
  scrollbar-color: rgba(111, 154, 79, 0.58) rgba(122, 147, 91, 0.12);
}

.model-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.model-header-title {
  font-size: 14px;
  font-weight: 700;
  color: #24311f;
}

.model-header-meta {
  margin-top: 4px;
  font-size: 12px;
  color: #708067;
  line-height: 1.6;
}

.model-custom-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 10px;
}

.model-section-title {
  font-size: 13px;
  font-weight: 700;
  color: #24311f;
}

.model-search-input {
  min-width: 0;
}

.current-model-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.current-model-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.88);
  border: 1px solid rgba(122, 147, 91, 0.12);
}

.current-model-main {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
}

.current-model-name {
  min-width: 0;
  font-size: 13px;
  font-weight: 600;
  color: #24311f;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.current-model-actions {
  flex-shrink: 0;
}

.model-source-badge {
  display: inline-flex;
  align-items: center;
  padding: 2px 6px;
  border-radius: 999px;
  background: rgba(83, 117, 53, 0.14);
  color: #537535;
  font-size: 10px;
  font-weight: 700;
}

.model-source-badge.is-custom {
  background: rgba(37, 99, 235, 0.12);
  color: #2563eb;
}

.model-check-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.model-check-item {
  display: flex;
  align-items: center;
  min-height: 32px;
  color: #51604a;
}

.model-empty,
.model-empty-state {
  font-size: 12px;
  line-height: 1.6;
  color: #708067;
}

.model-empty-state {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 12px;
}

:deep(.provider-dialog .el-dialog),
:deep(.model-dialog .el-dialog) {
  background: linear-gradient(180deg, rgba(251, 252, 247, 0.98), rgba(246, 248, 239, 0.96));
  border: 1px solid rgba(122, 147, 91, 0.18);
  box-shadow: 0 24px 60px rgba(67, 86, 50, 0.18);
}

:deep(.provider-dialog .el-dialog__title),
:deep(.model-dialog .el-dialog__title) {
  color: #24311f;
}

:deep(.provider-dialog .el-dialog__header),
:deep(.model-dialog .el-dialog__header) {
  margin-right: 0;
  padding-bottom: 8px;
  border-bottom: 1px solid rgba(122, 147, 91, 0.1);
}

:deep(.provider-dialog .el-input__wrapper),
:deep(.provider-dialog .el-select__wrapper),
:deep(.model-dialog .el-input__wrapper),
:deep(.model-dialog .el-select__wrapper),
.agent-shell :deep(.el-input__wrapper),
.agent-shell :deep(.el-select__wrapper) {
  background: rgba(255, 255, 255, 0.92);
  box-shadow: 0 0 0 1px rgba(122, 147, 91, 0.14) inset;
}

:deep(.provider-dialog .el-input__inner),
:deep(.provider-dialog .el-select__selected-item),
:deep(.model-dialog .el-input__inner),
:deep(.model-dialog .el-select__selected-item),
.agent-shell :deep(.el-input__inner),
.agent-shell :deep(.el-select__selected-item) {
  color: #24311f;
}

:deep(.provider-dialog .el-input__inner::placeholder),
:deep(.model-dialog .el-input__inner::placeholder),
.agent-shell :deep(.el-input__inner::placeholder) {
  color: #87937e;
}

:deep(.provider-dialog .el-button),
:deep(.model-dialog .el-button) {
  --el-button-bg-color: rgba(255, 255, 255, 0.78);
  --el-button-border-color: rgba(122, 147, 91, 0.18);
  --el-button-text-color: #607057;
}

.model-check-list :deep(.el-checkbox__label) {
  color: #51604a;
}

@media (max-width: 1200px) {
  .agent-panel {
    width: min(500px, calc(100vw - 24px));
  }
}

@media (max-width: 720px) {
  .agent-panel {
    width: calc(100vw - 24px);
  }

  .agent-shell {
    height: min(640px, calc(100vh - 84px));
    min-height: 420px;
    max-height: calc(100vh - 84px);
  }

  .agent-inline-selects {
    grid-template-columns: 1fr;
  }

  .agent-toolbar-line,
  .agent-bottom-bar {
    flex-direction: column;
    align-items: stretch;
  }

  .agent-session-line,
  .agent-session-select,
  .agent-controls {
    width: 100%;
  }

  .provider-manager,
  .model-grid,
  .model-pane-head-row {
    grid-template-columns: 1fr;
  }
}
</style>
