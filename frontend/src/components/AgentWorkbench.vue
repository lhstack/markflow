<template>
  <section class="agent-workbench">
    <div class="workbench-shell">
      <header class="workbench-topbar">
        <div class="session-group">
          <span class="topbar-label">会话</span>
          <select v-model="currentSessionId" class="workbench-select session-select">
            <option v-for="session in sessions" :key="session.id" :value="session.id">
              {{ session.title }}
            </option>
          </select>
          <button class="icon-button" type="button" @click="createSession">+</button>
          <button
            class="icon-button muted-button"
            type="button"
            :disabled="sessions.length <= 1"
            @click="removeCurrentSession"
          >
            -
          </button>
        </div>

        <div class="control-row">
          <div class="control-item">
            <span class="topbar-label">供应方</span>
            <select v-model="selectedProviderId" class="workbench-select" @change="handleProviderChange">
              <option v-for="provider in providers" :key="provider.id" :value="String(provider.id)">
                {{ provider.name }}
              </option>
            </select>
          </div>

          <div class="control-item">
            <span class="topbar-label">模型</span>
            <select v-model="selectedModel" class="workbench-select">
              <option v-for="model in currentModels" :key="model" :value="model">
                {{ model }}
              </option>
            </select>
          </div>

          <div class="control-item">
            <span class="topbar-label">模式</span>
            <select v-model="currentSession.mode" class="workbench-select">
              <option value="conversation">Conversation</option>
              <option value="loop">Loop</option>
            </select>
          </div>

          <button class="icon-button muted-button" type="button" @click="openProviderDialog">
            ⚙
          </button>
        </div>
      </header>

      <div class="workbench-body">
        <aside class="workbench-rail">
          <section class="rail-card">
            <div class="rail-title">
              <span>计划</span>
              <span class="pill" :class="currentSession.mode">{{ currentSession.mode }}</span>
            </div>
            <div v-if="currentSession.plan.length" class="plan-list">
              <div
                v-for="step in currentSession.plan"
                :key="step.id"
                class="plan-step"
                :class="step.status"
              >
                <span class="plan-dot"></span>
                <span class="plan-text">{{ step.title }}</span>
              </div>
            </div>
            <p v-else class="rail-empty">Loop 模式下会在这里展示运行中的计划步骤。</p>
          </section>

          <section class="rail-card">
            <div class="rail-title">
              <span>活动</span>
              <span class="pill muted">{{ currentSession.toolEvents.length }}</span>
            </div>
            <div v-if="currentSession.toolEvents.length" class="tool-list">
              <div v-for="event in currentSession.toolEvents" :key="event.id" class="tool-entry">
                <div class="tool-name">{{ event.name }}</div>
                <div class="tool-summary">{{ event.summary }}</div>
              </div>
            </div>
            <p v-else class="rail-empty">工具调用和运行过程会按时间顺序显示在这里。</p>
          </section>

          <section class="rail-card">
            <div class="rail-title">
              <span>模型配置</span>
              <span class="pill muted">{{ selectedModel || '未选' }}</span>
            </div>
            <div v-if="selectedModelConfig" class="config-list">
              <div class="config-row">
                <span>模态</span>
                <span>{{ selectedModelConfig.modalities.join(' / ') || 'text' }}</span>
              </div>
              <div class="config-row">
                <span>Thinking</span>
                <span>{{ selectedModelConfig.thinking ? '开' : '关' }}</span>
              </div>
              <div class="config-row">
                <span>Tools</span>
                <span>{{ selectedModelConfig.tools_enabled ? '开' : '关' }}</span>
              </div>
              <div class="config-row">
                <span>Temperature</span>
                <span>{{ selectedModelConfig.temperature ?? '默认' }}</span>
              </div>
              <div class="config-row">
                <span>Max Tokens</span>
                <span>{{ selectedModelConfig.max_output_tokens ?? '默认' }}</span>
              </div>
            </div>
            <p v-else class="rail-empty">当前模型还没有单独配置，默认按 provider 基础能力运行。</p>
          </section>
        </aside>

        <div class="chat-panel">
          <div ref="messagesEl" class="messages">
            <div v-for="message in currentSession.messages" :key="message.id" class="message" :class="message.role">
              <div class="message-role">{{ message.role === 'user' ? '用户' : message.role === 'assistant' ? '助手' : '系统' }}</div>
              <div class="message-content">{{ message.content }}</div>
            </div>
            <div v-if="currentSession.running" class="message system">
              <div class="message-role">系统</div>
              <div class="message-content">正在执行中，新的工具事件和结果会持续更新。</div>
            </div>
          </div>

          <div class="composer">
            <textarea
              v-model="draft"
              class="composer-input"
              placeholder="直接告诉我目标。我会用新的 agents-sdk runtime 来处理对话或 loop。"
              @keydown.enter.exact.prevent="sendMessage"
            />
            <div class="composer-actions">
              <div class="context-note">
                当前上下文：
                <template v-if="projectName">{{ projectName }}</template>
                <template v-if="docName"> / {{ docName }}</template>
                <template v-if="!projectName && !docName">无打开项目</template>
              </div>
              <button
                v-if="props.docId"
                class="send-button secondary"
                type="button"
                :disabled="docStreamDisabled"
                @click="streamToDocument"
              >
                {{ currentSession.running ? '执行中' : '写入文档' }}
              </button>
              <button class="send-button" type="button" :disabled="sendDisabled" @click="sendMessage">
                {{ currentSession.running ? '执行中' : '发送' }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <el-dialog v-model="providerDialogVisible" title="Agent Provider" width="980px" append-to-body>
      <div class="provider-dialog">
        <aside class="provider-list">
          <button class="provider-new" type="button" @click="startCreateProvider">新建 Provider</button>
          <button
            v-for="provider in providers"
            :key="provider.id"
            class="provider-item"
            :class="{ active: editingProviderId === provider.id }"
            type="button"
            @click="selectProviderForEdit(provider.id)"
          >
            <div class="provider-item-name">{{ provider.name }}</div>
            <div class="provider-item-meta">{{ provider.provider_kind }} · {{ provider.base_url }}</div>
          </button>
        </aside>

        <div class="provider-form">
          <div class="provider-form-grid">
            <label class="field">
              <span>名称</span>
              <input v-model="providerForm.name" class="field-input" />
            </label>
            <label class="field">
              <span>类型</span>
              <select v-model="providerForm.provider_kind" class="field-input">
                <option value="openai">OpenAI / Compatible</option>
                <option value="anthropic">Anthropic / Claude</option>
                <option value="gemini">Gemini</option>
              </select>
            </label>
            <label class="field full">
              <span>Base URL</span>
              <input v-model="providerForm.base_url" class="field-input" />
            </label>
            <label class="field full">
              <span>API Key</span>
              <input v-model="providerForm.api_key" class="field-input" type="password" placeholder="留空表示沿用已有密钥" />
            </label>
            <label class="field full">
              <span>远程模型</span>
              <textarea v-model="providerForm.remote_models_text" class="field-textarea" placeholder="可从接口同步或手动补充，每行一个模型名" />
            </label>
            <label class="field full">
              <span>可用模型</span>
              <textarea v-model="providerForm.enabled_models_text" class="field-textarea" placeholder="每行一个模型名" />
            </label>
            <label class="field full">
              <span>自定义模型</span>
              <textarea v-model="providerForm.custom_models_text" class="field-textarea" placeholder="每行一个模型名" />
            </label>
          </div>

          <div class="model-config-editor">
            <div class="editor-header">
              <span>模型级配置</span>
              <div class="editor-tools">
                <select v-model="editingModelName" class="field-input compact-select">
                  <option value="">选择模型</option>
                  <option v-for="model in providerEditableModels" :key="model" :value="model">
                    {{ model }}
                  </option>
                </select>
                <button class="small-button" type="button" :disabled="!editingModelName" @click="loadModelConfig">加载</button>
              </div>
            </div>

            <div v-if="editingModelName" class="model-config-grid">
              <label class="field full">
                <span>模态</span>
                <div class="checkbox-row">
                  <label v-for="modality in modalityOptions" :key="modality" class="checkbox-item">
                    <input
                      type="checkbox"
                      :checked="modelConfigDraft.modalities.includes(modality)"
                      @change="toggleModality(modality, ($event.target as HTMLInputElement).checked)"
                    />
                    <span>{{ modality }}</span>
                  </label>
                </div>
              </label>

              <label class="field">
                <span>Temperature</span>
                <input v-model="modelConfigDraft.temperatureText" class="field-input" placeholder="如 0.7" />
              </label>
              <label class="field">
                <span>Max Tokens</span>
                <input v-model="modelConfigDraft.maxTokensText" class="field-input" placeholder="如 4096" />
              </label>
              <label class="field checkbox-field">
                <input v-model="modelConfigDraft.thinking" type="checkbox" />
                <span>开启 Thinking</span>
              </label>
              <label class="field checkbox-field">
                <input v-model="modelConfigDraft.tools_enabled" type="checkbox" />
                <span>允许使用工具</span>
              </label>

              <div class="config-actions">
                <button class="small-button primary" type="button" @click="applyModelConfig">应用到当前 Provider</button>
                <button class="small-button muted-button" type="button" @click="removeModelConfig" :disabled="!providerForm.model_configs[editingModelName]">
                  删除当前模型配置
                </button>
              </div>
            </div>
          </div>

          <div class="provider-actions">
            <button class="small-button" type="button" @click="activateEditingProvider" :disabled="!editingProviderId">设为当前激活</button>
            <button class="small-button danger" type="button" @click="deleteEditingProvider" :disabled="!editingProviderId">删除</button>
            <button class="small-button primary" type="button" @click="saveProvider">保存 Provider</button>
          </div>
        </div>
      </div>
    </el-dialog>
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import request from '@/utils/request'
import { getAgentEditorBridge } from '@/utils/agentEditorBridge'
import {
  dispatchAgentWriterChunk,
  dispatchAgentWriterComplete,
  dispatchAgentWriterStart,
} from '@/utils/agentWriter'

interface ProviderModelConfig {
  modalities: string[]
  thinking: boolean
  temperature: number | null
  max_output_tokens: number | null
  tools_enabled: boolean
}

interface ProviderSummary {
  id: number
  name: string
  provider_kind: string
  base_url: string
  remote_models: string[]
  enabled_models: string[]
  custom_models: string[]
  model_configs: Record<string, ProviderModelConfig>
  is_active: boolean
}

interface ProviderDetail extends ProviderSummary {
  api_key: string
}

interface ChatMessage {
  id: string
  role: 'user' | 'assistant' | 'system'
  content: string
}

interface PlanStep {
  id: string
  title: string
  status: 'pending' | 'in_progress' | 'completed'
}

interface ToolEventItem {
  id: string
  name: string
  summary: string
}

interface SessionContext {
  project_id: number | null
  project_name: string | null
  doc_id: number | null
  doc_name: string | null
}

interface SessionState {
  id: string
  title: string
  mode: 'conversation' | 'loop'
  providerId: string
  model: string
  context: SessionContext
  messages: ChatMessage[]
  plan: PlanStep[]
  toolEvents: ToolEventItem[]
  running: boolean
  createdAt?: string
  updatedAt?: string
}

interface RemoteSessionState {
  id: string
  title: string
  mode: 'conversation' | 'loop' | string
  providerId?: number | null
  model?: string | null
  context?: SessionContext | null
  messages?: ChatMessage[]
  plan?: PlanStep[]
  toolEvents?: ToolEventItem[]
  createdAt?: string
  updatedAt?: string
}

interface ProviderFormState {
  id: number | null
  name: string
  provider_kind: string
  base_url: string
  api_key: string
  remote_models_text: string
  enabled_models_text: string
  custom_models_text: string
  model_configs: Record<string, ProviderModelConfig>
}

interface ModelConfigDraft {
  modalities: string[]
  thinking: boolean
  temperatureText: string
  maxTokensText: string
  tools_enabled: boolean
}

const props = defineProps<{
  projectId: number | null
  projectName: string
  docId: number | null
  docName: string
}>()

const emit = defineEmits<{
  navigate: [target: { kind: 'overview' | 'project' | 'doc'; projectId?: number; nodeId?: number; name?: string }]
}>()

const STORAGE_KEY = 'markflow.agent.workbench.sessions.fallback'
const STORAGE_SESSION_KEY = 'markflow.agent.workbench.current'
const modalityOptions = ['text', 'image', 'audio', 'video']
const providerKindDefaults: Record<string, { baseUrl: string; models: string[] }> = {
  openai: {
    baseUrl: 'https://api.openai.com/v1',
    models: ['gpt-4.1', 'gpt-4.1-mini', 'gpt-4o', 'gpt-4o-mini'],
  },
  anthropic: {
    baseUrl: 'https://api.anthropic.com/v1',
    models: ['claude-sonnet-4-0', 'claude-3-7-sonnet-latest', 'claude-3-5-haiku-latest'],
  },
  gemini: {
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta',
    models: ['gemini-2.5-pro', 'gemini-2.5-flash', 'gemini-2.0-flash'],
  },
}

function uid(prefix: string) {
  const random = typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
    ? crypto.randomUUID()
    : `${Date.now()}_${Math.random().toString(16).slice(2)}`
  return `${prefix}_${random}`
}

function parseNumberText(value: string): number | null {
  const trimmed = value.trim()
  if (!trimmed) return null
  const parsed = Number(trimmed)
  return Number.isFinite(parsed) ? parsed : null
}

function normalizeModelConfig(input?: Partial<ProviderModelConfig> | null): ProviderModelConfig {
  return {
    modalities: Array.isArray(input?.modalities) && input?.modalities.length ? [...new Set(input.modalities)] : ['text'],
    thinking: input?.thinking === true,
    temperature: typeof input?.temperature === 'number' ? input.temperature : null,
    max_output_tokens: typeof input?.max_output_tokens === 'number' ? input.max_output_tokens : null,
    tools_enabled: input?.tools_enabled !== false,
  }
}

function createModelConfigDraft(config?: Partial<ProviderModelConfig> | null): ModelConfigDraft {
  const normalized = normalizeModelConfig(config)
  return {
    modalities: [...normalized.modalities],
    thinking: normalized.thinking,
    temperatureText: normalized.temperature === null ? '' : String(normalized.temperature),
    maxTokensText: normalized.max_output_tokens === null ? '' : String(normalized.max_output_tokens),
    tools_enabled: normalized.tools_enabled,
  }
}

function createProviderForm(): ProviderFormState {
  return {
    id: null,
    name: '',
    provider_kind: 'openai',
    base_url: '',
    api_key: '',
    remote_models_text: '',
    enabled_models_text: '',
    custom_models_text: '',
    model_configs: {},
  }
}

function createEmptySession(providerId = '', model = ''): SessionState {
  return {
    id: uid('session'),
    title: '新会话',
    mode: 'conversation',
    providerId,
    model,
    context: createSessionContext(),
    messages: [],
    plan: [],
    toolEvents: [],
    running: false,
  }
}

function loadFallbackSessions(): SessionState[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as SessionState[]
    if (!Array.isArray(parsed)) return []
    return parsed.map((session) => ({
      ...session,
      providerId: session.providerId || '',
      model: session.model || '',
      mode: session.mode === 'loop' ? 'loop' : 'conversation',
      context: normalizeSessionContext(session.context),
      messages: Array.isArray(session.messages) ? session.messages : [],
      plan: Array.isArray(session.plan) ? session.plan : [],
      toolEvents: Array.isArray(session.toolEvents) ? session.toolEvents : [],
      running: false,
    }))
  } catch {
    return []
  }
}

function createSessionContext(): SessionContext {
  return {
    project_id: props.projectId,
    project_name: props.projectName || null,
    doc_id: props.docId,
    doc_name: props.docName || null,
  }
}

function normalizeSessionContext(input?: Partial<SessionContext> | null): SessionContext {
  return {
    project_id: typeof input?.project_id === 'number' ? input.project_id : null,
    project_name: typeof input?.project_name === 'string' && input.project_name.trim() ? input.project_name : null,
    doc_id: typeof input?.doc_id === 'number' ? input.doc_id : null,
    doc_name: typeof input?.doc_name === 'string' && input.doc_name.trim() ? input.doc_name : null,
  }
}

function normalizeRemoteSession(session: RemoteSessionState): SessionState {
  return {
    id: session.id,
    title: session.title || '新会话',
    mode: session.mode === 'loop' ? 'loop' : 'conversation',
    providerId: session.providerId ? String(session.providerId) : '',
    model: session.model || '',
    context: normalizeSessionContext(session.context),
    messages: Array.isArray(session.messages) ? session.messages : [],
    plan: Array.isArray(session.plan) ? session.plan : [],
    toolEvents: Array.isArray(session.toolEvents) ? session.toolEvents : [],
    running: false,
    createdAt: session.createdAt,
    updatedAt: session.updatedAt,
  }
}

function parseLines(value: string) {
  return value
    .split('\n')
    .map((item) => item.trim())
    .filter(Boolean)
}

function sessionPayload(session: SessionState) {
  return {
    title: session.title,
    mode: session.mode,
    provider_id: session.providerId ? Number(session.providerId) : null,
    model: session.model || null,
    context: session.context,
    messages: session.messages,
    plan: session.plan,
    tool_events: session.toolEvents,
  }
}

const providers = ref<ProviderSummary[]>([])
const sessions = ref<SessionState[]>([])
const currentSessionId = ref(localStorage.getItem(STORAGE_SESSION_KEY) || '')
const draft = ref('')
const messagesEl = ref<HTMLDivElement | null>(null)
const providerDialogVisible = ref(false)
const editingProviderId = ref<number | null>(null)
const providerForm = ref<ProviderFormState>(createProviderForm())
const editingModelName = ref('')
const modelConfigDraft = ref<ModelConfigDraft>(createModelConfigDraft())
let sessionSaveTimer: ReturnType<typeof setTimeout> | null = null

const currentSession = computed({
  get() {
    let session = sessions.value.find((item) => item.id === currentSessionId.value) || null
    if (!session) {
      const defaultProviderId = providers.value.find((provider) => provider.is_active)?.id
      const defaultModel = defaultProviderId
        ? providerModels(defaultProviderId).at(0) || ''
        : ''
      session = createEmptySession(defaultProviderId ? String(defaultProviderId) : '', defaultModel)
      sessions.value = [session, ...sessions.value]
      currentSessionId.value = session.id
      scheduleSessionPersist()
    }
    return session
  },
  set(next) {
    sessions.value = sessions.value.map((item) => (item.id === next.id ? next : item))
  },
})

const selectedProviderId = computed({
  get: () => currentSession.value.providerId,
  set: (value: string) => {
    currentSession.value.providerId = value
    if (!providerModels(Number(value)).includes(currentSession.value.model)) {
      currentSession.value.model = providerModels(Number(value))[0] || ''
    }
    scheduleSessionPersist()
  },
})

const selectedModel = computed({
  get: () => currentSession.value.model,
  set: (value: string) => {
    currentSession.value.model = value
    scheduleSessionPersist()
  },
})

const currentModels = computed(() => providerModels(Number(selectedProviderId.value)))

const selectedProvider = computed(() =>
  providers.value.find((provider) => String(provider.id) === selectedProviderId.value) || null
)

const selectedModelConfig = computed(() => {
  const provider = selectedProvider.value
  if (!provider || !selectedModel.value) return null
  return provider.model_configs?.[selectedModel.value] || null
})

const providerEditableModels = computed(() => {
  const models = [
    ...parseLines(providerForm.value.remote_models_text),
    ...parseLines(providerForm.value.enabled_models_text),
    ...parseLines(providerForm.value.custom_models_text),
    ...Object.keys(providerForm.value.model_configs),
  ]
  return Array.from(new Set(models))
})

const sendDisabled = computed(() =>
  !draft.value.trim() || !selectedProviderId.value || !selectedModel.value || currentSession.value.running
)

const docStreamDisabled = computed(() => {
  if (!props.docId) return true
  if (!draft.value.trim() || !selectedProviderId.value || !selectedModel.value || currentSession.value.running) {
    return true
  }
  const bridge = getAgentEditorBridge()
  return !bridge || bridge.docId !== props.docId
})

function providerModels(providerId: number) {
  const provider = providers.value.find((item) => item.id === providerId) || null
  if (!provider) return []
  return Array.from(new Set([
    ...(provider.enabled_models || []),
    ...(provider.custom_models || []),
    ...(provider.remote_models || []),
  ]))
}

function applyProviderKindDefaults(kind: string, options: { force?: boolean } = {}) {
  const preset = providerKindDefaults[kind] || providerKindDefaults.openai
  if (options.force || !providerForm.value.base_url.trim()) {
    providerForm.value.base_url = preset.baseUrl
  }
  const currentModels = [
    ...parseLines(providerForm.value.remote_models_text),
    ...parseLines(providerForm.value.enabled_models_text),
    ...parseLines(providerForm.value.custom_models_text),
  ]
  if (options.force || currentModels.length === 0) {
    providerForm.value.enabled_models_text = preset.models.join('\n')
  }
}

watch(currentSessionId, (value) => {
  localStorage.setItem(STORAGE_SESSION_KEY, value)
})

watch(
  () => [props.projectId, props.projectName, props.docId, props.docName],
  () => {
    currentSession.value.context = createSessionContext()
    scheduleSessionPersist()
  }
)

watch(
  () => currentSession.value.messages.length,
  async () => {
    await nextTick()
    const el = messagesEl.value
    if (el) {
      el.scrollTop = el.scrollHeight
    }
  }
)

watch(
  sessions,
  (value) => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(value))
    scheduleSessionPersist()
  },
  { deep: true }
)

watch(currentModels, (models) => {
  if (!models.length) {
    selectedModel.value = ''
    return
  }
  if (!models.includes(selectedModel.value)) {
    selectedModel.value = models[0]
  }
})

function scheduleSessionPersist() {
  if (sessionSaveTimer) {
    clearTimeout(sessionSaveTimer)
  }
  sessionSaveTimer = setTimeout(() => {
    void persistCurrentSession()
  }, 300)
}

async function loadProviders() {
  try {
    const data = await request.get('/agent/providers') as { providers?: ProviderSummary[]; active_provider_id?: number | null }
    providers.value = Array.isArray(data.providers) ? data.providers : []
    const activeId = data.active_provider_id ? String(data.active_provider_id) : ''
    if (!selectedProviderId.value) {
      selectedProviderId.value = activeId || (providers.value[0] ? String(providers.value[0].id) : '')
    }
  } catch (error) {
    console.error('agent workbench loadProviders failed', error)
    ElMessage.error('加载供应商失败')
  }
}

async function loadSessions() {
  try {
    const data = await request.get('/agent/sessions') as { sessions?: RemoteSessionState[] }
    const remoteSessions = Array.isArray(data.sessions) ? data.sessions : []
    if (remoteSessions.length) {
      sessions.value = remoteSessions.map(normalizeRemoteSession)
      currentSessionId.value = localStorage.getItem(STORAGE_SESSION_KEY) || sessions.value[0]?.id || ''
      return
    }
  } catch (error) {
    console.error('agent workbench loadSessions failed', error)
  }

  const fallback = loadFallbackSessions()
  if (fallback.length) {
    sessions.value = fallback
    currentSessionId.value = localStorage.getItem(STORAGE_SESSION_KEY) || fallback[0].id
    return
  }

  const defaultProviderId = providers.value.find((provider) => provider.is_active)?.id
  const defaultModel = defaultProviderId ? providerModels(defaultProviderId).at(0) || '' : ''
  const session = createEmptySession(defaultProviderId ? String(defaultProviderId) : '', defaultModel)
  sessions.value = [session]
  currentSessionId.value = session.id
  await persistCurrentSession()
}

async function persistCurrentSession() {
  const session = currentSession.value
  try {
    await request.put(`/agent/sessions/${session.id}`, sessionPayload(session))
  } catch (error) {
    console.error('agent workbench persistCurrentSession failed', error)
  }
}

function handleProviderChange() {
  const models = currentModels.value
  if (models.length && !models.includes(selectedModel.value)) {
    selectedModel.value = models[0]
  }
}

async function createSession() {
  const session = createEmptySession(selectedProviderId.value, selectedModel.value)
  sessions.value = [session, ...sessions.value]
  currentSessionId.value = session.id
  await persistCurrentSession()
}

async function removeCurrentSession() {
  if (sessions.value.length <= 1) return
  const sessionId = currentSession.value.id
  try {
    await ElMessageBox.confirm('确认删除当前会话？', '删除会话', {
      type: 'warning',
      confirmButtonText: '删除',
      cancelButtonText: '取消',
    })
  } catch {
    return
  }

  sessions.value = sessions.value.filter((session) => session.id !== sessionId)
  currentSessionId.value = sessions.value[0]?.id || ''
  try {
    await request.delete(`/agent/sessions/${sessionId}`)
  } catch (error) {
    console.error('agent workbench delete session failed', error)
  }
}

function pushMessage(role: ChatMessage['role'], content: string) {
  const session = currentSession.value
  session.messages.push({
    id: uid(role),
    role,
    content,
  })
  if (role === 'user' && session.title === '新会话') {
    session.title = content.slice(0, 18) || '新会话'
  }
}

function pushToolEvent(name: string, summary: string) {
  currentSession.value.toolEvents.unshift({
    id: uid('tool'),
    name,
    summary,
  })
}

function updatePlan(steps: PlanStep[]) {
  currentSession.value.plan = steps
}

function handleResourceEvent(payload: Record<string, unknown>) {
  const kind = typeof payload.kind === 'string' ? payload.kind : ''
  const projectId = typeof payload.project_id === 'number' ? payload.project_id : null
  const nodeId = typeof payload.node_id === 'number' ? payload.node_id : null
  if ((kind === 'project_created' || kind === 'project_opened' || kind === 'tree_node_created') && projectId) {
    emit('navigate', { kind: 'project', projectId })
    return
  }
  if ((kind === 'tree_node_opened' || kind === 'document_written') && projectId && nodeId) {
    emit('navigate', { kind: 'doc', projectId, nodeId })
  }
}

async function consumeSseStream(reader: ReadableStreamDefaultReader<Uint8Array>) {
  const decoder = new TextDecoder('utf-8')
  let buffer = ''

  while (true) {
    const { value, done } = await reader.read()
    if (done) break
    buffer += decoder.decode(value, { stream: true })

    let boundary = buffer.indexOf('\n\n')
    while (boundary >= 0) {
      const rawEvent = buffer.slice(0, boundary)
      buffer = buffer.slice(boundary + 2)
      handleRawEvent(rawEvent)
      boundary = buffer.indexOf('\n\n')
    }
  }
}

function handleRawEvent(rawEvent: string) {
  const lines = rawEvent
    .split('\n')
    .map((line) => line.trimEnd())
    .filter(Boolean)

  let eventName = 'message'
  const dataLines: string[] = []
  for (const line of lines) {
    if (line.startsWith('event:')) {
      eventName = line.slice(6).trim()
      continue
    }
    if (line.startsWith('data:')) {
      dataLines.push(line.slice(5).trim())
    }
  }

  const rawData = dataLines.join('\n')
  let payload: Record<string, unknown> = {}
  if (rawData) {
    try {
      payload = JSON.parse(rawData)
    } catch (error) {
      console.error('agent workbench parse SSE payload failed', { eventName, rawData, error })
      return
    }
  }

  switch (eventName) {
    case 'plan':
      updatePlan(
        Array.isArray(payload.steps)
          ? payload.steps.map((step: any, index: number) => ({
              id: typeof step.id === 'string' ? step.id : `step_${index + 1}`,
              title: typeof step.title === 'string' ? step.title : typeof step.content === 'string' ? step.content : `步骤 ${index + 1}`,
              status: step.status === 'completed' || step.status === 'in_progress' ? step.status : 'pending',
            }))
          : []
      )
      break
    case 'tool_started':
      pushToolEvent(String(payload.tool_name || 'tool'), `开始执行：${String(payload.input_summary || '')}`)
      break
    case 'tool_completed':
      pushToolEvent(String(payload.tool_name || 'tool'), `已完成：${String(payload.result_summary || '')}`)
      break
    case 'tool_failed':
      pushToolEvent(String(payload.tool_name || 'tool'), `失败：${String(payload.error_message || '')}`)
      break
    case 'writer_start':
      dispatchAgentWriterStart({
        docId: Number(payload.doc_id || 0),
        mode: String(payload.mode || 'replace') as 'append' | 'replace',
        save: payload.save === true,
      })
      pushToolEvent('document_stream', '开始流式写入当前文档')
      break
    case 'writer_chunk':
      dispatchAgentWriterChunk({
        docId: Number(payload.doc_id || 0),
        chunk: String(payload.chunk || ''),
      })
      break
    case 'writer_complete':
      dispatchAgentWriterComplete({
        docId: Number(payload.doc_id || 0),
        payload,
      })
      pushToolEvent('document_stream', '已完成流式写入并触发保存')
      break
    case 'planning':
      pushToolEvent('planning', String(payload.action_summary || ''))
      break
    case 'assistant_message':
      pushMessage('assistant', String(payload.content || ''))
      break
    case 'error':
      pushMessage('system', `执行失败：${String(payload.message || '未知错误')}`)
      break
    case 'resource':
      handleResourceEvent(payload)
      break
    case 'done':
      currentSession.value.running = false
      scheduleSessionPersist()
      break
    default:
      break
  }
}

async function sendMessage() {
  if (sendDisabled.value) return

  const session = currentSession.value
  const content = draft.value.trim()
  const providerId = Number(selectedProviderId.value)
  const model = selectedModel.value
  if (!providerId || !model) return

  session.context = createSessionContext()
  pushMessage('user', content)
  draft.value = ''
  session.running = true
  scheduleSessionPersist()

  try {
    const token = localStorage.getItem('token')
    const response = await fetch('/api/agent/chat/stream', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify({
        session_id: session.id,
        input: content,
        agent_mode: session.mode,
        provider: {
          provider_id: providerId,
          model,
        },
        context: session.context,
      }),
    })

    if (!response.ok || !response.body) {
      const text = await response.text()
      throw new Error(text || `HTTP ${response.status}`)
    }

    await consumeSseStream(response.body.getReader())
  } catch (error) {
    console.error('agent workbench sendMessage failed', error)
    pushMessage('system', `发送失败：${error instanceof Error ? error.message : '未知错误'}`)
    session.running = false
    scheduleSessionPersist()
  }
}

async function streamToDocument() {
  if (docStreamDisabled.value || !props.docId) return

  const bridge = getAgentEditorBridge()
  if (!bridge || bridge.docId !== props.docId) {
    ElMessage.warning('当前没有可用的文档编辑器，无法写入')
    return
  }

  const session = currentSession.value
  const content = draft.value.trim()
  const providerId = Number(selectedProviderId.value)
  const model = selectedModel.value
  if (!providerId || !model) return

  session.context = createSessionContext()
  pushMessage('user', content)
  draft.value = ''
  session.running = true
  scheduleSessionPersist()

  try {
    const token = localStorage.getItem('token')
    const response = await fetch('/api/agent/document/stream', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify({
        session_id: session.id,
        input: content,
        provider: {
          provider_id: providerId,
          model,
        },
        doc_id: props.docId,
        doc_name: props.docName || null,
        current_content: bridge.getValue(),
        save: true,
      }),
    })

    if (!response.ok || !response.body) {
      const text = await response.text()
      throw new Error(text || `HTTP ${response.status}`)
    }

    await consumeSseStream(response.body.getReader())
  } catch (error) {
    console.error('agent workbench streamToDocument failed', error)
    pushMessage('system', `写入失败：${error instanceof Error ? error.message : '未知错误'}`)
    session.running = false
    scheduleSessionPersist()
  }
}

function openProviderDialog() {
  providerDialogVisible.value = true
  if (selectedProvider.value) {
    void selectProviderForEdit(selectedProvider.value.id)
  } else {
    startCreateProvider()
  }
}

function startCreateProvider() {
  editingProviderId.value = null
  providerForm.value = createProviderForm()
  applyProviderKindDefaults(providerForm.value.provider_kind, { force: true })
  editingModelName.value = ''
  modelConfigDraft.value = createModelConfigDraft()
}

async function selectProviderForEdit(providerId: number) {
  try {
    const detail = await request.get(`/agent/providers/${providerId}`) as ProviderDetail
    editingProviderId.value = providerId
    providerForm.value = {
      id: detail.id,
      name: detail.name,
      provider_kind: detail.provider_kind || 'openai',
      base_url: detail.base_url,
      api_key: '',
      remote_models_text: detail.remote_models.join('\n'),
      enabled_models_text: detail.enabled_models.join('\n'),
      custom_models_text: detail.custom_models.join('\n'),
      model_configs: Object.fromEntries(
        Object.entries(detail.model_configs || {}).map(([model, config]) => [model, normalizeModelConfig(config)])
      ),
    }
    editingModelName.value = providerEditableModels.value[0] || ''
    loadModelConfig()
  } catch (error) {
    console.error('agent workbench selectProviderForEdit failed', error)
    ElMessage.error('加载 provider 详情失败')
  }
}

function loadModelConfig() {
  modelConfigDraft.value = createModelConfigDraft(
    editingModelName.value ? providerForm.value.model_configs[editingModelName.value] : null
  )
}

function toggleModality(modality: string, enabled: boolean) {
  if (enabled) {
    modelConfigDraft.value.modalities = Array.from(new Set([...modelConfigDraft.value.modalities, modality]))
  } else {
    modelConfigDraft.value.modalities = modelConfigDraft.value.modalities.filter((item) => item !== modality)
    if (!modelConfigDraft.value.modalities.length) {
      modelConfigDraft.value.modalities = ['text']
    }
  }
}

function applyModelConfig() {
  if (!editingModelName.value) return
  providerForm.value.model_configs = {
    ...providerForm.value.model_configs,
    [editingModelName.value]: normalizeModelConfig({
      modalities: modelConfigDraft.value.modalities,
      thinking: modelConfigDraft.value.thinking,
      temperature: parseNumberText(modelConfigDraft.value.temperatureText),
      max_output_tokens: parseNumberText(modelConfigDraft.value.maxTokensText),
      tools_enabled: modelConfigDraft.value.tools_enabled,
    }),
  }
  ElMessage.success(`已更新模型 ${editingModelName.value} 的配置`)
}

function removeModelConfig() {
  if (!editingModelName.value) return
  const next = { ...providerForm.value.model_configs }
  delete next[editingModelName.value]
  providerForm.value.model_configs = next
  modelConfigDraft.value = createModelConfigDraft()
}

async function saveProvider() {
  const payload = {
    id: providerForm.value.id,
    name: providerForm.value.name,
    provider_kind: providerForm.value.provider_kind,
    base_url: providerForm.value.base_url,
    api_key: providerForm.value.api_key || undefined,
    remote_models: parseLines(providerForm.value.remote_models_text),
    enabled_models: parseLines(providerForm.value.enabled_models_text),
    custom_models: parseLines(providerForm.value.custom_models_text),
    model_configs: providerForm.value.model_configs,
  }

  if (!payload.name.trim()) {
    ElMessage.warning('Provider 名称不能为空')
    return
  }

  try {
    const data = await request.post('/agent/providers', payload) as { provider_id?: number }
    await loadProviders()
    if (data.provider_id) {
      await selectProviderForEdit(data.provider_id)
      if (!selectedProviderId.value) {
        selectedProviderId.value = String(data.provider_id)
      }
    }
    ElMessage.success('Provider 已保存')
  } catch (error) {
    console.error('agent workbench saveProvider failed', error)
    ElMessage.error('保存 Provider 失败')
  }
}

async function activateEditingProvider() {
  if (!editingProviderId.value) return
  try {
    await request.post(`/agent/providers/${editingProviderId.value}/activate`, {})
    await loadProviders()
    selectedProviderId.value = String(editingProviderId.value)
    ElMessage.success('已切换当前 Provider')
  } catch (error) {
    console.error('agent workbench activate provider failed', error)
    ElMessage.error('激活 Provider 失败')
  }
}

async function deleteEditingProvider() {
  if (!editingProviderId.value) return
  try {
    await ElMessageBox.confirm('确认删除这个 Provider？', '删除 Provider', {
      type: 'warning',
      confirmButtonText: '删除',
      cancelButtonText: '取消',
    })
  } catch {
    return
  }

  try {
    await request.delete(`/agent/providers/${editingProviderId.value}`)
    await loadProviders()
    startCreateProvider()
    ElMessage.success('Provider 已删除')
  } catch (error) {
    console.error('agent workbench delete provider failed', error)
    ElMessage.error('删除 Provider 失败')
  }
}

onMounted(async () => {
  await loadProviders()
  await loadSessions()
  handleProviderChange()
})

watch(
  () => providerForm.value.provider_kind,
  (kind, previousKind) => {
    if (!kind || kind === previousKind) return
    applyProviderKindDefaults(kind)
  }
)
</script>

<style scoped>
.agent-workbench {
  position: fixed;
  right: 28px;
  bottom: 28px;
  z-index: 1200;
  width: min(1120px, calc(100vw - 40px));
}

.workbench-shell {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 16px;
  border: 1px solid rgba(100, 116, 139, 0.22);
  border-radius: 26px;
  background:
    radial-gradient(circle at top left, rgba(207, 250, 254, 0.55), transparent 40%),
    radial-gradient(circle at top right, rgba(254, 249, 195, 0.45), transparent 32%),
    rgba(255, 255, 252, 0.96);
  box-shadow: 0 26px 70px rgba(15, 23, 42, 0.16);
  backdrop-filter: blur(18px);
}

.workbench-topbar,
.session-group,
.control-row,
.control-item,
.composer-actions,
.rail-title,
.provider-dialog,
.provider-actions,
.editor-header,
.editor-tools,
.checkbox-row {
  display: flex;
  align-items: center;
}

.workbench-topbar {
  justify-content: space-between;
  gap: 16px;
  flex-wrap: wrap;
}

.session-group,
.control-row {
  gap: 10px;
  flex-wrap: wrap;
}

.control-row {
  flex: 1;
  justify-content: flex-end;
}

.control-item {
  gap: 8px;
}

.topbar-label {
  font-size: 12px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: #64748b;
}

.workbench-select,
.field-input {
  min-width: 140px;
  height: 36px;
  padding: 0 12px;
  border: 1px solid rgba(148, 163, 184, 0.35);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.84);
  color: #0f172a;
}

.session-select {
  min-width: 220px;
}

.icon-button,
.send-button,
.small-button,
.provider-new,
.provider-item {
  border: none;
  cursor: pointer;
}

.icon-button,
.small-button,
.provider-new {
  border-radius: 999px;
}

.icon-button {
  width: 36px;
  height: 36px;
  background: linear-gradient(135deg, #166534, #84cc16);
  color: white;
  font-size: 18px;
}

.muted-button {
  background: rgba(148, 163, 184, 0.2);
  color: #334155;
}

.danger {
  background: rgba(239, 68, 68, 0.15);
  color: #991b1b;
}

.small-button {
  height: 36px;
  padding: 0 14px;
  background: rgba(148, 163, 184, 0.18);
  color: #334155;
  font-weight: 700;
}

.small-button.primary,
.send-button,
.provider-new {
  background: linear-gradient(135deg, #166534, #84cc16);
  color: white;
}

.send-button {
  min-width: 90px;
  height: 40px;
  padding: 0 18px;
  border-radius: 999px;
  font-weight: 700;
}

.send-button.secondary {
  background: linear-gradient(135deg, #0f766e, #22c55e);
}

.send-button:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.workbench-body {
  display: grid;
  grid-template-columns: 300px minmax(0, 1fr);
  gap: 14px;
  min-height: 560px;
}

.workbench-rail {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.rail-card,
.chat-panel {
  border: 1px solid rgba(148, 163, 184, 0.18);
  border-radius: 22px;
  background: rgba(255, 255, 255, 0.72);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.4);
}

.rail-card {
  padding: 14px;
}

.rail-title {
  justify-content: space-between;
  margin-bottom: 12px;
  font-weight: 700;
  color: #0f172a;
}

.pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 64px;
  height: 28px;
  padding: 0 12px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 700;
  text-transform: capitalize;
}

.pill.loop {
  background: rgba(14, 165, 233, 0.15);
  color: #0369a1;
}

.pill.conversation,
.pill.muted {
  background: rgba(148, 163, 184, 0.16);
  color: #475569;
}

.rail-empty,
.tool-summary,
.message-content,
.context-note {
  line-height: 1.6;
}

.plan-list,
.tool-list,
.config-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.plan-step {
  display: flex;
  align-items: center;
  gap: 10px;
  color: #334155;
}

.plan-dot {
  width: 10px;
  height: 10px;
  border-radius: 999px;
  background: #cbd5e1;
  flex-shrink: 0;
}

.plan-step.in_progress .plan-dot {
  background: #0ea5e9;
}

.plan-step.completed .plan-dot {
  background: #16a34a;
}

.tool-entry {
  padding: 10px 12px;
  border-radius: 16px;
  background: rgba(248, 250, 252, 0.92);
}

.tool-name,
.provider-item-name {
  font-weight: 700;
  color: #0f172a;
}

.config-row {
  display: flex;
  justify-content: space-between;
  gap: 10px;
  color: #334155;
}

.chat-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.messages {
  flex: 1;
  min-height: 0;
  max-height: 540px;
  overflow-y: auto;
  padding: 18px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.message {
  padding: 14px 16px;
  border-radius: 20px;
  max-width: 90%;
  white-space: pre-wrap;
  word-break: break-word;
}

.message.user {
  align-self: flex-end;
  background: linear-gradient(135deg, #0f766e, #22c55e);
  color: white;
}

.message.assistant {
  align-self: flex-start;
  background: rgba(241, 245, 249, 0.96);
  color: #0f172a;
}

.message.system {
  align-self: center;
  background: rgba(254, 249, 195, 0.7);
  color: #713f12;
}

.message-role {
  margin-bottom: 6px;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  opacity: 0.7;
}

.composer {
  padding: 16px;
  border-top: 1px solid rgba(148, 163, 184, 0.18);
}

.composer-input,
.field-textarea {
  width: 100%;
  padding: 14px 16px;
  border: 1px solid rgba(148, 163, 184, 0.3);
  border-radius: 20px;
  background: rgba(255, 255, 255, 0.94);
  outline: none;
  font: inherit;
  color: #0f172a;
}

.composer-input {
  min-height: 110px;
  resize: vertical;
}

.composer-actions {
  justify-content: space-between;
  gap: 16px;
  margin-top: 12px;
}

.provider-dialog {
  min-height: 520px;
  gap: 18px;
}

.provider-list {
  width: 250px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.provider-new {
  height: 40px;
}

.provider-item {
  padding: 12px 14px;
  border-radius: 16px;
  background: rgba(248, 250, 252, 0.92);
  text-align: left;
}

.provider-item.active {
  outline: 2px solid rgba(22, 163, 74, 0.4);
}

.provider-item-meta {
  margin-top: 4px;
  color: #64748b;
  font-size: 12px;
  word-break: break-all;
}

.provider-form {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.provider-form-grid,
.model-config-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 8px;
  color: #334155;
}

.field.full,
.config-actions {
  grid-column: 1 / -1;
}

.field-textarea {
  min-height: 96px;
  resize: vertical;
}

.editor-header,
.provider-actions,
.config-actions {
  justify-content: space-between;
  gap: 12px;
}

.compact-select {
  min-width: 180px;
}

.checkbox-row {
  gap: 14px;
  flex-wrap: wrap;
}

.checkbox-item,
.checkbox-field {
  display: flex;
  align-items: center;
  gap: 8px;
}

@media (max-width: 1180px) {
  .agent-workbench {
    width: calc(100vw - 24px);
    right: 12px;
    bottom: 12px;
  }

  .workbench-body,
  .provider-dialog {
    grid-template-columns: 1fr;
    flex-direction: column;
  }

  .provider-list {
    width: 100%;
  }
}
</style>
