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
              {{ activeProvider ? normalizedProviderBaseUrl(activeProvider.base_url) : DEFAULT_BASE_URL }}
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
                <el-button class="agent-header-icon" :icon="Setting" circle @click="openProviderDialog" />
              </el-tooltip>
              <el-tooltip content="外部资源管理" placement="top">
                <el-button class="agent-header-icon" :icon="Connection" circle @click="openMcpDialog" />
              </el-tooltip>
              <button class="header-btn" title="清空当前会话" @click="clearCurrentSession">清空</button>
              <button class="header-btn" title="收起" @click="toggleCollapse(true)">收起</button>
            </div>
          </div>
        </div>
      </header>

      <section class="agent-main">
        <div class="agent-workspace">
          <div ref="messagesRef" class="agent-messages">
            <template v-if="visibleMessages.length">
              <article
                v-for="message in visibleMessages"
                :key="message.id"
                class="agent-message"
                :class="message.role"
              >
                <div class="message-role">
                  <span>{{ message.role === 'user' ? '用户' : '助手' }}</span>
                  <span class="message-time">{{ formatMessageTime(message.createdAt || message.updatedAt) }}</span>
                </div>

                <details
                  v-if="message.role === 'assistant' && displayedMessageReasoning(message)"
                  class="message-reasoning"
                >
                  <summary>推理</summary>
                  <pre class="message-reasoning-content">{{ displayedMessageReasoning(message) }}</pre>
                </details>

                <MessageToolEvents
                  v-if="message.role === 'assistant' && displayedMessageToolEvents(message).length"
                  :events="displayedMessageToolEvents(message)"
                />

                <div v-if="message.attachments?.length" class="message-attachments">
                  <div
                    v-for="attachment in message.attachments"
                    :key="`${message.id}-${attachment.uploadId}`"
                    class="message-attachment-chip"
                    @click="openAttachment(attachment)"
                  >
                    <span class="message-attachment-name">{{ attachment.name }}</span>
                    <span class="message-attachment-meta">{{ attachment.kind === 'image' ? '图片' : '文件' }}</span>
                  </div>
                </div>

                <MarkdownContent
                  v-if="displayedMessageContent(message) || (streaming && message.role === 'assistant')"
                  class="message-content"
                  :content="displayedMessageContent(message) || '...'"
                  @preview-image="onPreviewImage"
                />

                <div
                  v-if="message.role === 'assistant' && messageTokenTotal(message) > 0"
                  class="message-usage"
                  :title="messageUsageDetail(message)"
                >
                  {{ formatTokenUsage(messageTokenTotal(message)) }} tokens
                </div>
              </article>
            </template>

            <div v-else class="agent-empty">
              <div class="agent-empty-title">可以直接开始对话或写文档</div>
              <div class="agent-empty-desc">直接描述你的目标即可，模型会判断是答复、续写、改写，或在兼容接口上继续流式返回结果。</div>
            </div>
          </div>

          <div class="agent-composer">
            <div class="agent-mode-tip">{{ modeTip }}</div>
            <div v-if="composerAttachments.length" class="agent-attachment-list">
              <div
                v-for="attachment in composerAttachments"
                :key="attachment.localId"
                class="agent-attachment-item"
                @click="openAttachment(attachment)"
              >
                <div class="agent-attachment-item-main">
                  <span class="agent-attachment-item-name">{{ attachment.name }}</span>
                </div>
                <span class="agent-attachment-item-meta">
                  {{ attachment.kind === 'image' ? '图片' : '文件' }} · {{ formatAttachmentSize(attachment.size) }}
                </span>
                <button
                  class="agent-attachment-remove"
                  :disabled="streaming || attachmentUploading"
                  @click.stop="removeComposerAttachment(attachment.localId)"
                >
                  移除
                </button>
              </div>
            </div>
            <textarea
              v-model="prompt"
              class="agent-textarea"
              :placeholder="textareaPlaceholder"
              :disabled="streaming"
              @keydown="handleComposerKeydown"
              @paste="handleComposerPaste"
            />

          <input
            ref="attachmentInputRef"
            class="agent-hidden-input"
            type="file"
            :accept="composerAttachmentAccept"
            multiple
            :disabled="streaming || attachmentUploading"
            @change="handleAttachmentInputChange"
          />


            <div class="agent-composer-footer">
            <div class="agent-shortcut-tip">
              Ctrl+Enter / Cmd+Enter 发送，Enter 换行<span v-if="composerAttachmentEnabled">，支持粘贴并上传{{ composerSupportsImage && composerSupportsDocument ? '图片和文件' : composerSupportsImage ? '图片' : '文件' }}</span>
            </div>

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
                <el-tooltip v-if="composerAttachmentEnabled" content="添加附件" placement="top">
                  <el-button
                    class="agent-icon-btn"
                    :icon="Paperclip"
                    :disabled="streaming || attachmentUploading"
                    :loading="attachmentUploading"
                    circle
                    @click="openAttachmentPicker"
                  />
                </el-tooltip>
                <el-tooltip content="模型管理" placement="top">
                  <el-button
                    class="agent-icon-btn"
                    :icon="Setting"
                    :disabled="!activeProvider"
                    circle
                    @click="showProviderDialog = true"
                  />
                </el-tooltip>
                <button class="primary-action" @click="onSendClick">
                  {{ streaming ? '停止' : '发送' }}
                </button>
              </div>
            </div>
            </div>
          </div>
        </div>
      </section>
    </div>

    <ProviderModelDialog v-model="showProviderDialog" @changed="onProvidersChanged" />

    <McpManagerDialog v-model="showMcpDialog" />

    <el-dialog
      v-model="showAttachmentPreview"
      class="agent-dialog attachment-preview-dialog"
      title="图片预览"
      width="min(920px, calc(100vw - 36px))"
      append-to-body
      destroy-on-close
      align-center
    >
      <div class="attachment-preview-body">
        <img v-if="previewAttachmentUrl" :src="previewAttachmentUrl" alt="附件预览" class="attachment-preview-image" />
      </div>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, triggerRef, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Connection, Paperclip, Plus, QuestionFilled, Setting } from '@element-plus/icons-vue'

import request from '@/utils/request'
import {
  executeAgentToolCalls,
  type AgentToolCall,
  type AgentToolOutputPayload,
} from '@/utils/agentTools'
import {
  AGENT_WRITER_RESULT_EVENT,
  dispatchAgentWriterChunk,
  dispatchAgentWriterComplete,
  dispatchAgentWriterStart,
  getAgentEditorSnapshot,
  type AgentWriterMode,
  type AgentWriterResultDetail,
} from '@/utils/agentWriter'
import { getAgentEditorBridge } from '@/utils/agentEditorBridge'
import { hasDocDraft } from '@/utils/docDraftCache'
import {
  AGENT_TASK_ANALYSIS_COMPLEXITIES,
  AGENT_TASK_ANALYSIS_INTENTS,
  AGENT_TASK_ANALYSIS_MODES,
  AGENT_TASK_ANALYSIS_WRITE_SCOPES,
  DEFAULT_AGENT_BASE_URL,
  resolveAgentEditorSnapshotSource,
  toolHasOnlyCapabilities,
  type AgentControlBlock,
  type AgentPageScope,
} from '@/agent/protocol'
import { useSystemStore } from '@/stores/system'
import { useAgentProviders } from '@/composables/useAgentProviders'
import ProviderModelDialog from '@/components/agent/ProviderModelDialog.vue'
import McpManagerDialog from '@/components/agent/McpManagerDialog.vue'
import MessageToolEvents from '@/components/agent/MessageToolEvents.vue'
import MarkdownContent from '@/components/agent/MarkdownContent.vue'
import { usePanelPosition } from '@/composables/usePanelPosition'
import { useAgentChatStream } from '@/composables/useAgentChatStream'
import type { ProvidersResponse, ProviderSummary, ModelSummary } from '@/api/agentProvider'

import type {
  AgentArtifact,
  AgentAttachment,
  AgentExecutionMemory,
  AgentExecutionState,
  AgentExecutionToolCallSummary,
  AgentMessage,
  AgentRequestMessage,
  AgentRouteTarget,
  AgentRuntimePlan,
  AgentRuntimePlanStep,
  AgentSession,
  AgentSessionMemory,
  AgentStructuredResponse,
  AgentTaskAnalysis,
  AgentToolEvent,
  AgentTokenUsage,
  DocType,
  OpenAiApiMode,
  PageScope,
  ProviderKind,
  RequestRole,
  RouteKind,
  SessionRole,
  StreamAction,
} from '@/components/agent/types'
import { genId } from '@/components/agent/id'
import {
  REQUEST_SUMMARY_ITEM_CHARS,
  REQUEST_SUMMARY_MAX_ITEMS,
  appendRecentToolCalls,
  buildControlFromExecutionState,
  buildExecutionProgressSignature,
  buildHistorySummary,
  buildRoundToolCallSummaryFromToolEvent,
  buildRuntimePlanFromText,
  buildSessionMemory,
  coerceBooleanLike,
  coerceNumberLike,
  coerceStringListLike,
  compactJsonLike,
  compactMessageSummary,
  compactMessageText,
  extractPlanBlock,
  extractSaveNoopState,
  hasSuccessfulMutationToolCall,
  isAgentInternalInterceptError,
  isDocumentMutationTool,
  isHostStateTool,
  isInternalExecutionLedgerMessage,
  isLikelySaveFollowUpReply,
  isPartialWriteTask,
  isReadOrSaveOnlyBatch,
  normalizeArtifact,
  normalizeControlPayload,
  normalizeMessage,
  normalizePlanStepTitle,
  normalizeRuntimePlan,
  normalizeRuntimePlanStep,
  normalizeSessionMemory,
  normalizeStructuredResponse,
  normalizeTaskAnalysis,
  normalizeToolEvent,
  parseJsonLikeValue,
  parsePlanSteps,
  parseSseBlock,
  parseToolArguments,
  resolveAgentInterceptDetails,
  resolveCurrentPlanStepIndexByTitle,
  resolvePlanStepIndicesByTitles,
  runtimePlanToPlanText,
  shouldTreatPlanAsPending,
  stripProtocolContent,
  summarizeAgentEventForDebug,
  summarizeMessageAttachments,
  summarizeRoundActions,
  summarizeToolCallBatch,
} from '@/components/agent/runtime-helpers'
import {
  useComposerAttachments,
  normalizeAttachment,
  formatAttachmentSize,
} from '@/composables/useComposerAttachments'
import {
  agentDebugEnabled,
  buildDebugExecutionSnapshot,
  buildDebugRuntimePlanSnapshot,
  logAgentDebugGroup,
  logAgentModelIo,
  logAgentPanelError,
} from '@/components/agent/debug'

const props = defineProps<{
  pageScope: PageScope
  pageState: string
  projectId: number | null
  projectName: string
  docId: number | null
  docName: string
  docType: DocType
  docContent: string
  projectCatalog: string
  currentNodeCatalog: string
  editorAvailable: boolean
  editorSnapshotSource: string
  editorUnsavedChanges: boolean
}>()

const emit = defineEmits<{
  navigate: [target: AgentRouteTarget]
}>()

const DEFAULT_BASE_URL = DEFAULT_AGENT_BASE_URL
const providerKindDefaults: Record<ProviderKind, {
  label: string
  api: OpenAiApiMode
  baseUrl: string
  auth: string
  modelsApi: string
  models: string[]
  summary: string
}> = {
  openai: {
    label: 'OpenAI / Compatible',
    api: 'auto',
    baseUrl: 'https://api.openai.com/v1',
    auth: 'Bearer API Key',
    modelsApi: 'GET /models',
    models: ['gpt-4.1', 'gpt-4.1-mini', 'gpt-4o', 'gpt-4o-mini'],
    summary: '适用于 OpenAI 兼容协议，以及大多数第三方兼容网关。',
  },
  anthropic: {
    label: 'Anthropic / Claude',
    api: 'auto',
    baseUrl: 'https://api.anthropic.com/v1',
    auth: 'x-api-key + anthropic-version',
    modelsApi: 'GET /v1/models',
    models: ['claude-sonnet-4-0', 'claude-3-7-sonnet-latest', 'claude-3-5-haiku-latest'],
    summary: '适用于 Claude 原生协议，Base URL 通常以 /v1 结尾。',
  },
  gemini: {
    label: 'Gemini',
    api: 'auto',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta',
    auth: 'URL query key=API_KEY',
    modelsApi: 'GET /v1beta/models?key=...',
    models: ['gemini-2.5-pro', 'gemini-2.5-flash', 'gemini-2.0-flash'],
    summary: '适用于 Gemini 原生协议，模型列表接口通常走 Google Generative Language。',
  },
}
const modalityOptions = [
  { value: 'text', label: '文本' },
  { value: 'image', label: '图片' },
  { value: 'audio', label: '音频' },
  { value: 'video', label: '视频' },
  { value: 'document', label: '文件' },
]
const reasoningEffortOptions = [
  { value: '', label: '未设置（默认）' },
  { value: 'none', label: 'none' },
  { value: 'minimal', label: 'minimal' },
  { value: 'low', label: 'low' },
  { value: 'medium', label: 'medium' },
  { value: 'high', label: 'high' },
  { value: 'xhigh', label: 'xhigh' },
]
const openAiApiOptions: Array<{ value: OpenAiApiMode; label: string }> = [
  { value: 'auto', label: '自动（官方 Responses / 兼容 Chat）' },
  { value: 'responses', label: 'Responses API' },
  { value: 'chat', label: 'Chat Completions API' },
]
const modelApiOptions: Array<{ value: '' | 'responses' | 'chat'; label: string }> = [
  { value: '', label: '继承供应商' },
  { value: 'responses', label: 'Responses API' },
  { value: 'chat', label: 'Chat Completions API' },
]
const booleanOptions = [
  { value: '', label: '未设置（默认）' },
  { value: 'true', label: '开启' },
  { value: 'false', label: '关闭' },
]
const modelParameterHelp: Record<string, string> = {
  api: '当前模型使用的 OpenAI API 模式。继承表示使用供应商设置；Responses 适合 OpenAI 官方和支持 Responses 的网关；Chat Completions 适合绝大多数 OpenAI 兼容接口。',
  modalities: '声明这个模型在你项目中的可用输入模态。它主要用于前端能力提示和后续多模态开关判断；留空表示按模型实际能力与 SDK 默认处理。',
  tools_enabled: '是否允许这个模型在 Agent 过程中发起工具调用。留空表示按系统默认策略；关闭后会禁止工具调用，只保留纯文本对话生成。',
  thinking: '是否显式开启推理/思考模式。留空表示不额外指定，由 provider 或模型自行决定；开启后会尽量向上游传递 reasoning / thinking 配置。',
  temperature: '采样温度，数值越低越稳定，越高越发散。常见范围是 0 到 2；留空表示沿用 provider / SDK 默认值。',
  max_output_tokens: '单次响应允许生成的最大 token 数。留空表示使用模型默认上限或 SDK 默认值。',
  top_p: '核采样阈值。设置后会限制累计概率最高的一部分 token 参与采样；通常与 temperature 二选一微调即可。',
  top_k: '仅对支持的模型生效，限制每一步只在前 K 个候选 token 中采样。Gemini 等模型更常见。',
  presence_penalty: '出现惩罚。鼓励模型引入新词、新主题，减少重复已出现过的内容。',
  frequency_penalty: '频率惩罚。对已经高频出现的 token 提高惩罚，抑制重复表达。',
  parallel_tool_calls: '是否允许模型并行发起多个工具调用。只对支持该能力的 provider 生效；留空表示使用默认行为。',
  reasoning_effort: '推理强度。当前主要用于支持 reasoning effort 的模型；数值越高，通常思考更深但成本和时延也更高。',
  stop_sequences: '命中这些停止词后立刻结束生成。每行一个，留空表示不设置。',
  response_mime_type: '期望的响应 MIME 类型，主要用于 Gemini 等支持结构化 MIME 的模型，例如 application/json。',
  additional_params: '高级附加参数，会原样透传给对应 provider。用于补充当前表单未覆盖的协议字段，必须填写合法 JSON 对象。',
}
const SESSIONS_KEY = 'markflow.agent.sessions'
const MAX_TOOL_CALL_ROUNDS = 1000
const MAX_REPEAT_TOOL_SIGNATURE_HITS = 4

const mounted = ref(false)
const showProviderDialog = ref(false)
const showMcpDialog = ref(false)
const showRuntimeDock = ref(true)
const streaming = ref(false)
const modelLoading = ref(false)
const prompt = ref('')
const currentSessionId = ref<string>('')
const sessions = ref<AgentSession[]>([])
const {
  collapsed,
  panelX,
  panelY,
  panelStyle,
  toggleCollapse,
  persistPanelState,
  startDrag,
  stopDrag,
  handleFabClick,
  loadPanelState,
  reclampPanelPosition,
} = usePanelPosition(() => currentSessionId.value)
const {
  providers,
  activeProviderId,
  activeProvider,
  activeModelOptions,
  applyResponse: applyProvidersResponse,
  loadProviders,
  activate: activateProviderById,
  findModel,
  fallbackModel,
  modelOptionsForProvider,
  normalizedBaseUrl: normalizedProviderBaseUrl,
} = useAgentProviders()
const streamingAssistantId = ref('')
const liveAssistantContent = ref('')
const liveAssistantReasoning = ref('')
const agentTransportMode = ref<'responses' | 'chat_fallback' | 'chat' | ''>('')
const messagesRef = ref<HTMLElement | null>(null)

let activeStreamController: AbortController | null = null
const systemStore = useSystemStore()

const currentSession = computed(() => sessions.value.find((session) => session.id === currentSessionId.value) || null)
const visibleMessages = computed(() =>
  (currentSession.value?.messages || []).filter((message) =>
    message.role === 'user' || message.role === 'assistant',
  ),
)
const currentSessionLastPlan = computed(() => currentSession.value?.lastPlan?.trim() || '')
const currentSessionTaskAnalysis = computed(() => currentSession.value?.taskAnalysis || null)
const currentSessionRuntimePlan = computed(() => currentSession.value?.runtimePlan || null)
const currentSessionArtifacts = computed(() => currentSession.value?.artifacts || [])
const currentSessionToolEvents = computed(() => currentSession.value?.toolEvents || [])
const currentRuntimeStep = computed(() => {
  const runtimePlan = currentSessionRuntimePlan.value
  if (!runtimePlan?.steps?.length) return null
  return runtimePlan.steps.find((step) => step.status === 'running')
    || runtimePlan.steps.find((step) => step.status === 'pending')
    || runtimePlan.steps[runtimePlan.steps.length - 1]
    || null
})
const runtimeDockSummary = computed(() => {
  if (currentPlanNeedsConfirmation.value) {
    return '待确认计划'
  }
  if (streaming.value) {
    return currentRuntimeStep.value?.title || '正在执行'
  }
  if (currentSessionRuntimePlan.value?.status === 'completed') {
    return '执行完成'
  }
  return currentRuntimeStep.value?.title || '暂无运行状态'
})
const selectedProviderId = computed<number | null>({
  get: () => activeProviderId.value,
  set: (value) => {
    if (value !== null && value !== undefined) {
      void activateProviderById(value)
    }
  },
})
const currentSessionModel = computed({
  get: () => currentSession.value?.model || '',
  set: (value: string) => {
    const session = currentSession.value
    if (!session) return
    const normalized = value.trim()
    const modelChanged = session.model !== normalized
    session.providerId = activeProvider.value?.id ?? null
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
const activeModelConfig = computed(() => {
  const provider = activeProvider.value
  const model = currentSessionModel.value.trim()
  if (!provider || !model) return null
  return findModel(provider, model)?.config || null
})
const activeModelModalities = computed(() => uniqueStrings(activeModelConfig.value?.modalities || []))
const composerSupportsImage = computed(() => activeModelModalities.value.includes('image'))
const composerSupportsDocument = computed(() => activeModelModalities.value.includes('document'))
const composerAttachmentEnabled = computed(() => composerSupportsImage.value || composerSupportsDocument.value)
const {
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
} = useComposerAttachments({
  supportsImage: composerSupportsImage,
  supportsDocument: composerSupportsDocument,
  attachmentEnabled: composerAttachmentEnabled,
  streaming,
})
const currentConfirmationPlan = computed(() => {
  const session = currentSession.value
  if (!session) return ''
  const awaitingPlanConfirmation = session.lastExecutionMemory?.currentMode === 'plan'
    && session.lastExecutionMemory?.awaiting === 'user_input'
  if (!awaitingPlanConfirmation && session.runtimePlan?.status !== 'pending') {
    return ''
  }
  return runtimePlanToPlanText(session.runtimePlan).trim() || session.pendingPlan?.trim() || ''
})
const currentPlanNeedsConfirmation = computed(() => Boolean(currentConfirmationPlan.value))

const { sendMessage, stopStreaming } = useAgentChatStream({
  props,
  emit,
  streaming,
  sessions,
  liveAssistantContent,
  liveAssistantReasoning,
  streamingAssistantId,
  agentTransportMode,
  prompt,
  composerAttachments,
  showProviderDialog,
  attachmentUploading,
  activeProvider,
  currentConfirmationPlan,
  currentPlanNeedsConfirmation,
  ensureSession,
  openProviderDialog,
  syncSessionWithActiveProvider,
  clearComposerAttachments,
  scrollMessagesToBottom,
  persistSessions,
})

function onSendClick() {
  if (streaming.value) {
    stopStreaming()
  } else {
    void sendMessage()
  }
}

const modeTip = computed(() => {
  if (props.docType === 'doc') return '直接说你的目标即可，模型会自己决定是答复、续写还是重写当前文档。'
  return '当前不在具体文档页时，模型只进行对话回答，不会直接写入文档。'
})

const textareaPlaceholder = computed(() => {
  if (props.docType === 'doc') return '例如：继续完善这篇文档的部署说明，并补充安装、验证和常见问题'
  return '输入你的问题，按 Enter 发送'
})


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

function parseNumberText(value: string): number | null {
  const trimmed = value.trim()
  if (!trimmed) return null
  const parsed = Number(trimmed)
  return Number.isFinite(parsed) ? parsed : null
}

function parseIntegerText(value: string): number | null {
  const parsed = parseNumberText(value)
  return parsed === null ? null : Math.trunc(parsed)
}

function parseBooleanChoice(value: '' | 'true' | 'false'): boolean | null {
  if (value === 'true') return true
  if (value === 'false') return false
  return null
}

function persistSessions() {
  localStorage.setItem(SESSIONS_KEY, JSON.stringify(sessions.value))
}

function normalizeOptionalTimestamp(value: unknown) {
  if (typeof value === 'string' && value.trim()) {
    const timestamp = new Date(value).getTime()
    return Number.isFinite(timestamp) ? timestamp : null
  }
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value
  }
  return null
}

function formatOptionalTimestamp(value: number | null) {
  if (!value) return '未同步'
  return formatSessionTime(value)
}

function normalizeSession(raw: any, provider: ProviderSummary | null): AgentSession | null {
  if (!raw || typeof raw !== 'object') return null
  const messages = Array.isArray(raw.messages)
    ? raw.messages
      .map((message: any) => normalizeMessage(message))
      .filter((message: AgentMessage | null): message is AgentMessage => {
        if (!message) return false
        return !(message.role === 'system' && isInternalExecutionLedgerMessage(message.content))
      })
    : []

  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : genId(),
    title: typeof raw.title === 'string' && raw.title.trim() ? raw.title.trim() : '新会话',
    messages,
    taskAnalysis: normalizeTaskAnalysis(raw.taskAnalysis ?? raw.task_analysis),
    runtimePlan: normalizeRuntimePlan(raw.runtimePlan ?? raw.runtime_plan),
    artifacts: Array.isArray(raw.artifacts)
      ? raw.artifacts
        .map((artifact: any) => normalizeArtifact(artifact))
        .filter((artifact: AgentArtifact | null): artifact is AgentArtifact => Boolean(artifact))
      : [],
    toolEvents: Array.isArray(raw.toolEvents ?? raw.tool_events)
      ? (raw.toolEvents ?? raw.tool_events)
        .map((event: any) => normalizeToolEvent(event))
        .filter((event: AgentToolEvent | null): event is AgentToolEvent => Boolean(event))
      : [],
    providerId: typeof raw.providerId === 'number' ? raw.providerId : provider?.id ?? null,
    model: typeof raw.model === 'string' ? raw.model.trim() : fallbackModel(provider),
    previousResponseId: typeof raw.previousResponseId === 'string' && raw.previousResponseId.trim() ? raw.previousResponseId.trim() : null,
    pendingPlan: typeof raw.pendingPlan === 'string' && raw.pendingPlan.trim() ? raw.pendingPlan.trim() : null,
    pendingPlanToolOutputs: Array.isArray(raw.pendingPlanToolOutputs)
      ? raw.pendingPlanToolOutputs
        .map((output: any) => ({
          call_id: typeof output?.call_id === 'string' && output.call_id.trim() ? output.call_id.trim() : genId(),
          name: typeof output?.name === 'string' && output.name.trim() ? output.name.trim() : undefined,
          arguments: typeof output?.arguments === 'string' && output.arguments.trim() ? output.arguments.trim() : undefined,
          output: output?.output ?? null,
        }))
        .filter((output: AgentToolOutputPayload) => Boolean(output.call_id))
      : [],
    lastPlan: typeof raw.lastPlan === 'string' && raw.lastPlan.trim() ? raw.lastPlan.trim() : null,
    lastExecutionMemory: raw.lastExecutionMemory && typeof raw.lastExecutionMemory === 'object'
        ? {
            currentMode: typeof raw.lastExecutionMemory.currentMode === 'string' && raw.lastExecutionMemory.currentMode.trim()
              ? raw.lastExecutionMemory.currentMode.trim()
              : typeof raw.lastExecutionMemory.current_mode === 'string' && raw.lastExecutionMemory.current_mode.trim()
                ? raw.lastExecutionMemory.current_mode.trim()
                : 'normal',
            awaiting: typeof raw.lastExecutionMemory.awaiting === 'string' && raw.lastExecutionMemory.awaiting.trim()
              ? raw.lastExecutionMemory.awaiting.trim()
              : null,
            currentActionKind: typeof raw.lastExecutionMemory.currentActionKind === 'string' && raw.lastExecutionMemory.currentActionKind.trim()
              ? raw.lastExecutionMemory.currentActionKind.trim()
              : typeof raw.lastExecutionMemory.current_action_kind === 'string' && raw.lastExecutionMemory.current_action_kind.trim()
                ? raw.lastExecutionMemory.current_action_kind.trim()
                : null,
            currentActionStatus: typeof raw.lastExecutionMemory.currentActionStatus === 'string' && raw.lastExecutionMemory.currentActionStatus.trim()
              ? raw.lastExecutionMemory.currentActionStatus.trim()
              : typeof raw.lastExecutionMemory.current_action_status === 'string' && raw.lastExecutionMemory.current_action_status.trim()
                ? raw.lastExecutionMemory.current_action_status.trim()
                : null,
            currentActionMode: typeof raw.lastExecutionMemory.currentActionMode === 'string' && raw.lastExecutionMemory.currentActionMode.trim()
              ? raw.lastExecutionMemory.currentActionMode.trim()
              : typeof raw.lastExecutionMemory.current_action_mode === 'string' && raw.lastExecutionMemory.current_action_mode.trim()
                ? raw.lastExecutionMemory.current_action_mode.trim()
                : null,
            currentActionTarget: typeof raw.lastExecutionMemory.currentActionTarget === 'string' && raw.lastExecutionMemory.currentActionTarget.trim()
              ? raw.lastExecutionMemory.currentActionTarget.trim()
              : typeof raw.lastExecutionMemory.current_action_target === 'string' && raw.lastExecutionMemory.current_action_target.trim()
                ? raw.lastExecutionMemory.current_action_target.trim()
                : null,
            confirmationRequired: raw.lastExecutionMemory.confirmationRequired === true || raw.lastExecutionMemory.confirmation_required === true,
            plan: typeof raw.lastExecutionMemory.plan === 'string' && raw.lastExecutionMemory.plan.trim() ? raw.lastExecutionMemory.plan.trim() : null,
            assistantSummary: typeof raw.lastExecutionMemory.assistantSummary === 'string' && raw.lastExecutionMemory.assistantSummary.trim() ? raw.lastExecutionMemory.assistantSummary.trim() : null,
            controlPhase: typeof raw.lastExecutionMemory.controlPhase === 'string' && raw.lastExecutionMemory.controlPhase.trim() ? raw.lastExecutionMemory.controlPhase.trim() : null,
            taskKind: typeof raw.lastExecutionMemory.taskKind === 'string' && raw.lastExecutionMemory.taskKind.trim() ? raw.lastExecutionMemory.taskKind.trim() : null,
            editIntent: typeof raw.lastExecutionMemory.editIntent === 'string' && raw.lastExecutionMemory.editIntent.trim() ? raw.lastExecutionMemory.editIntent.trim() : null,
            editStage: typeof raw.lastExecutionMemory.editStage === 'string' && raw.lastExecutionMemory.editStage.trim() ? raw.lastExecutionMemory.editStage.trim() : null,
            saveRequested: raw.lastExecutionMemory.saveRequested === true,
            writeCompleted: raw.lastExecutionMemory.writeCompleted === true,
            planStepIndex: Number.isFinite(raw.lastExecutionMemory.planStepIndex) ? Number(raw.lastExecutionMemory.planStepIndex) : null,
            planTotalSteps: Number.isFinite(raw.lastExecutionMemory.planTotalSteps) ? Number(raw.lastExecutionMemory.planTotalSteps) : null,
            planCurrentStep: typeof raw.lastExecutionMemory.planCurrentStep === 'string' && raw.lastExecutionMemory.planCurrentStep.trim() ? raw.lastExecutionMemory.planCurrentStep.trim() : null,
            planCompletedSteps: Array.isArray(raw.lastExecutionMemory.planCompletedSteps)
              ? raw.lastExecutionMemory.planCompletedSteps
                .filter((item: unknown): item is string => typeof item === 'string' && Boolean(item.trim()))
                .map((item: string) => item.trim())
              : [],
            documentWriteObserved: raw.lastExecutionMemory.documentWriteObserved === true,
            saveAttemptWithoutDocumentChange: raw.lastExecutionMemory.saveAttemptWithoutDocumentChange === true,
            lastInterceptCode: typeof raw.lastExecutionMemory.lastInterceptCode === 'string' && raw.lastExecutionMemory.lastInterceptCode.trim()
              ? raw.lastExecutionMemory.lastInterceptCode.trim()
              : null,
            lastInterceptMessage: typeof raw.lastExecutionMemory.lastInterceptMessage === 'string' && raw.lastExecutionMemory.lastInterceptMessage.trim()
              ? raw.lastExecutionMemory.lastInterceptMessage.trim()
              : null,
            lastInterceptGuidance: typeof raw.lastExecutionMemory.lastInterceptGuidance === 'string' && raw.lastExecutionMemory.lastInterceptGuidance.trim()
              ? raw.lastExecutionMemory.lastInterceptGuidance.trim()
              : null,
            recentToolCalls: Array.isArray(raw.lastExecutionMemory.recentToolCalls)
            ? raw.lastExecutionMemory.recentToolCalls
              .map((call: any) => ({
                name: typeof call?.name === 'string' ? call.name : '',
                arguments: typeof call?.arguments === 'string' && call.arguments.trim() ? call.arguments.trim() : null,
                output: typeof call?.output === 'string' && call.output.trim() ? call.output.trim() : null,
                ok: typeof call?.ok === 'boolean' ? call.ok : null,
                outcome: call?.outcome === 'success' || call?.outcome === 'noop' || call?.outcome === 'error'
                  ? call.outcome
                  : 'unknown',
              }))
              .filter((call: AgentExecutionToolCallSummary) => Boolean(call.name.trim()))
            : [],
        }
      : null,
    sessionMemory: normalizeSessionMemory(raw.sessionMemory ?? raw.session_memory),
    lastSyncedMessageCount: Number.isInteger(raw.lastSyncedMessageCount) ? Math.max(0, raw.lastSyncedMessageCount) : 0,
    createdAt: Number.isFinite(raw.createdAt) ? raw.createdAt : Date.now(),
    updatedAt: Number.isFinite(raw.updatedAt) ? raw.updatedAt : Date.now(),
  }
}

function loadSessions(provider: ProviderSummary | null) {
  return loadJson<any[]>(SESSIONS_KEY, [])
    .map((session) => normalizeSession(session, provider))
    .filter((session): session is AgentSession => Boolean(session))
}

function ensureSession(): AgentSession {
  let session = currentSession.value
  if (session) return session

  session = {
    id: genId(),
    title: '新会话',
    messages: [],
    taskAnalysis: null,
    runtimePlan: null,
    artifacts: [],
    toolEvents: [],
    providerId: activeProvider.value?.id || null,
    model: fallbackModel(activeProvider.value),
    previousResponseId: null,
    pendingPlan: null,
    pendingPlanToolOutputs: [],
    lastPlan: null,
    lastExecutionMemory: null,
    sessionMemory: null,
    lastSyncedMessageCount: 0,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }
  sessions.value.unshift(session)
  currentSessionId.value = session.id
  persistSessions()
  persistPanelState(currentSessionId.value)
  return session
}

function createSession() {
  const session: AgentSession = {
    id: genId(),
    title: '新会话',
    messages: [],
    taskAnalysis: null,
    runtimePlan: null,
    artifacts: [],
    toolEvents: [],
    providerId: activeProvider.value?.id || null,
    model: fallbackModel(activeProvider.value),
    previousResponseId: null,
    pendingPlan: null,
    pendingPlanToolOutputs: [],
    lastPlan: null,
    lastExecutionMemory: null,
    sessionMemory: null,
    lastSyncedMessageCount: 0,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }
  sessions.value.unshift(session)
  currentSessionId.value = session.id
  prompt.value = ''
  persistSessions()
  persistPanelState(currentSessionId.value)
  scrollMessagesToBottom()
}

function selectSession(sessionId: string) {
  currentSessionId.value = sessionId
  persistPanelState(currentSessionId.value)
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
  persistPanelState(currentSessionId.value)
}

function clearCurrentSession() {
  const session = ensureSession()
  session.messages = []
  session.title = '新会话'
  session.taskAnalysis = null
  session.runtimePlan = null
  session.artifacts = []
  session.toolEvents = []
  session.previousResponseId = null
  session.pendingPlan = null
  session.pendingPlanToolOutputs = []
  session.lastPlan = null
  session.lastExecutionMemory = null
  session.sessionMemory = null
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
  const content = isStreamingAssistantMessage(message) ? liveAssistantContent.value : message.content
  return stripProtocolContent(content)
}

function displayedMessageReasoning(message: AgentMessage) {
  return isStreamingAssistantMessage(message) ? liveAssistantReasoning.value : (message.reasoning || '')
}

function displayedMessageToolEvents(message: AgentMessage) {
  return message.toolEvents || []
}

function messageTokenTotal(message: AgentMessage) {
  const usage = message.usage
  if (!usage) return 0
  const num = (value: number | undefined) => (Number.isFinite(value) && (value as number) > 0 ? (value as number) : 0)
  const explicit = num(usage.total_tokens)
  if (explicit) return explicit
  return num(usage.input_tokens)
    + num(usage.output_tokens)
    + num(usage.cached_input_tokens)
    + num(usage.cache_creation_input_tokens)
    + num(usage.tool_use_prompt_tokens)
    + num(usage.reasoning_tokens)
}

function formatTokenUsage(value: number) {
  const number = Number(value)
  if (!Number.isFinite(number) || number <= 0) return '0'
  const units = [
    { value: 1_000_000_000, suffix: 'b' },
    { value: 1_000_000, suffix: 'm' },
    { value: 1_000, suffix: 'k' },
  ]
  const unit = units.find((item) => number >= item.value)
  if (!unit) return `${Math.round(number)}`
  const scaled = number / unit.value
  const text = scaled >= 10 ? scaled.toFixed(0) : scaled.toFixed(1)
  return `${text.replace(/\.0$/, '')}${unit.suffix}`
}

function messageUsageDetail(message: AgentMessage) {
  const usage = message.usage
  if (!usage) return ''
  const labels: Array<[keyof AgentTokenUsage, string]> = [
    ['input_tokens', '输入'],
    ['output_tokens', '输出'],
    ['reasoning_tokens', '推理'],
    ['cached_input_tokens', '缓存读取'],
    ['cache_creation_input_tokens', '缓存写入'],
    ['tool_use_prompt_tokens', '工具'],
    ['total_tokens', '合计'],
  ]
  return labels
    .map(([key, label]) => [label, Number(usage[key])] as [string, number])
    .filter(([, value]) => Number.isFinite(value) && value > 0)
    .map(([label, value]) => `${label} ${value}`)
    .join(' · ')
}

function onPreviewImage(url: string) {
  if (!url) return
  previewAttachmentUrl.value = url
  showAttachmentPreview.value = true
}

function syncSessionWithActiveProvider(session: AgentSession | null) {
  if (!session) return

  const provider = activeProvider.value
  const providerId = provider?.id || null
  const allowedModels = modelOptionsForProvider(provider)
  let changed = false

  if (session.providerId !== providerId) {
    session.providerId = providerId
    session.previousResponseId = null
    session.pendingPlan = null
    session.pendingPlanToolOutputs = []
    session.lastPlan = null
    session.lastExecutionMemory = null
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

function formatMessageTime(timestamp?: number) {
  const value = Number.isFinite(timestamp) ? Number(timestamp) : Date.now()
  const date = new Date(value)
  const yyyy = `${date.getFullYear()}`
  const MM = `${date.getMonth() + 1}`.padStart(2, '0')
  const dd = `${date.getDate()}`.padStart(2, '0')
  const hh = `${date.getHours()}`.padStart(2, '0')
  const mm = `${date.getMinutes()}`.padStart(2, '0')
  const ss = `${date.getSeconds()}`.padStart(2, '0')
  return `${yyyy}-${MM}-${dd} ${hh}:${mm}:${ss}`
}

function openProviderDialog() {
  showProviderDialog.value = true
}

function onProvidersChanged(response: ProvidersResponse) {
  applyProvidersResponse(response)
  syncSessionWithActiveProvider(currentSession.value)
}

function openMcpDialog() {
  showMcpDialog.value = true
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
  reclampPanelPosition(currentSessionId.value)
})

watch([activeProviderId, currentSessionId, activeModelKey], () => {
  syncSessionWithActiveProvider(currentSession.value)
})

watch([currentSessionId, () => currentSessionModel.value, activeProviderId], () => {
  clearComposerAttachments()
})

watch(composerAttachmentEnabled, (enabled) => {
  if (!enabled && composerAttachments.value.length) {
    clearComposerAttachments()
  }
})

onMounted(async () => {
  mounted.value = true

  const panelState = loadPanelState()

  try {
    await loadProviders()
  } catch (error: any) {
    logAgentPanelError('refresh_providers_state', error)
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
  width: min(440px, calc(100vw - 32px));
  max-width: calc(100vw - 32px);
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
  height: min(760px, calc(100vh - 64px));
  min-height: 460px;
  max-height: calc(100vh - 64px);
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
  flex-direction: row;
}

.agent-sidebar {
  display: none;
  width: 308px;
  flex-shrink: 0;
  border-right: 1px solid rgba(122, 147, 91, 0.12);
  background: rgba(247, 249, 242, 0.72);
  padding: 14px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.agent-sidebar-toggle {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 14px;
  border: 1px solid rgba(122, 147, 91, 0.16);
  background: rgba(255, 255, 255, 0.88);
  cursor: pointer;
  text-align: left;
}

.agent-sidebar-toggle-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.agent-sidebar-toggle-title {
  font-size: 12px;
  font-weight: 700;
  color: #31402b;
}

.agent-sidebar-toggle-summary {
  font-size: 12px;
  color: #6b7961;
  line-height: 1.5;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.agent-sidebar-toggle-meta {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.agent-sidebar-scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding-right: 4px;
}

.agent-workspace {
  flex: 1;
  min-width: 0;
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
  padding: 18px 24px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.agent-messages::-webkit-scrollbar,
.agent-sidebar-scroll::-webkit-scrollbar,
.provider-list::-webkit-scrollbar,
.model-pane-scroll::-webkit-scrollbar {
  width: 10px;
}

.agent-messages::-webkit-scrollbar-track,
.agent-sidebar-scroll::-webkit-scrollbar-track,
.provider-list::-webkit-scrollbar-track,
.model-pane-scroll::-webkit-scrollbar-track {
  background: rgba(122, 147, 91, 0.12);
  border-radius: 999px;
}

.agent-messages::-webkit-scrollbar-thumb,
.agent-sidebar-scroll::-webkit-scrollbar-thumb,
.provider-list::-webkit-scrollbar-thumb,
.model-pane-scroll::-webkit-scrollbar-thumb {
  background: rgba(111, 154, 79, 0.58);
  border-radius: 999px;
  border: 2px solid rgba(246, 248, 239, 0.9);
}

.agent-plan-card {
  border: 1px solid rgba(122, 147, 91, 0.2);
  border-radius: 16px;
  padding: 11px 12px;
  background: rgba(250, 252, 244, 0.92);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.72);
}

.agent-plan-card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 8px;
}

.agent-plan-card-title {
  font-size: 12px;
  font-weight: 700;
  color: #405037;
}

.agent-plan-card-status {
  border-radius: 999px;
  padding: 2px 8px;
  background: rgba(157, 172, 140, 0.18);
  color: #607057;
  font-size: 11px;
  font-weight: 700;
}

.agent-plan-card-status.active {
  background: rgba(111, 154, 79, 0.16);
  color: #4d6a34;
}

.agent-plan-card-content {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 12px;
  line-height: 1.6;
  color: #2c3a26;
}

.agent-runtime-card {
  border: 1px solid rgba(122, 147, 91, 0.16);
  border-radius: 16px;
  padding: 11px 12px;
  background: rgba(255, 255, 255, 0.72);
}

.agent-runtime-overview {
  gap: 10px;
}

.agent-runtime-chip-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.agent-runtime-chip {
  display: inline-flex;
  align-items: center;
  padding: 4px 8px;
  border-radius: 999px;
  background: rgba(122, 147, 91, 0.1);
  color: #506046;
  font-size: 11px;
  line-height: 1.4;
}

.agent-runtime-current-step {
  font-size: 13px;
  line-height: 1.6;
  color: #24311f;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.agent-runtime-empty {
  min-height: 140px;
  justify-content: center;
}

.agent-runtime-empty-text {
  font-size: 12px;
  line-height: 1.7;
  color: #6b7961;
}

.agent-plan-steps {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 10px;
}

.agent-plan-step {
  border-radius: 12px;
  padding: 10px 12px;
  background: rgba(246, 248, 239, 0.88);
  border: 1px solid rgba(122, 147, 91, 0.14);
}

.agent-plan-step.running {
  border-color: rgba(111, 154, 79, 0.38);
  background: rgba(240, 247, 231, 0.95);
}

.agent-plan-step.completed {
  opacity: 0.78;
}

.agent-plan-step-title {
  font-size: 12px;
  font-weight: 700;
  color: #24311f;
}

.agent-plan-step-meta {
  margin-top: 4px;
  font-size: 11px;
  color: #6a7861;
}

.agent-plan-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
}

.agent-tool-events {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.agent-tool-event {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px 10px;
  border-radius: 12px;
  background: rgba(248, 250, 243, 0.92);
  border: 1px solid rgba(122, 147, 91, 0.1);
}

.agent-tool-event-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.agent-tool-event-name {
  min-width: 0;
  font-size: 12px;
  font-weight: 700;
  color: #31402b;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.agent-tool-event-status {
  flex-shrink: 0;
  font-size: 11px;
  color: #6f7e65;
}

.agent-tool-event-summary {
  font-size: 11px;
  line-height: 1.55;
  color: #4d5b44;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.agent-artifact-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.agent-artifact-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.agent-artifact-title {
  font-size: 12px;
  font-weight: 700;
  color: #24311f;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.agent-artifact-status {
  font-size: 11px;
  color: #607057;
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
  gap: 6px;
  width: 100%;
}
.agent-message.user {
  align-items: flex-end;
}
.agent-message.assistant {
  align-items: flex-start;
}

.message-role {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  font-weight: 700;
  padding: 2px 10px;
  border-radius: 999px;
  letter-spacing: 0.02em;
}
.message-time {
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0;
  opacity: 0.72;
}
.agent-message.user .message-role {
  color: #3d6b2e;
  background: rgba(111, 154, 79, 0.18);
}
.agent-message.assistant .message-role {
  color: #4a5a8a;
  background: rgba(96, 118, 170, 0.16);
}

.message-content {
  box-sizing: border-box;
  font-size: 13px;
  line-height: 1.6;
  color: #24311f;
  border-radius: 12px;
  padding: 10px 12px;
  max-width: 98%;
}
.agent-message.user .message-content {
  background: rgba(111, 154, 79, 0.14);
  border: 1px solid rgba(111, 154, 79, 0.22);
  border-top-right-radius: 4px;
}
.agent-message.assistant .message-content {
  background: rgba(238, 243, 231, 0.9);
  border: 1px solid rgba(122, 147, 91, 0.2);
  border-top-left-radius: 4px;
}

.message-usage {
  align-self: flex-start;
  margin-top: 2px;
  padding: 2px 8px;
  border-radius: 999px;
  background: rgba(122, 147, 91, 0.1);
  color: #6a7a58;
  font-size: 11px;
  line-height: 1.4;
  cursor: default;
  user-select: none;
}

/* 助手侧的推理 / 工具卡片与气泡同宽左对齐 */
.agent-message.assistant .message-reasoning,
.agent-message.assistant .tool-events {
  max-width: 98%;
}
/* 附件按角色贴边 */
.agent-message.user .message-attachments {
  justify-content: flex-end;
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
  padding: 8px 10px;
  font-size: 12px;
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
  padding: 0 10px 10px;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: inherit;
  font-size: 12px;
  line-height: 1.6;
  color: #708067;
  max-height: 280px;
  overflow-y: auto;
  scrollbar-width: thin;
}
.message-reasoning-content::-webkit-scrollbar {
  width: 8px;
}
.message-reasoning-content::-webkit-scrollbar-thumb {
  background: rgba(111, 154, 79, 0.4);
  border-radius: 999px;
}

.message-attachments {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 10px;
}

.message-attachment-chip {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.9);
  border: 1px solid rgba(122, 147, 91, 0.14);
  color: #51604a;
  font-size: 12px;
  cursor: pointer;
  transition: border-color 0.16s ease, background 0.16s ease;
}

.message-attachment-chip:hover {
  background: rgba(244, 248, 235, 0.98);
  border-color: rgba(122, 147, 91, 0.22);
}

.message-attachment-name {
  font-weight: 700;
  color: #31402b;
}

.message-attachment-meta {
  color: #7a846f;
}

.agent-composer {
  flex-shrink: 0;
  border-top: 1px solid rgba(122, 147, 91, 0.12);
  padding: 12px 18px 16px;
  background: rgba(251, 252, 247, 0.94);
}

.agent-runtime-pill {
  display: inline-flex;
  align-items: center;
  padding: 4px 8px;
  border-radius: 999px;
  background: rgba(122, 147, 91, 0.12);
  color: #537535;
  font-size: 11px;
  font-weight: 700;
}

.agent-runtime-chevron {
  font-size: 11px;
  color: #7a846f;
}

.agent-mode-tip {
  width: 100%;
  margin: 0 auto;
  font-size: 11px;
  color: #7b8771;
  line-height: 1.6;
}

.agent-textarea {
  width: 100%;
  margin: 10px auto 0;
  min-height: 112px;
  resize: none;
  border-radius: 16px;
  border: 1px solid rgba(122, 147, 91, 0.18);
  background: rgba(255, 255, 255, 0.92);
  color: #1d2719;
  padding: 10px 12px;
  font: inherit;
  font-size: 13px;
  line-height: 1.6;
}

.agent-textarea::placeholder {
  color: #87937e;
}

.agent-textarea:focus {
  outline: none;
  border-color: rgba(111, 154, 79, 0.46);
  box-shadow: 0 0 0 3px rgba(111, 154, 79, 0.12);
}

.agent-hidden-input {
  display: none;
}

.agent-attachment-list {
  display: flex;
  gap: 8px;
  flex-wrap: nowrap;
  min-width: 0;
  width: 100%;
  margin: 10px auto 0;
  overflow-x: auto;
  overflow-y: hidden;
  padding-bottom: 2px;
}

.agent-attachment-item {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  gap: 8px;
  max-width: 260px;
  padding: 6px 10px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.88);
  border: 1px solid rgba(122, 147, 91, 0.12);
  cursor: pointer;
  transition: border-color 0.16s ease, background 0.16s ease;
}

.agent-attachment-item:hover {
  background: rgba(244, 248, 235, 0.98);
  border-color: rgba(122, 147, 91, 0.22);
}

.agent-attachment-item-main {
  min-width: 0;
  display: flex;
  align-items: center;
}

.agent-attachment-item-name {
  max-width: 180px;
  font-size: 12px;
  font-weight: 700;
  color: #31402b;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.agent-attachment-item-meta {
  font-size: 11px;
  color: #7a846f;
  white-space: nowrap;
}

.agent-attachment-remove {
  flex-shrink: 0;
  border: none;
  background: transparent;
  color: #c65b4d;
  font-size: 12px;
  padding: 0;
  cursor: pointer;
}

.agent-attachment-remove:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.attachment-preview-body {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 240px;
}

.attachment-preview-image {
  display: block;
  max-width: 100%;
  max-height: 72vh;
  object-fit: contain;
  border-radius: 16px;
  box-shadow: 0 18px 40px rgba(34, 48, 25, 0.18);
}

.agent-composer-footer {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 10px;
  margin: 12px auto 0;
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
}

.agent-inline-selects {
  display: grid;
  grid-template-columns: minmax(0, 1.1fr) minmax(0, 1.35fr);
  gap: 12px;
  width: 100%;
}

.agent-select-row {
  display: grid;
  grid-template-columns: 42px minmax(0, 1fr);
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.agent-control-label {
  font-size: 12px;
  color: #607057;
  text-align: right;
  white-space: nowrap;
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
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.model-action-button {
  --el-button-text-color: #537535;
  --el-button-hover-text-color: #405a29;
  --el-button-bg-color: rgba(122, 147, 91, 0.14);
  --el-button-hover-bg-color: rgba(122, 147, 91, 0.2);
  --el-button-active-bg-color: rgba(122, 147, 91, 0.24);
  padding: 4px 10px;
  border-radius: 999px;
  font-weight: 700;
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

.model-source-badge.is-configured {
  background: rgba(217, 119, 6, 0.12);
  color: #b45309;
}

.model-configured-dot {
  display: inline-flex;
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: #d97706;
  box-shadow: 0 0 0 4px rgba(217, 119, 6, 0.12);
  flex-shrink: 0;
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

.model-config-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.model-config-toolbar {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.model-config-reference {
  min-width: 0;
  flex: 1;
  padding: 10px 12px;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.68);
  border: 1px solid rgba(122, 147, 91, 0.12);
}

.model-config-reference-title {
  font-size: 12px;
  font-weight: 700;
  color: #24311f;
  margin-bottom: 4px;
}

.model-config-reference-text {
  font-size: 11px;
  line-height: 1.5;
  color: #708067;
}

.model-config-toolbar-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.model-config-scroll {
  max-height: 48vh;
  overflow: auto;
  padding-right: 4px;
}

.model-config-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.model-config-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.model-config-field-full {
  grid-column: 1 / -1;
}

.model-config-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 700;
  color: #4f6047;
}

.param-help-icon {
  color: #8a957f;
  cursor: help;
}

.model-modality-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  min-height: 36px;
  padding: 8px 10px;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.82);
  border: 1px solid rgba(122, 147, 91, 0.14);
}

.model-modality-item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: #51604a;
}

:deep(.provider-dialog .el-dialog),
:deep(.model-dialog .el-dialog),
:deep(.model-config-dialog .el-dialog) {
  background: linear-gradient(180deg, rgba(251, 252, 247, 0.98), rgba(246, 248, 239, 0.96));
  border: 1px solid rgba(122, 147, 91, 0.18);
  box-shadow: 0 24px 60px rgba(67, 86, 50, 0.18);
}

:deep(.model-config-dialog .el-dialog) {
  width: min(760px, calc(100vw - 36px)) !important;
}

:deep(.provider-dialog .el-dialog__title),
:deep(.model-dialog .el-dialog__title),
:deep(.model-config-dialog .el-dialog__title) {
  color: #24311f;
}

:deep(.provider-dialog .el-dialog__header),
:deep(.model-dialog .el-dialog__header),
:deep(.model-config-dialog .el-dialog__header) {
  margin-right: 0;
  padding-bottom: 8px;
  border-bottom: 1px solid rgba(122, 147, 91, 0.1);
}

:deep(.model-config-dialog .el-dialog__body) {
  padding-top: 16px;
  padding-bottom: 12px;
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
:deep(.model-dialog .el-button),
:deep(.model-config-dialog .el-button) {
  --el-button-bg-color: rgba(255, 255, 255, 0.78);
  --el-button-border-color: rgba(122, 147, 91, 0.18);
  --el-button-text-color: #607057;
}

.model-config-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  width: 100%;
}

.model-config-footer-tip {
  font-size: 12px;
  color: #7a846f;
  text-align: left;
}

.model-config-footer-actions {
  display: flex;
  align-items: center;
  gap: 10px;
}

.model-check-list :deep(.el-checkbox__label) {
  color: #51604a;
}

@media (max-width: 1200px) {
  .agent-panel {
    width: min(440px, calc(100vw - 32px));
  }

  .model-config-toolbar {
    flex-direction: column;
    align-items: stretch;
  }

  .model-config-toolbar-actions {
    justify-content: flex-start;
  }

  .model-config-footer {
    flex-direction: column;
    align-items: stretch;
  }

  .model-config-footer-actions {
    justify-content: flex-end;
  }
}

@media (max-width: 720px) {
  .agent-panel {
    width: calc(100vw - 24px);
  }

  .agent-shell {
    height: min(760px, calc(100vh - 40px));
    min-height: 420px;
    max-height: calc(100vh - 40px);
  }

  .agent-main {
    flex-direction: column;
  }

  .agent-sidebar {
    width: 100%;
    max-height: 34vh;
    border-right: none;
    border-bottom: 1px solid rgba(122, 147, 91, 0.12);
  }

  .agent-inline-selects {
    grid-template-columns: 1fr;
  }

  .agent-toolbar-line,
  .agent-bottom-bar {
    flex-direction: column;
    align-items: stretch;
  }

  .agent-attachment-list {
    flex: none;
  }

  .agent-session-line,
  .agent-session-select,
  .agent-controls {
    width: 100%;
  }

  .provider-manager,
  .model-grid,
  .model-pane-head-row,
  .mcp-form-grid {
    grid-template-columns: 1fr;
  }

  .model-config-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
