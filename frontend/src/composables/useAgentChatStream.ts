// 对话运行时 composable：从 AgentPanel 抽出的会话发送、SSE 流式处理、
// 工具循环、计划推进与执行状态机。组件通过 deps 注入所需的响应式状态与
// 少量宿主函数（会话持久化、滚动、附件清理等），其余纯逻辑来自 runtime-helpers。

import { nextTick, type ComputedRef, type Ref } from 'vue'
import { ElMessage } from 'element-plus'

import request from '@/utils/request'
import { executeAgentToolCalls, type AgentToolCall, type AgentToolOutputPayload } from '@/utils/agentTools'
import {
  AGENT_WRITER_RESULT_EVENT,
  type AgentWriterResultDetail,
} from '@/utils/agentWriter'
import { getAgentEditorBridge } from '@/utils/agentEditorBridge'
import { getAgentEditorSnapshot } from '@/utils/agentWriter'
import { hasDocDraft } from '@/utils/docDraftCache'
import {
  resolveAgentEditorSnapshotSource,
  type AgentControlBlock,
  type AgentPageScope,
} from '@/agent/protocol'
import { genId } from '@/components/agent/id'
import { buildDebugExecutionSnapshot, logAgentDebugGroup, logAgentModelIo, logAgentPanelError } from '@/components/agent/debug'
import {
  appendRecentToolCalls,
  buildControlFromExecutionState,
  buildConversationMessages,
  buildExecutionProgressSignature,
  buildRoundToolCallSummaryFromToolEvent,
  buildRuntimePlanFromText,
  buildSessionMemory,
  coerceBooleanLike,
  compactMessageText,
  extractPlanBlock,
  extractSaveNoopState,
  hasSuccessfulMutationToolCall,
  isAgentInternalInterceptError,
  isDocumentMutationTool,
  isHostStateTool,
  isLikelySaveFollowUpReply,
  isPartialWriteTask,
  isReadOrSaveOnlyBatch,
  normalizeControlPayload,
  normalizeRuntimePlan,
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
  summarizeRoundActions,
  summarizeToolCallBatch
} from '@/components/agent/runtime-helpers'
import type {
  AgentArtifact,
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
  DocType,
  PageScope,
  StreamAction,
} from '@/components/agent/types'
import type { ProviderSummary } from '@/api/agentProvider'

const REQUEST_SUMMARY_TRIGGER_CHARS = 6000
const MAX_SEMANTIC_CONTINUATION_ROUNDS = 128
const MAX_TOOL_CALL_ROUNDS = 1000
const MAX_REPEAT_TOOL_SIGNATURE_HITS = 4

export interface AgentChatStreamProps {
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
}

export interface AgentChatStreamDeps {
  props: AgentChatStreamProps
  emit: (event: 'navigate', target: AgentRouteTarget) => void
  // refs
  streaming: Ref<boolean>
  sessions: Ref<AgentSession[]>
  liveAssistantContent: Ref<string>
  liveAssistantReasoning: Ref<string>
  streamingAssistantId: Ref<string>
  agentTransportMode: Ref<'responses' | 'chat_fallback' | 'chat' | ''>
  prompt: Ref<string>
  composerAttachments: Ref<any[]>
  attachmentUploading: ComputedRef<boolean>
  showProviderDialog: Ref<boolean>
  // computed
  activeProvider: ComputedRef<ProviderSummary | null>
  currentConfirmationPlan: ComputedRef<string>
  currentPlanNeedsConfirmation: ComputedRef<boolean>
  // host functions
  ensureSession: () => AgentSession
  syncSessionWithActiveProvider: (session: AgentSession | null) => void
  clearComposerAttachments: () => void
  scrollMessagesToBottom: () => void
  persistSessions: () => void
  openProviderDialog: () => void
}

export function useAgentChatStream(deps: AgentChatStreamDeps) {
  const {
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
    attachmentUploading,
    showProviderDialog,
    activeProvider,
    currentConfirmationPlan,
    currentPlanNeedsConfirmation,
    ensureSession,
    syncSessionWithActiveProvider,
    clearComposerAttachments,
    scrollMessagesToBottom,
    persistSessions,
    openProviderDialog,
  } = deps

  let activeStreamController: AbortController | null = null


  function currentDocumentHasUnsavedChanges() {
    if (!props.docId) return false
    const liveBridge = getAgentEditorBridge()
    if (liveBridge?.docId === props.docId) {
      return liveBridge.getValue() !== (props.docContent ?? '')
    }
    const snapshot = getAgentEditorSnapshot(props.docId)
    if (snapshot !== null) {
      return snapshot !== (props.docContent ?? '')
    }
    if (hasDocDraft(props.docId)) return true
    return false
  }

  function currentEditorAvailable() {
    const liveBridge = getAgentEditorBridge()
    return Boolean(liveBridge && (!props.docId || liveBridge.docId === props.docId))
  }

  function currentEditorSnapshotSource() {
    const liveBridge = getAgentEditorBridge()
    return resolveAgentEditorSnapshotSource({
      hasLiveEditor: Boolean(liveBridge?.docId === props.docId),
      hasAgentSnapshot: Boolean(props.docId && getAgentEditorSnapshot(props.docId) !== null),
      hasDraftCache: Boolean(props.docId && hasDocDraft(props.docId)),
    })
  }

  function hasActivePendingPlan(session: AgentSession) {
    return Boolean(session.pendingPlan?.trim())
  }

  function isNeedsSaveContinuation(session: AgentSession) {
    if (hasActivePendingPlan(session)) return false
    return session.lastExecutionMemory?.awaiting === 'user_confirm_write'
  }

  function appendToolEventsToSession(session: AgentSession, roundToolCalls: AgentExecutionToolCallSummary[]) {
    if (!roundToolCalls.length) return
    const nextEvents = roundToolCalls.map((call) => ({
      id: genId(),
      tool: call.name,
      status: call.outcome,
      summary: call.outcome === 'noop'
        ? `${call.name} 未产生新的状态变更`
        : call.outcome === 'error'
          ? `${call.name} 执行失败`
          : `${call.name} 已执行`,
    } satisfies AgentToolEvent))
    session.toolEvents = [...session.toolEvents, ...nextEvents].slice(-20)
  }

  function mergePendingPlanToolOutputs(
    existing: AgentToolOutputPayload[],
    nextBatch: AgentToolOutputPayload[],
  ) {
    if (!nextBatch.length) return existing
    const merged = [...existing]
    const indexByCallId = new Map(merged.map((item, index) => [item.call_id, index]))
    for (const output of nextBatch) {
      const existingIndex = indexByCallId.get(output.call_id)
      if (existingIndex !== undefined) {
        merged[existingIndex] = output
        continue
      }
      indexByCallId.set(output.call_id, merged.length)
      merged.push(output)
    }
    return merged.slice(-8)
  }

  function syncRuntimePlanStatus(
    session: AgentSession,
    control: AgentControlBlock | null,
    executionState: AgentExecutionState,
  ) {
    const runtimePlan = session.runtimePlan
    if (!runtimePlan) return
    const planStepIndex = Number.isFinite(control?.planStepIndex) ? Number(control?.planStepIndex) : executionState.planStepIndex
    const actionStatus = typeof control?.currentActionStatus === 'string' && control.currentActionStatus.trim()
      ? control.currentActionStatus.trim()
      : executionState.currentActionStatus?.trim() || ''
    const awaitingConfirmation = (
      (control?.currentMode || executionState.currentMode) === 'plan'
      && (control?.awaiting || executionState.awaiting) === 'user_input'
    )
    const planCurrentStep = typeof control?.planCurrentStep === 'string' && control.planCurrentStep.trim()
      ? control.planCurrentStep.trim()
      : executionState.planCurrentStep?.trim() || ''
    const completedStepTitles = (
      (Array.isArray(control?.planCompletedSteps) && control?.planCompletedSteps?.length
        ? control.planCompletedSteps
        : executionState.planCompletedSteps
      )
        .map((step) => step.trim())
        .filter(Boolean)
    )
    const completedIndices = new Set(resolvePlanStepIndicesByTitles(runtimePlan.steps, completedStepTitles))
    let activeIndex = Number.isFinite(planStepIndex) && (planStepIndex || 0) > 0
      ? Number(planStepIndex) - 1
      : -1
    if (activeIndex < 0 && planCurrentStep) {
      activeIndex = resolveCurrentPlanStepIndexByTitle(runtimePlan.steps, planCurrentStep, completedIndices)
    }
    if (activeIndex < 0) {
      activeIndex = runtimePlan.steps.findIndex((_, index) => !completedIndices.has(index))
    }
    const lastStepIndex = runtimePlan.steps.length - 1
    const allStepsExplicitlyCompleted = runtimePlan.steps.every((_, index) => completedIndices.has(index))

    runtimePlan.steps = runtimePlan.steps.map((step, index) => {
      const explicitCompleted = completedIndices.has(index)
      if (explicitCompleted) return { ...step, status: 'completed' }
      if (awaitingConfirmation) return { ...step, status: 'pending' }
      if (activeIndex >= 0 && index < activeIndex) return { ...step, status: 'completed' }
      if (activeIndex >= 0 && index === activeIndex) {
        if (actionStatus === 'failed') return { ...step, status: 'failed' }
        if (actionStatus === 'completed' && (allStepsExplicitlyCompleted || index === lastStepIndex)) {
          return { ...step, status: 'completed' }
        }
        return {
          ...step,
          status: 'running',
        }
      }
      return { ...step, status: 'pending' }
    })

    const hasRunningSteps = runtimePlan.steps.some((step) => step.status === 'running')
    const hasPendingSteps = runtimePlan.steps.some((step) => step.status === 'pending')
    const hasFailedSteps = runtimePlan.steps.some((step) => step.status === 'failed')
    const allStepsCompleted = runtimePlan.steps.every((step) => step.status === 'completed')
    const hasStartedSteps = runtimePlan.steps.some((step) => step.status === 'running' || step.status === 'completed')
    if (hasFailedSteps) runtimePlan.status = 'failed'
    else if (awaitingConfirmation) runtimePlan.status = 'pending'
    else if (allStepsCompleted) runtimePlan.status = 'completed'
    else if (hasRunningSteps || hasStartedSteps) runtimePlan.status = 'running'
    else if (hasPendingSteps) runtimePlan.status = 'pending'
    runtimePlan.updatedAt = new Date().toISOString()
  }

  function currentRuntimePlanStep(
    runtimePlan: AgentRuntimePlan | null,
    executionState: AgentExecutionState,
    control: AgentControlBlock | null = null,
  ) {
    if (!runtimePlan?.steps?.length) return null
    const planStepIndex = Number.isFinite(control?.planStepIndex)
      ? Number(control?.planStepIndex)
      : executionState.planStepIndex
    if (planStepIndex && planStepIndex > 0 && runtimePlan.steps[planStepIndex - 1]) {
      return runtimePlan.steps[planStepIndex - 1]
    }
    const planCurrentStep = typeof control?.planCurrentStep === 'string' && control.planCurrentStep.trim()
      ? control.planCurrentStep.trim()
      : executionState.planCurrentStep?.trim() || ''
    if (planCurrentStep) {
      return runtimePlan.steps.find((step) => step.title.trim() === planCurrentStep) || null
    }
    return runtimePlan.steps.find((step) => step.status === 'running') || null
  }

  function currentStepRequiresDocumentWrite(
    runtimePlan: AgentRuntimePlan | null,
    executionState: AgentExecutionState,
    control: AgentControlBlock | null = null,
  ) {
    const runtimeStep = currentRuntimePlanStep(runtimePlan, executionState, control)
    if (runtimeStep?.requiresDocumentWrite === true) return true
    if (runtimePlan?.steps?.length) return false
    return Boolean(
      (control?.writeScope || control?.preferredWriteAction)
      && (control?.writeCompleted !== true && executionState.writeCompleted !== true),
    )
  }

  function isAwaitingPlanConfirmation(
    executionState: AgentExecutionState,
    control: AgentControlBlock | null = null,
  ) {
    if (control?.currentMode === 'plan' && control?.awaiting === 'user_input') return true
    if (!executionState.confirmationRequired) return false
    if (executionState.pendingPlanUserReply?.trim()) return false
    return executionState.planConfirmationDecision !== 'approved'
  }

  function currentStepRequiresDocumentSave(
    runtimePlan: AgentRuntimePlan | null,
    executionState: AgentExecutionState,
    control: AgentControlBlock | null = null,
  ) {
    const runtimeStep = currentRuntimePlanStep(runtimePlan, executionState, control)
    if (runtimeStep?.requiresDocumentSave === true) return true
    return executionState.saveRequested || control?.saveRequested === true
  }

  function currentPlanStepKey(
    executionState: AgentExecutionState,
    control: AgentControlBlock | null = null,
  ) {
    const planStepIndex = Number.isFinite(control?.planStepIndex)
      ? Number(control?.planStepIndex)
      : executionState.planStepIndex
    if (planStepIndex && planStepIndex > 0) {
      return `index:${planStepIndex}`
    }
    const planCurrentStep = typeof control?.planCurrentStep === 'string' && control.planCurrentStep.trim()
      ? control.planCurrentStep.trim()
      : executionState.planCurrentStep?.trim() || ''
    return planCurrentStep ? `title:${planCurrentStep}` : null
  }

  function hasRemainingStructuredPlanWork(
    runtimePlan: AgentRuntimePlan | null,
    executionState: AgentExecutionState,
    control: AgentControlBlock | null = null,
  ) {
    if (!runtimePlan?.steps?.length) return false
    if (runtimePlan.status === 'failed' || runtimePlan.status === 'blocked') {
      return false
    }

    const planStepIndex = Number.isFinite(control?.planStepIndex)
      ? Number(control?.planStepIndex)
      : executionState.planStepIndex

    if (planStepIndex && runtimePlan.steps.length >= planStepIndex) {
      return runtimePlan.steps.slice(planStepIndex - 1).some((step) => step.status !== 'completed')
    }

    return runtimePlan.steps.some((step) => step.status === 'pending' || step.status === 'running')
  }

  function advanceExecutionPlanStep(
    runtimePlan: AgentRuntimePlan | null,
    executionState: AgentExecutionState,
    control: AgentControlBlock | null,
    options: {
      wroteDocument: boolean
      savedDocument: boolean
      mutationCompleted?: boolean
    },
  ) {
    if (!runtimePlan?.steps?.length || executionState.pendingPlan) return false

    const currentStep = currentRuntimePlanStep(runtimePlan, executionState, control)
    if (!currentStep) return false

    const currentIndex = runtimePlan.steps.findIndex((step) => step.id === currentStep.id)
    if (currentIndex < 0) return false

    const writeSatisfied = options.wroteDocument || executionState.writeCompleted || control?.writeCompleted === true
    const saveSatisfied = options.savedDocument
    const mutationSatisfied = options.mutationCompleted === true
    const requiresWrite = currentStepRequiresDocumentWrite(runtimePlan, executionState, control)
    const requiresSave = currentStepRequiresDocumentSave(runtimePlan, executionState, control)
    const canAdvance = saveSatisfied
      || (requiresWrite && writeSatisfied && !requiresSave)
      || (!requiresWrite && mutationSatisfied && !requiresSave)
    if (!canAdvance) return false

    const currentTitle = currentStep.title.trim()
    if (currentTitle && !executionState.planCompletedSteps.includes(currentTitle)) {
      executionState.planCompletedSteps = [...executionState.planCompletedSteps, currentTitle]
    }

    const nextStep = runtimePlan.steps[currentIndex + 1] || null
    executionState.planStepIndex = nextStep ? currentIndex + 2 : currentIndex + 1
    executionState.planCurrentStep = nextStep ? nextStep.title : currentStep.title
    executionState.writeCompleted = false
    executionState.saveRequested = false
    return true
  }

  function upsertArtifactDraft(
    session: AgentSession,
    contentDelta: string,
    options: { finalize?: boolean; docId?: number | null; docName?: string | null } = {},
  ) {
    const title = options.docName?.trim() ? `${options.docName.trim()} 草稿` : 'Markdown 草稿'
    const relatedDocId = Number.isFinite(options.docId) ? Number(options.docId) : null
    let artifact = session.artifacts.find((item) => item.status === 'drafting' && item.relatedDocId === relatedDocId) || null
    if (!artifact) {
      artifact = {
        id: genId(),
        type: 'markdown_doc',
        title,
        status: 'drafting',
        content: '',
        relatedDocId,
      }
      session.artifacts = [...session.artifacts, artifact].slice(-6)
    }
    artifact.content = `${artifact.content}${contentDelta}`
    artifact.status = options.finalize ? 'ready' : 'drafting'
  }

  function buildRequestBody(
    messages: AgentRequestMessage[],
    provider: ProviderSummary,
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
        attachments: (message.attachments || []).map((attachment) => ({
          upload_id: attachment.uploadId,
          kind: attachment.kind,
          name: attachment.name,
          url: attachment.url,
          content_type: attachment.contentType,
          size: attachment.size,
        })),
      })),
      mode: 'auto',
      context: {
        page_scope: props.pageScope,
        page_state: props.pageState || null,
        project_name: props.projectName || null,
        doc_id: props.docId,
        doc_name: props.docName || null,
        project_catalog: props.projectCatalog || null,
        current_node_catalog: props.currentNodeCatalog || null,
        editor_available: currentEditorAvailable(),
        editor_snapshot_source: currentEditorSnapshotSource(),
        editor_unsaved_changes: currentDocumentHasUnsavedChanges(),
      },
      previous_response_id: options.previousResponseId || null,
      tool_outputs: options.toolOutputs || null,
    }
  }

  function stopStreaming() {
    activeStreamController?.abort()
  }

  async function sendMessage() {
    if (streaming.value) return
    const session = ensureSession()
    const text = prompt.value.trim()
    const attachments = [...composerAttachments.value]
    if (!text && !attachments.length) return
    if (attachmentUploading.value) {
      ElMessage.warning('附件仍在上传中，请稍候再发送')
      return
    }

    const provider = activeProvider.value
    if (!provider) {
      ElMessage.warning('请先配置并激活一个供应商')
      openProviderDialog()
      return
    }
    if (!provider.has_api_key) {
      ElMessage.warning('当前激活供应商缺少 API Key')
      openProviderDialog()
      return
    }

    syncSessionWithActiveProvider(session)
    const pendingPlan = (session.pendingPlan?.trim() || currentConfirmationPlan.value || '').trim()
    const continuingNeedsSave = !pendingPlan && isNeedsSaveContinuation(session) && isLikelySaveFollowUpReply(text)
    const shouldPreserveExecutionContext = Boolean(pendingPlan || continuingNeedsSave)
    const carriedPendingPlanToolOutputs = shouldPreserveExecutionContext
      ? [...session.pendingPlanToolOutputs]
      : []
    let collectedPlanToolOutputs = shouldPreserveExecutionContext
      ? [...session.pendingPlanToolOutputs]
      : []
    if (!shouldPreserveExecutionContext) {
      session.pendingPlan = null
      session.pendingPlanToolOutputs = []
      session.runtimePlan = null
      session.taskAnalysis = null
      session.lastPlan = ''
    }
    const existingMessages = [...session.messages]

    if (!session.model.trim()) {
      ElMessage.warning('请先在供应商管理中配置模型')
      showProviderDialog.value = true
      return
    }

    const initialMessageTime = Date.now()
    const userMessage: AgentMessage = {
      id: genId(),
      role: 'user',
      content: text,
      attachments,
      createdAt: initialMessageTime,
      updatedAt: initialMessageTime,
    }
    let assistantMessage: AgentMessage = {
      id: genId(),
      role: 'assistant',
      content: '',
      reasoning: '',
      createdAt: initialMessageTime,
      updatedAt: initialMessageTime,
    }
    session.messages.push(userMessage)
    session.messages.push(assistantMessage)
    session.updatedAt = Date.now()
    session.providerId = provider.id

    if (session.title === '新会话') {
      session.title = text
        ? text.slice(0, 18)
        : attachments[0]?.name?.slice(0, 18) || '附件会话'
    }

    sessions.value = [...sessions.value]
    persistSessions()
    scrollMessagesToBottom()
    prompt.value = ''
    clearComposerAttachments()
    streaming.value = true
    streamingAssistantId.value = assistantMessage.id
    liveAssistantContent.value = ''
    liveAssistantReasoning.value = ''
    let assistantMessageTimestampSettled = false
    const markAssistantMessageStarted = () => {
      if (assistantMessageTimestampSettled) return
      const now = Date.now()
      assistantMessage.createdAt = now
      assistantMessage.updatedAt = now
      assistantMessageTimestampSettled = true
    }
    let routeAction: StreamAction | null = null
    let writerStarted = false
    let renderBuffer = ''
    let inThinkBlock = false
    let streamFailed = false
    let pendingToolOutputs: AgentToolOutputPayload[] | null = null
    let streamAborted = false
    let currentAgentRunId = ''
    const pendingFrontendToolRequests: Promise<void>[] = []
    let previousResponseId: string | null = (
      session.previousResponseId
      && session.previousResponseId.trim()
      && session.previousResponseId.trim() !== session.model.trim()
    )
      ? session.previousResponseId.trim()
      : null
    if (session.previousResponseId && !previousResponseId) {
      session.previousResponseId = null
    }
    const pendingPlanSteps = pendingPlan ? parsePlanSteps(pendingPlan) : []
    const continuationMemory = pendingPlan || continuingNeedsSave
      ? session.lastExecutionMemory
      : null
    const compositeWriteThenSaveRequest = false
    const executionState: AgentExecutionState = {
      currentMode: pendingPlan || session.runtimePlan?.steps?.length
        ? 'plan'
        : continuationMemory?.currentMode || 'normal',
      awaiting: pendingPlan ? 'user_input' : continuationMemory?.awaiting ?? null,
      currentActionKind: continuationMemory?.currentActionKind ?? null,
      currentActionStatus: continuationMemory?.currentActionStatus ?? null,
      currentActionMode: continuationMemory?.currentActionMode ?? null,
      currentActionTarget: continuationMemory?.currentActionTarget ?? null,
      confirmationRequired: Boolean(pendingPlan),
      pendingPlan: pendingPlan || null,
      pendingPlanUserReply: pendingPlan ? text : null,
      planConfirmationDecision: null,
      compositeWriteThenSave: compositeWriteThenSaveRequest,
      semanticContinuation: false,
      semanticContinuationRound: 0,
      previousAssistantSummary: null,
      taskKind: continuingNeedsSave ? continuationMemory?.taskKind ?? null : null,
      editIntent: continuingNeedsSave ? continuationMemory?.editIntent ?? null : null,
      editStage: continuingNeedsSave ? continuationMemory?.editStage ?? null : null,
      saveRequested: continuingNeedsSave,
      writeCompleted: continuingNeedsSave
        ? Boolean(continuationMemory?.writeCompleted || continuationMemory?.documentWriteObserved)
        : false,
      planStepIndex: continuationMemory
        ? continuationMemory.planStepIndex ?? (pendingPlanSteps.length ? 1 : null)
        : null,
      planTotalSteps: continuationMemory
        ? continuationMemory.planTotalSteps ?? (pendingPlanSteps.length || null)
        : null,
      planCurrentStep: continuationMemory
        ? continuationMemory.planCurrentStep ?? pendingPlanSteps[0] ?? null
        : null,
      planCompletedSteps: continuationMemory ? [...(continuationMemory.planCompletedSteps || [])] : [],
      documentWriteObserved: continuingNeedsSave
        ? Boolean(continuationMemory?.documentWriteObserved || continuationMemory?.writeCompleted)
        : false,
      saveAttemptWithoutDocumentChange: false,
      lastInterceptCode: null,
      lastInterceptMessage: null,
      lastInterceptGuidance: null,
      recentToolCalls: [],
    }
    let semanticContinuationRounds = 0
    let toolCallRounds = 0
    const toolCallSignatureHits = new Map<string, number>()
    let nonWritingPlanRounds = 0
    let idleSemanticContinuationRounds = 0
    let rawAssistantContent = ''
    let completedAssistantContent = ''
    let sawPlanSignalThisRound = false
    let wroteDocument = false
    let writeSatisfiedStepKey: string | null = null
    let liveAssistantRoundSummary = ''
    let saveToolAttemptedThisTurn = false
    let saveToolSucceededThisTurn = false
    let planStepAdvancedThisRound = false
    let roundDocumentWriteObserved = false
    let roundToolCalls: AgentExecutionToolCallSummary[] = []
    let consumeAssistantText = (_rawChunk: string, _force = false) => {}
    let handleAssistantChunk = (_rawChunk: string) => {}
    let routeChunk = (_rawChunk: string, _force = false) => {}
    let buildVisibleAssistantContent = (_source = rawAssistantContent, _streamMode = false) => _source
    let appendUnsavedDraftNotice = (_control: AgentControlBlock | null) => {}
    let finalizePendingAssistantOutput = (_options?: { allowIncompleteAction?: boolean }) => {}
    let finalizeAssistantMessageForDisplay = (_finalContent: string, _options?: { includeStructuredMessage?: boolean }) => {}
    let appendFinalLoopResultMessage = (_resultContent: string) => {}
    let syncExecutionPlanProgress = (_control: AgentControlBlock | null) => {}
    let syncPendingPlanPreviewFromStream = () => {}
    let finalControl: AgentControlBlock | null = null
    let latestStructuredResponse: AgentStructuredResponse | null = null
    let latestToolDrivenControl: AgentControlBlock | null = null
    let latestToolDrivenPlan: AgentRuntimePlan | null = null
    let hostStateUpdatedThisRound = false
    let lastWriterResult: AgentWriterResultDetail | null = null
    let interceptedFailure: ReturnType<typeof resolveAgentInterceptDetails> | null = null
    let runtimeSummaryAppendedThisRequest = false
    let requestRound = 0
    // 纯聊天分支：整个 turn 只维护一条 assistant 消息，正文/推理跨轮累积，不新建消息、不清空内容。
    // 文档写入模式（docType==='doc' 且有 docId）沿用原有每轮新建消息的逻辑，暂不改动。
    const chatOnlyMode = props.docType !== 'doc' || !props.docId
    // 聊天模式下已完成轮次的正文累积（不含当前进行中的轮）。显示时拼接：已提交 + 当前轮。
    let committedChatContent = ''
    // 整个 turn 的 token 消耗（后端在 message.completed 里聚合下发）。
    let turnUsage: Record<string, number> | null = null
    const handleWriterResult = (event: Event) => {
      const detail = (event as CustomEvent<AgentWriterResultDetail>).detail
      if (!detail || detail.docId !== props.docId) return
      lastWriterResult = detail
    }
    const readLatestWriterResult = (): AgentWriterResultDetail | null => lastWriterResult
    window.addEventListener(AGENT_WRITER_RESULT_EVENT, handleWriterResult as EventListener)

    try {
      const abortController = new AbortController()
      activeStreamController = abortController

      const resetSemanticContinuationBudget = () => {
        semanticContinuationRounds = 0
        executionState.semanticContinuation = false
        executionState.semanticContinuationRound = 0
        executionState.previousAssistantSummary = null
      }

      const shouldTriggerSemanticContinuation = (finalContent: string, control: AgentControlBlock | null) => {
        void finalContent
        void control
        return false
      }

      const enqueueSemanticContinuation = (finalContent: string) => {
        semanticContinuationRounds += 1
        executionState.semanticContinuation = true
        executionState.semanticContinuationRound = semanticContinuationRounds
        executionState.confirmationRequired = false
        executionState.pendingPlan = null
        executionState.pendingPlanUserReply = null
        executionState.planConfirmationDecision = null
        session.pendingPlan = null
        executionState.previousAssistantSummary =
          compactMessageText(finalContent, 320)
          || currentRuntimePlanStep(session.runtimePlan, executionState)?.title
          || executionState.planCurrentStep
          || '继续执行当前计划'
      }


      syncExecutionPlanProgress = (control: AgentControlBlock | null) => {
        if (!control) return
        executionState.currentMode = typeof control.currentMode === 'string' && control.currentMode.trim()
          ? control.currentMode.trim()
          : executionState.currentMode
        executionState.awaiting = typeof control.awaiting === 'string' && control.awaiting.trim()
          ? control.awaiting.trim()
          : control.awaiting === null
            ? null
            : executionState.awaiting
        executionState.currentActionKind = typeof control.currentActionKind === 'string' && control.currentActionKind.trim()
          ? control.currentActionKind.trim()
          : control.currentActionKind === null
            ? null
            : executionState.currentActionKind
        executionState.currentActionStatus = typeof control.currentActionStatus === 'string' && control.currentActionStatus.trim()
          ? control.currentActionStatus.trim()
          : control.currentActionStatus === null
            ? null
            : executionState.currentActionStatus
        executionState.currentActionMode = typeof control.currentActionMode === 'string' && control.currentActionMode.trim()
          ? control.currentActionMode.trim()
          : control.currentActionMode === null
            ? null
            : executionState.currentActionMode
        executionState.currentActionTarget = typeof control.currentActionTarget === 'string' && control.currentActionTarget.trim()
          ? control.currentActionTarget.trim()
          : control.currentActionTarget === null
            ? null
            : executionState.currentActionTarget
        if (control.confirmationRequired === true) {
          executionState.confirmationRequired = true
        }
        if (session.taskAnalysis) {
          if (typeof control.writeScope === 'string' && control.writeScope.trim()) {
            session.taskAnalysis.writeScope = control.writeScope.trim()
          }
          if (typeof control.preferredWriteAction === 'string' && control.preferredWriteAction.trim()) {
            session.taskAnalysis.preferredWriteAction = control.preferredWriteAction.trim()
          }
        }
        executionState.taskKind = typeof control.taskKind === 'string' && control.taskKind.trim()
          ? control.taskKind.trim()
          : executionState.taskKind
        executionState.editIntent = typeof control.editIntent === 'string' && control.editIntent.trim()
          ? control.editIntent.trim()
          : executionState.editIntent
        executionState.editStage = typeof control.editStage === 'string' && control.editStage.trim()
          ? control.editStage.trim()
          : executionState.editStage
        if (control.saveRequested === true) {
          executionState.saveRequested = true
        }
        if (control.writeCompleted === true) {
          executionState.writeCompleted = true
        }
        executionState.planStepIndex = Number.isFinite(control.planStepIndex) ? Number(control.planStepIndex) : executionState.planStepIndex
        executionState.planTotalSteps = Number.isFinite(control.planTotalSteps) ? Number(control.planTotalSteps) : executionState.planTotalSteps
        executionState.planCurrentStep = typeof control.planCurrentStep === 'string' && control.planCurrentStep.trim()
          ? control.planCurrentStep.trim()
          : executionState.planCurrentStep
        if (Array.isArray(control.planCompletedSteps) && control.planCompletedSteps.length) {
          executionState.planCompletedSteps = [...control.planCompletedSteps]
        }
        if (control.writeCompleted === true) {
          writeSatisfiedStepKey = currentPlanStepKey(executionState, control) || writeSatisfiedStepKey
        }
      }

      const hasImplicitControlSignals = () => Boolean(
        latestStructuredResponse?.state
        || latestToolDrivenControl
        || hostStateUpdatedThisRound
        || session.pendingPlan?.trim()
        || session.runtimePlan?.steps?.length
        || executionState.confirmationRequired
        || executionState.pendingPlan
        || executionState.planCurrentStep
        || executionState.planStepIndex
        || executionState.planCompletedSteps.length
        || executionState.saveRequested
        || executionState.writeCompleted
        || executionState.documentWriteObserved
        || roundToolCalls.length
        || roundDocumentWriteObserved
        || wroteDocument
      )

      const resolveEffectiveControl = (content: string) => {
        const explicitControl = latestStructuredResponse?.state || latestToolDrivenControl
        if (explicitControl) return explicitControl
        if (!hasImplicitControlSignals()) return null
        return buildControlFromExecutionState(executionState, session.taskAnalysis)
      }

      const setCurrentActionState = (
        kind: string | null,
        status: string | null,
        options: {
          mode?: string | null
          target?: string | null
        } = {},
      ) => {
        executionState.currentActionKind = kind
        executionState.currentActionStatus = status
        executionState.currentActionMode = options.mode ?? executionState.currentActionMode
        executionState.currentActionTarget = options.target ?? executionState.currentActionTarget
      }

      const adoptRuntimePlan = (plan: AgentRuntimePlan | null) => {
        if (!plan) return
        session.runtimePlan = plan
        latestToolDrivenPlan = plan
        const planText = runtimePlanToPlanText(plan)
        if (planText) {
          session.lastPlan = planText
        }
        executionState.planTotalSteps = plan.steps.length || null
      }

      const clearPendingPlanState = () => {
        executionState.confirmationRequired = false
        executionState.pendingPlan = null
        executionState.pendingPlanUserReply = null
        executionState.planConfirmationDecision = null
        session.pendingPlan = null
        session.pendingPlanToolOutputs = []
      }

      const syncPlanCursorToIndex = (plan: AgentRuntimePlan | null, index: number | null) => {
        executionState.planStepIndex = index
        if (!plan || !index || index < 1 || index > plan.steps.length) {
          executionState.planCurrentStep = null
          return
        }
        executionState.planCurrentStep = plan.steps[index - 1]?.title || null
      }

      const buildPlanFromHostArgs = (args: Record<string, any>) => {
        const explicitPlan = normalizeRuntimePlan(parseJsonLikeValue(args.plan))
        if (explicitPlan) return explicitPlan

        if (Array.isArray(args.steps)) {
          const normalizedPlan = normalizeRuntimePlan({
            id: typeof args.id === 'string' && args.id.trim() ? args.id.trim() : genId(),
            goal: typeof args.goal === 'string' && args.goal.trim() ? args.goal.trim() : (userMessage?.content || pendingPlan || '执行计划'),
            summary: typeof args.summary === 'string' && args.summary.trim() ? args.summary.trim() : null,
            status: 'pending',
            steps: args.steps,
          })
          if (normalizedPlan) return normalizedPlan
        }

        const explicitPlanText = typeof args.plan_text === 'string' && args.plan_text.trim()
          ? args.plan_text.trim()
          : typeof args.planText === 'string' && args.planText.trim()
            ? args.planText.trim()
            : ''
        if (!explicitPlanText) return null

        return buildRuntimePlanFromText(
          explicitPlanText,
          typeof args.goal === 'string' && args.goal.trim()
            ? args.goal.trim()
            : (userMessage?.content || pendingPlan || ''),
          session.runtimePlan,
        )
      }

      const flushOpenWriteStream = () => {
        if (!(routeAction && routeAction !== 'chat')) return
        routeChunk('', true)
        consumeAssistantText('', true)
        completeWriterBlock()
      }

      const executeHostStateToolCall = async (call: AgentToolCall): Promise<AgentToolOutputPayload> => {
        const args = parseToolArguments(call.arguments) || {}
        const control = normalizeControlPayload(args)
        let nextPlan = buildPlanFromHostArgs(args)

        switch (call.name) {
          case 'plan_start': {
            if (nextPlan) {
              adoptRuntimePlan(nextPlan)
              executionState.planCompletedSteps = []
              executionState.currentMode = 'plan'
              syncPlanCursorToIndex(nextPlan, nextPlan.steps.length ? 1 : null)
            }
            const shouldAwaitConfirmation = session.taskAnalysis?.requiresUserConfirmation !== false
            const pendingPlanText = (nextPlan ? runtimePlanToPlanText(nextPlan) : session.lastPlan || '').trim() || null
            if (shouldAwaitConfirmation && pendingPlanText) {
              executionState.confirmationRequired = true
              executionState.pendingPlan = pendingPlanText
              executionState.pendingPlanUserReply = null
              executionState.planConfirmationDecision = null
              executionState.awaiting = 'user_input'
              session.pendingPlan = pendingPlanText
            } else {
              clearPendingPlanState()
              executionState.awaiting = null
            }
            setCurrentActionState('plan_start', 'completed')
            break
          }
          case 'plan_step_update': {
            const stepId = typeof args.step_id === 'string' && args.step_id.trim()
              ? args.step_id.trim()
              : typeof args.stepId === 'string' && args.stepId.trim()
                ? args.stepId.trim()
                : ''
            const nextStatus = typeof args.status === 'string' && args.status.trim()
              ? args.status.trim()
              : 'completed'
            const plan = session.runtimePlan
            executionState.currentMode = 'plan'
            if (plan && stepId) {
              const stepIndex = plan.steps.findIndex((step) => step.id === stepId)
              if (stepIndex >= 0) {
                plan.steps[stepIndex] = {
                  ...plan.steps[stepIndex],
                  status: nextStatus === 'failed' ? 'failed' : 'completed',
                }
                if (nextStatus === 'failed') {
                  plan.status = 'failed'
                  executionState.awaiting = 'user_input'
                  syncPlanCursorToIndex(plan, stepIndex + 1)
                } else {
                  const completedTitle = plan.steps[stepIndex].title.trim()
                  if (completedTitle && !executionState.planCompletedSteps.includes(completedTitle)) {
                    executionState.planCompletedSteps.push(completedTitle)
                  }
                  const nextIndex = plan.steps.findIndex((step) => step.status !== 'completed')
                  if (nextIndex >= 0) {
                    plan.status = 'running'
                    executionState.awaiting = null
                    syncPlanCursorToIndex(plan, nextIndex + 1)
                  } else {
                    plan.status = 'completed'
                    executionState.awaiting = null
                    executionState.planStepIndex = plan.steps.length || null
                    executionState.planCurrentStep = null
                  }
                }
                plan.updatedAt = new Date().toISOString()
              }
            }
            setCurrentActionState('plan_step_update', nextStatus === 'failed' ? 'failed' : 'completed')
            break
          }
          case 'plan_complete': {
            if (session.runtimePlan) {
              session.runtimePlan.status = 'completed'
              session.runtimePlan.steps = session.runtimePlan.steps.map((step) => ({
                ...step,
                status: step.status === 'failed' ? step.status : 'completed',
              }))
              session.runtimePlan.updatedAt = new Date().toISOString()
              executionState.planCompletedSteps = session.runtimePlan.steps
                .map((step) => step.title.trim())
                .filter(Boolean)
            }
            executionState.currentMode = 'normal'
            executionState.awaiting = null
            executionState.planCurrentStep = null
            clearPendingPlanState()
            setCurrentActionState('plan_complete', 'completed')
            break
          }
          case 'plan_cancel': {
            if (session.runtimePlan) {
              session.runtimePlan.status = 'cancelled'
              session.runtimePlan.updatedAt = new Date().toISOString()
            }
            executionState.currentMode = 'normal'
            executionState.awaiting = null
            executionState.planCurrentStep = null
            clearPendingPlanState()
            setCurrentActionState('plan_cancel', 'completed')
            break
          }
          case 'write_start': {
            const mode = typeof args.mode === 'string' && args.mode.trim() ? args.mode.trim() : 'replace'
            const target = typeof args.target === 'string' && args.target.trim() ? args.target.trim() : 'full_doc'
            executionState.currentMode = session.runtimePlan?.steps?.length ? 'plan' : executionState.currentMode
            executionState.awaiting = null
            executionState.writeCompleted = false
            setCurrentActionState('write', 'running', { mode, target })
            if (props.docType === 'doc' && (mode === 'append' || mode === 'replace')) {
              routeAction = mode as StreamAction
            }
            break
          }
          case 'write_end': {
            const target = typeof args.target === 'string' && args.target.trim()
              ? args.target.trim()
              : executionState.currentActionTarget || 'full_doc'
            flushOpenWriteStream()
            executionState.writeCompleted = true
            if (roundDocumentWriteObserved || wroteDocument) {
              executionState.documentWriteObserved = true
              executionState.saveAttemptWithoutDocumentChange = false
              writeSatisfiedStepKey = currentPlanStepKey(executionState) || writeSatisfiedStepKey
            }
            setCurrentActionState('write', 'completed', {
              mode: executionState.currentActionMode,
              target,
            })
            break
          }
          default:
            break
        }

        if (control) {
          latestToolDrivenControl = control
          syncExecutionPlanProgress(control)
        }

        const explicitPendingPlan = Object.prototype.hasOwnProperty.call(args, 'pending_plan')
          ? coerceBooleanLike(args.pending_plan)
          : Object.prototype.hasOwnProperty.call(args, 'pendingPlan')
            ? coerceBooleanLike(args.pendingPlan)
            : null
        const pendingPlanText = (nextPlan ? runtimePlanToPlanText(nextPlan) : '').trim()
        if (explicitPendingPlan === true) {
          const nextPendingPlan = pendingPlanText || session.lastPlan || executionState.pendingPlan || pendingPlan || null
          executionState.confirmationRequired = true
          executionState.pendingPlan = nextPendingPlan
          executionState.pendingPlanUserReply = null
          executionState.planConfirmationDecision = null
          executionState.awaiting = 'user_input'
          session.pendingPlan = nextPendingPlan
        } else if (explicitPendingPlan === false || control?.awaiting !== 'user_input') {
          clearPendingPlanState()
        }

        if (control?.currentMode === 'plan' && control?.awaiting === 'user_input') {
          executionState.confirmationRequired = true
          executionState.awaiting = 'user_input'
        } else if (control?.awaiting === 'user_confirm_write') {
          executionState.confirmationRequired = false
          executionState.awaiting = 'user_confirm_write'
        } else if (control?.awaiting == null) {
          executionState.confirmationRequired = false
          executionState.awaiting = null
        }

        if (Object.prototype.hasOwnProperty.call(args, 'save_requested')) {
          executionState.saveRequested = coerceBooleanLike(args.save_requested) === true
        } else if (Object.prototype.hasOwnProperty.call(args, 'saveRequested')) {
          executionState.saveRequested = coerceBooleanLike(args.saveRequested) === true
        }
        if (Object.prototype.hasOwnProperty.call(args, 'write_completed')) {
          executionState.writeCompleted = coerceBooleanLike(args.write_completed) === true
        } else if (Object.prototype.hasOwnProperty.call(args, 'writeCompleted')) {
          executionState.writeCompleted = coerceBooleanLike(args.writeCompleted) === true
        }

        latestToolDrivenControl = buildControlFromExecutionState(executionState, session.taskAnalysis)

        hostStateUpdatedThisRound = true
        session.updatedAt = Date.now()
        sessions.value = [...sessions.value]

        return {
          call_id: call.call_id,
          name: call.name,
          arguments: call.arguments,
          output: {
            ok: true,
            tool: call.name,
            result: {
              state_updated: true,
              plan_updated: Boolean(nextPlan),
              current_mode: executionState.currentMode,
              awaiting: executionState.awaiting,
              current_action_kind: executionState.currentActionKind,
              current_action_status: executionState.currentActionStatus,
              pending_plan: executionState.pendingPlan,
              phase: latestToolDrivenControl?.phase || buildControlFromExecutionState(executionState, session.taskAnalysis).phase || null,
            },
          },
        }
      }

      const executeRoundToolCalls = async (calls: AgentToolCall[]) => {
        const hostStateCalls = calls.filter((call) => isHostStateTool(call.name))
        const externalCalls = calls.filter((call) => !isHostStateTool(call.name))
        const outputs: AgentToolOutputPayload[] = []

        for (const call of hostStateCalls) {
          outputs.push(await executeHostStateToolCall(call))
        }

        if (externalCalls.length) {
          outputs.push(...await executeAgentToolCalls(externalCalls))
        }

        return outputs
      }

      const submitFrontendToolResult = async (
        runId: string,
        callId: string,
        output: unknown,
      ) => {
        const token = localStorage.getItem('token')
        const response = await fetch('/api/agent/tool-callback', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
          },
          body: JSON.stringify({
            run_id: runId,
            call_id: callId,
            output,
          }),
        })

        if (!response.ok) {
          const data = await response.json().catch(() => ({}))
          throw new Error(data.error || '前端工具结果回传失败')
        }
      }

      const executeFrontendToolRequest = async (eventData: any) => {
        const runId = typeof eventData?.run_id === 'string' && eventData.run_id.trim()
          ? eventData.run_id.trim()
          : currentAgentRunId
        const callId = typeof eventData?.call_id === 'string' && eventData.call_id.trim()
          ? eventData.call_id.trim()
          : ''
        const toolName = typeof eventData?.name === 'string' && eventData.name.trim()
          ? eventData.name.trim()
          : ''
        const args = eventData?.arguments && typeof eventData.arguments === 'object'
          ? eventData.arguments
          : {}

        if (!runId || !callId || !toolName) {
          return
        }

        let callbackOutput: unknown
        try {
          const outputs = await executeAgentToolCalls([{
            call_id: callId,
            name: toolName,
            arguments: JSON.stringify(args),
          }])
          callbackOutput = outputs[0]?.output ?? {
            ok: false,
            tool: toolName,
            error: '工具没有返回结果',
          }
        } catch (error: any) {
          callbackOutput = {
            ok: false,
            tool: toolName,
            error: error?.message || '前端工具执行失败',
          }
        }

        await submitFrontendToolResult(runId, callId, callbackOutput)
      }

      const applyLiveHostToolDelta = (eventData: any) => {
        const toolName = typeof eventData?.name === 'string' ? eventData.name.trim() : ''
        if (!toolName || !isHostStateTool(toolName)) return
        const args = parseToolArguments(typeof eventData?.arguments === 'string' ? eventData.arguments : null) || {}

        if (toolName === 'write_start') {
          const mode = typeof args.mode === 'string' && args.mode.trim() ? args.mode.trim() : 'replace'
          const target = typeof args.target === 'string' && args.target.trim() ? args.target.trim() : 'full_doc'
          executionState.currentMode = session.runtimePlan?.steps?.length ? 'plan' : executionState.currentMode
          executionState.awaiting = null
          executionState.writeCompleted = false
          setCurrentActionState('write', 'running', { mode, target })
          if (props.docType === 'doc' && (mode === 'append' || mode === 'replace')) {
            routeAction = mode as StreamAction
          }
        } else if (toolName === 'write_end') {
          const target = typeof args.target === 'string' && args.target.trim()
            ? args.target.trim()
            : executionState.currentActionTarget || 'full_doc'
          flushOpenWriteStream()
          setCurrentActionState('write', 'completed', {
            mode: executionState.currentActionMode,
            target,
          })
        }

        latestToolDrivenControl = buildControlFromExecutionState(executionState, session.taskAnalysis)
        hostStateUpdatedThisRound = true
      }

      syncPendingPlanPreviewFromStream = () => {
        if (latestStructuredResponse?.plan || latestStructuredResponse?.state || latestToolDrivenPlan || latestToolDrivenControl) {
          if (latestStructuredResponse?.plan) {
            session.runtimePlan = latestStructuredResponse.plan
            const planText = runtimePlanToPlanText(latestStructuredResponse.plan)
            if (planText) {
              session.lastPlan = planText
            }
          } else if (latestToolDrivenPlan) {
            session.runtimePlan = latestToolDrivenPlan
            const planText = runtimePlanToPlanText(latestToolDrivenPlan)
            if (planText) {
              session.lastPlan = planText
            }
          }
          return
        }
        const source = completedAssistantContent || rawAssistantContent
        const streamedControl = resolveEffectiveControl(source)
        const streamedPlan = extractPlanBlock(source)
        if (streamedPlan) {
          sawPlanSignalThisRound = true
        }
        const runtimePlanText = runtimePlanToPlanText(session.runtimePlan)
        const nextPlan = streamedPlan || runtimePlanText
        const requiresConfirmation = shouldTreatPlanAsPending({
          planText: nextPlan,
          control: streamedControl,
          session,
          executionState,
          sawPlanSignal: sawPlanSignalThisRound,
        })

        let changed = false

        if (nextPlan && session.lastPlan !== nextPlan) {
          session.lastPlan = nextPlan
          changed = true
        }

        if (!session.runtimePlan && nextPlan) {
          const fallbackRuntimePlan = buildRuntimePlanFromText(nextPlan, userMessage?.content || pendingPlan || '', session.runtimePlan)
          if (fallbackRuntimePlan) {
            session.runtimePlan = fallbackRuntimePlan
            changed = true
          }
        }

        if (requiresConfirmation && session.pendingPlan !== nextPlan) {
          session.pendingPlan = nextPlan
          changed = true
        }

        if (changed) {
          session.updatedAt = Date.now()
          sessions.value = [...sessions.value]
        }
      }

      const buildAssistantRoundSummary = (control: AgentControlBlock | null) => {
        const parts: string[] = []
        if (Number.isFinite(control?.planStepIndex) && Number.isFinite(control?.planTotalSteps) && control?.planCurrentStep) {
          parts.push(`当前计划进度：第 ${Number(control.planStepIndex)}/${Number(control.planTotalSteps)} 步，${control.planCurrentStep}。`)
        } else if (control?.planCurrentStep) {
          parts.push(`当前步骤：${control.planCurrentStep}。`)
        }
        if (roundDocumentWriteObserved) {
          parts.push(props.docName ? `已写入《${props.docName}》正文。` : '本轮已写入文档内容。')
        }
        parts.push(...summarizeRoundActions(roundToolCalls))
        if (control?.awaiting === 'user_confirm_write') {
          parts.push('当前文档仍未保存，正在等待保存决策。')
        } else if (control?.currentMode === 'plan' && !control?.awaiting && hasRemainingStructuredPlanWork(session.runtimePlan, executionState, control)) {
          parts.push('系统将继续执行后续步骤。')
        }
        return parts.filter(Boolean).join('\n')
      }

      const buildPendingPlanConfirmationMessage = () => {
        const planText = (session.pendingPlan?.trim() || runtimePlanToPlanText(session.runtimePlan)).trim()
        if (!planText) return '我已经整理好执行计划，请确认后我再继续。'
        return [
          '我准备按这个计划继续：',
          '',
          planText,
          '',
          '请确认是否继续执行？',
        ].join('\n')
      }

      const mergeVisibleAssistantContent = (base: string, appendix: string) => {
        const normalizedBase = base.trim()
        const normalizedAppendix = appendix.trim()
        if (!normalizedAppendix) return normalizedBase
        if (!normalizedBase) return normalizedAppendix
        if (normalizedBase.includes(normalizedAppendix)) return normalizedBase
        return `${normalizedBase}\n\n${normalizedAppendix}`.trim()
      }

      const syncLiveAssistantDisplay = (baseVisible: string) => {
        // 聊天模式：正文只做纯模型文本的线性累积，不注入合成的轮次动作摘要
        // （摘要与工具卡片重复，多轮下会插到错误位置导致乱序）。工具执行交给工具卡片展示。
        if (chatOnlyMode) {
          liveAssistantContent.value = mergeVisibleAssistantContent(committedChatContent, baseVisible)
          assistantMessage.content = liveAssistantContent.value
          assistantMessage.reasoning = liveAssistantReasoning.value
          assistantMessage.internalStatus = false
          scrollMessagesToBottom()
          return
        }
        const roundVisible = mergeVisibleAssistantContent(baseVisible, liveAssistantRoundSummary)
        liveAssistantContent.value = roundVisible
        assistantMessage.content = liveAssistantContent.value
        assistantMessage.reasoning = liveAssistantReasoning.value
        assistantMessage.internalStatus = false
        scrollMessagesToBottom()
      }

      // 聊天模式：把当前轮的纯模型正文并入跨轮累积器，供后续轮次线性拼接展示（不含合成摘要）。
      const commitCurrentRoundToChatContent = () => {
        const roundVisible = buildVisibleAssistantContent(completedAssistantContent || rawAssistantContent)
        committedChatContent = mergeVisibleAssistantContent(committedChatContent, roundVisible)
      }

      const ensureVisibleAssistantRoundSummary = (control: AgentControlBlock | null) => {
        // 聊天模式：不生成也不注入合成摘要，仅按纯模型正文刷新展示。
        if (chatOnlyMode) {
          syncLiveAssistantDisplay(buildVisibleAssistantContent(
            completedAssistantContent || rawAssistantContent,
            !completedAssistantContent,
          ))
          return
        }
        const fallback = buildAssistantRoundSummary(control)
        if (!fallback.trim()) return
        if (fallback !== liveAssistantRoundSummary) {
          runtimeSummaryAppendedThisRequest = true
          liveAssistantRoundSummary = fallback
        }
        const baseVisible = buildVisibleAssistantContent(
          completedAssistantContent || rawAssistantContent,
          !completedAssistantContent,
        )
        syncLiveAssistantDisplay(baseVisible)
      }

      appendFinalLoopResultMessage = (resultContent: string) => {
        const normalized = stripProtocolContent(resultContent).trim()
        if (!normalized) return
        // 单消息模型：一轮对话只对应一条助手消息，最终叙述并入当前消息，不再新建气泡。
        // 聊天模式下正文已跨轮累积，此处无需处理；文档模式把最终结果合并进当前消息。
        if (chatOnlyMode) return
        const merged = mergeVisibleAssistantContent(
          assistantMessage.content || liveAssistantContent.value || '',
          normalized,
        )
        assistantMessage.content = merged
        liveAssistantContent.value = merged
        session.updatedAt = Date.now()
        sessions.value = [...sessions.value]
        scrollMessagesToBottom()
      }

      finalizeAssistantMessageForDisplay = (
        finalContent: string,
        options: { includeStructuredMessage?: boolean } = {},
      ) => {
        if (options.includeStructuredMessage === true && latestStructuredResponse?.message.trim()) {
          const baseVisible = mergeVisibleAssistantContent(
            buildVisibleAssistantContent(completedAssistantContent || rawAssistantContent),
            latestStructuredResponse.message.trim(),
          )
          syncLiveAssistantDisplay(baseVisible)
        }
        const control = resolveEffectiveControl(finalContent)
        const hideAsRuntimeMessage = Boolean(
          (roundToolCalls.length || roundDocumentWriteObserved || executionState.documentWriteObserved)
          && !(control?.currentMode === 'plan' && control?.awaiting === 'user_input')
          && hasRemainingStructuredPlanWork(session.runtimePlan, executionState, control),
        )
        void hideAsRuntimeMessage
        ensureVisibleAssistantRoundSummary(control)
      }

      const maybeAutoSaveAfterDocumentWrite = async (control: AgentControlBlock | null) => {
        if (streamFailed || streamAborted) return
        if (props.docType !== 'doc' || !props.docId) return
        if (!(wroteDocument || executionState.writeCompleted || roundDocumentWriteObserved)) return
        if (!currentStepRequiresDocumentSave(session.runtimePlan, executionState, control)) return
        if (!currentDocumentHasUnsavedChanges()) return
        if (saveToolSucceededThisTurn) return

        const autoSaveOutputs = await executeAgentToolCalls([{
          call_id: genId(),
          name: 'save_current_document',
          arguments: JSON.stringify({ doc_id: props.docId }),
        }])
        if (!autoSaveOutputs.length) return

        pendingToolOutputs = mergePendingPlanToolOutputs(pendingToolOutputs || [], autoSaveOutputs)
        collectedPlanToolOutputs = mergePendingPlanToolOutputs(collectedPlanToolOutputs, autoSaveOutputs)

        const autoSaveCalls = autoSaveOutputs.map((output) => ({
          call_id: output.call_id,
          name: output.name || 'save_current_document',
          arguments: output.arguments || JSON.stringify({ doc_id: props.docId }),
          stage_policy: 'mutation',
          capabilities: ['save'],
        }))
        const autoSaveRoundCalls = summarizeToolCallBatch(autoSaveCalls, autoSaveOutputs)
        if (autoSaveRoundCalls.length) {
          roundToolCalls = [...roundToolCalls, ...autoSaveRoundCalls]
          executionState.recentToolCalls = appendRecentToolCalls(
            executionState.recentToolCalls,
            autoSaveRoundCalls,
          )
          appendToolEventsToSession(session, autoSaveRoundCalls)
        }

        saveToolAttemptedThisTurn = true
        if (autoSaveRoundCalls.some((call) => call.outcome === 'success' || call.outcome === 'noop')) {
          saveToolSucceededThisTurn = true
          executionState.saveRequested = false
          if (!planStepAdvancedThisRound) {
            planStepAdvancedThisRound = advanceExecutionPlanStep(session.runtimePlan, executionState, control, {
              wroteDocument: roundDocumentWriteObserved || wroteDocument,
              savedDocument: true,
              mutationCompleted: hasSuccessfulMutationToolCall(autoSaveRoundCalls),
            })
          }
        }
      }

      const appendAssistantContent = (content: string) => {
        if (!content) return
        liveAssistantContent.value = `${liveAssistantContent.value}${content}`
        scrollMessagesToBottom()
      }

      const appendAssistantReasoning = (delta: string) => {
        if (!delta) return
        liveAssistantReasoning.value = `${liveAssistantReasoning.value}${delta}`
        scrollMessagesToBottom()
      }

      buildVisibleAssistantContent = (source = rawAssistantContent) => stripProtocolContent(source)

      appendUnsavedDraftNotice = (control: AgentControlBlock | null) => {
        if (!wroteDocument || streamAborted || streamFailed) return
        if (!currentDocumentHasUnsavedChanges()) return
        if (control?.awaiting === 'user_confirm_write') return
        const notice = '内容已写入当前文档草稿，尚未保存。是否现在保存？'
        const merged = mergeVisibleAssistantContent(liveAssistantRoundSummary, notice)
        if (merged !== liveAssistantRoundSummary) {
          liveAssistantRoundSummary = merged
          runtimeSummaryAppendedThisRequest = true
        }
        syncLiveAssistantDisplay(buildVisibleAssistantContent(completedAssistantContent || rawAssistantContent))
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
          if (tail) {
            rawAssistantContent += tail
            handleAssistantChunk(tail)
          }
          return
        }
        rawAssistantContent = completedContent
        handleAssistantChunk(completedContent)
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

      const hasIncompleteActionWrite = () => false
      const completeWriterBlock = () => {}

      routeChunk = (rawChunk: string, force = false) => {
        consumeAssistantText(rawChunk, force)
      }

      handleAssistantChunk = (rawChunk: string) => {
        if (!rawChunk) return
        routeChunk(rawChunk)
      }

      const resetStreamingRoundState = () => {
        routeAction = null
        writerStarted = false
        lastWriterResult = null
        renderBuffer = ''
        inThinkBlock = false
        rawAssistantContent = ''
        completedAssistantContent = ''
        sawPlanSignalThisRound = false
        roundDocumentWriteObserved = false
        roundToolCalls = []
        liveAssistantRoundSummary = ''
        if (!chatOnlyMode) {
          liveAssistantContent.value = ''
          liveAssistantReasoning.value = ''
        }
      }

      const startContinuationAssistantMessage = (finalContent: string) => {
        finalizeAssistantMessageForDisplay(finalContent)

        if (chatOnlyMode) {
          commitCurrentRoundToChatContent()
          resetStreamingRoundState()
          scrollMessagesToBottom()
          return
        }

        const assistantMessageTime = Date.now()
        assistantMessage = {
          id: genId(),
          role: 'assistant',
          content: '',
          reasoning: '',
          createdAt: assistantMessageTime,
          updatedAt: assistantMessageTime,
        }
        assistantMessageTimestampSettled = false
        session.messages.push(assistantMessage)
        session.updatedAt = Date.now()
        sessions.value = [...sessions.value]
        streamingAssistantId.value = assistantMessage.id
        resetStreamingRoundState()
        scrollMessagesToBottom()
      }

      const startFollowupAssistantMessageForCompleted = (finalContent: string) => {
        finalizeAssistantMessageForDisplay(finalContent)

        if (chatOnlyMode) {
          commitCurrentRoundToChatContent()
          resetStreamingRoundState()
          scrollMessagesToBottom()
          return
        }

        const currentContent = (assistantMessage.content || liveAssistantContent.value || '').trim()
        const currentReasoning = (assistantMessage.reasoning || liveAssistantReasoning.value || '').trim()
        if (!currentContent && !currentReasoning) {
          return
        }

        const assistantMessageTime = Date.now()
        assistantMessage = {
          id: genId(),
          role: 'assistant',
          content: '',
          reasoning: '',
          createdAt: assistantMessageTime,
          updatedAt: assistantMessageTime,
        }
        assistantMessageTimestampSettled = false
        session.messages.push(assistantMessage)
        session.updatedAt = Date.now()
        sessions.value = [...sessions.value]
        streamingAssistantId.value = assistantMessage.id
        resetStreamingRoundState()
        scrollMessagesToBottom()
      }

      finalizePendingAssistantOutput = (options: { allowIncompleteAction?: boolean } = {}) => {
        routeChunk('', true)
        consumeAssistantText('', true)
        if (routeAction && routeAction !== 'chat') {
          completeWriterBlock()
        }
        const finalizedSource = completedAssistantContent || rawAssistantContent
        liveAssistantContent.value = buildVisibleAssistantContent(finalizedSource)
      }

      while (true) {
        requestRound += 1
        planStepAdvancedThisRound = false
        const progressSignatureBeforeRound = buildExecutionProgressSignature(executionState, finalControl)
        assistantMessage.content = liveAssistantContent.value
        assistantMessage.reasoning = liveAssistantReasoning.value
        const requestMessages = buildConversationMessages(session.messages, {
          preserveFullHistory: Boolean(
            requestRound > 1
            || session.pendingPlan
            || session.runtimePlan?.status === 'running'
            || session.runtimePlan?.status === 'pending',
          ),
        })
        if (executionState.semanticContinuation && executionState.lastInterceptGuidance?.trim()) {
          requestMessages.push({
            role: 'user',
            content: [
              '[内部执行修正]',
              '上一轮没有产生有效的文档写入；不要复述上一轮内容，也不要解释。',
              executionState.lastInterceptGuidance.trim(),
            ].join('\n'),
            attachments: [],
          })
        }
        const requestBody = buildRequestBody(requestMessages, provider, session.model.trim(), {
          previousResponseId,
          toolOutputs: pendingToolOutputs || (pendingPlan ? carriedPendingPlanToolOutputs : null),
        })
        const roundEvents: Array<Record<string, unknown>> = []
        const roundLabel = `session=${session.id} round=${requestRound}`
        const token = localStorage.getItem('token')
        logAgentDebugGroup(`${roundLabel} request`, {
          providerId: provider.id,
          providerName: provider.name,
          model: session.model,
          previousResponseId,
          executionSnapshotBeforeRound: buildDebugExecutionSnapshot(session, executionState, finalControl),
          requestBody,
        })
        const response = await fetch('/api/agent/chat/stream', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
          },
          signal: abortController.signal,
          body: JSON.stringify(requestBody),
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
          roundEvents.push(summarizeAgentEventForDebug(parsed.event, parsed.data))

          if (parsed.event === 'message.started') {
            markAssistantMessageStarted()
            currentAgentRunId = typeof parsed.data.run_id === 'string' && parsed.data.run_id.trim()
              ? parsed.data.run_id.trim()
              : currentAgentRunId
          } else if (parsed.event === 'message.delta') {
            markAssistantMessageStarted()
            cycleReceivedDelta = true
            const content = parsed.data.content || ''
            rawAssistantContent += content
            handleAssistantChunk(content)
            syncLiveAssistantDisplay(buildVisibleAssistantContent(rawAssistantContent, true))
            syncPendingPlanPreviewFromStream()
          } else if (parsed.event === 'tool.call.delta') {
            applyLiveHostToolDelta(parsed.data)
          } else if (parsed.event === 'structured_response') {
            const normalized = normalizeStructuredResponse(parsed.data)
            if (normalized) {
              latestStructuredResponse = normalized
              if (normalized.plan) {
                session.runtimePlan = normalized.plan
                const planText = runtimePlanToPlanText(normalized.plan)
                if (planText) {
                  session.lastPlan = planText
                }
                sawPlanSignalThisRound = true
              }
              syncPendingPlanPreviewFromStream()
            }
          } else if (parsed.event === 'task_analysis') {
            const normalized = normalizeTaskAnalysis(parsed.data)
            if (normalized) {
              session.taskAnalysis = normalized
              executionState.confirmationRequired = normalized.requiresUserConfirmation === true
              syncPendingPlanPreviewFromStream()
            }
          } else if (parsed.event === 'plan_event') {
            const normalized = normalizeRuntimePlan(parsed.data.plan)
            if (normalized) {
              session.runtimePlan = normalized
              sawPlanSignalThisRound = true
              syncPendingPlanPreviewFromStream()
            }
          } else if (parsed.event === 'tool_event') {
            const normalized = normalizeToolEvent(parsed.data)
            if (normalized) {
              session.toolEvents = [...session.toolEvents, normalized].slice(-20)
              // 同步挂到当前助手消息，前端按“本轮回复”粒度展示工具卡片。
              // 同一 call_id 的后续事件（requested -> completed）就地覆盖，避免重复卡片。
              const existingEvents = assistantMessage.toolEvents || []
              const sameCallIndex = normalized.callId
                ? existingEvents.findIndex((event) => event.callId === normalized.callId)
                : -1
              if (sameCallIndex >= 0) {
                const merged = [...existingEvents]
                merged[sameCallIndex] = { ...merged[sameCallIndex], ...normalized, id: merged[sameCallIndex].id }
                assistantMessage.toolEvents = merged
              } else {
                assistantMessage.toolEvents = [...existingEvents, normalized]
              }
              sessions.value = [...sessions.value]
              scrollMessagesToBottom()
              const roundCallSummary = buildRoundToolCallSummaryFromToolEvent(parsed.data)
              if (roundCallSummary) {
                roundToolCalls = [...roundToolCalls, roundCallSummary]
                executionState.recentToolCalls = appendRecentToolCalls(
                  executionState.recentToolCalls,
                  [roundCallSummary],
                )
                ensureVisibleAssistantRoundSummary(resolveEffectiveControl(completedAssistantContent || rawAssistantContent))
              }
            }
          } else if (parsed.event === 'tool.request') {
            const requestPromise = executeFrontendToolRequest(parsed.data).catch((error) => {
              console.error('frontend tool request failed', error)
            })
            pendingFrontendToolRequests.push(requestPromise)
          } else if (parsed.event === 'artifact_delta') {
            if (typeof parsed.data.delta === 'string' && parsed.data.delta) {
              upsertArtifactDraft(session, parsed.data.delta, {
                docId: props.docId,
                docName: props.docName,
              })
            }
          } else if (parsed.event === 'artifact_done') {
            upsertArtifactDraft(session, '', {
              finalize: true,
              docId: props.docId,
              docName: props.docName,
            })
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
            const usagePayload = parsed.data.usage
            if (usagePayload && typeof usagePayload === 'object') {
              turnUsage = usagePayload as Record<string, number>
            }
            const normalizedCompleted = stripProtocolContent(completedContent).trim()
            const shouldDeferCompletedIntoFinalBlock = Boolean(
              normalizedCompleted
              && runtimeSummaryAppendedThisRequest
              && (roundToolCalls.length || roundDocumentWriteObserved || executionState.documentWriteObserved)
            )
            if (!shouldDeferCompletedIntoFinalBlock) {
              appendCompletedTail(completedContent)
              const completedVisibleContent = buildVisibleAssistantContent(completedAssistantContent || rawAssistantContent)
              syncLiveAssistantDisplay(completedVisibleContent)
            } else {
              const currentVisibleContent = buildVisibleAssistantContent(rawAssistantContent)
              syncLiveAssistantDisplay(currentVisibleContent)
            }
            syncPendingPlanPreviewFromStream()
            streamDone = true
          } else if (parsed.event === 'agent.transport') {
            const mode = parsed.data.mode === 'chat_fallback'
              ? 'chat_fallback'
              : parsed.data.mode === 'chat'
                ? 'chat'
                : 'responses'
            agentTransportMode.value = mode
          } else if (parsed.event === 'agent.debug.model_request') {
            logAgentModelIo(
              `${roundLabel} model_request`,
              parsed.data && typeof parsed.data === 'object'
                ? parsed.data as Record<string, unknown>
                : { data: parsed.data },
            )
          } else if (parsed.event === 'agent.debug.model_response') {
            logAgentModelIo(
              `${roundLabel} model_response`,
              parsed.data && typeof parsed.data === 'object'
                ? parsed.data as Record<string, unknown>
                : { data: parsed.data },
            )
          } else if (parsed.event === 'agent.debug.model_error') {
            logAgentModelIo(
              `${roundLabel} model_error`,
              parsed.data && typeof parsed.data === 'object'
                ? parsed.data as Record<string, unknown>
                : { data: parsed.data },
            )
          } else if (parsed.event === 'tool.calls.required') {
            const completedContent = typeof parsed.data.content === 'string' ? parsed.data.content : ''
            if (completedContent) {
              completedAssistantContent = completedContent
              appendCompletedTail(completedContent)
            }
            finalizePendingAssistantOutput()
            syncPendingPlanPreviewFromStream()
            const confirmationPlan = (
              runtimePlanToPlanText(session.runtimePlan)
              || session.lastPlan?.trim()
              || session.pendingPlan?.trim()
              || ''
            ).trim()
            const confirmationControl = resolveEffectiveControl(completedAssistantContent || rawAssistantContent)
            const confirmationRequired = currentPlanNeedsConfirmation.value || shouldTreatPlanAsPending({
              planText: confirmationPlan,
              control: confirmationControl,
              session,
              executionState,
              sawPlanSignal: sawPlanSignalThisRound || Boolean(confirmationPlan),
            })
            if (confirmationRequired && confirmationPlan) {
              session.pendingPlan = confirmationPlan
              session.lastPlan = confirmationPlan
              session.updatedAt = Date.now()
              sessions.value = [...sessions.value]
            }
            toolResponseId = typeof parsed.data.response_id === 'string' ? parsed.data.response_id : ''
            if (toolResponseId.trim()) {
              previousResponseId = toolResponseId.trim()
            }
            const contentForToolCall = completedContent || rawAssistantContent
            requiredToolCalls = Array.isArray(parsed.data.calls) ? parsed.data.calls : []
            if (requiredToolCalls.length) {
              startContinuationAssistantMessage(contentForToolCall)
            }
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

        if (pendingFrontendToolRequests.length) {
          await Promise.allSettled(pendingFrontendToolRequests.splice(0))
        }

        if (!requiredToolCalls.length) {
          finalizePendingAssistantOutput({ allowIncompleteAction: true })
          const finalContent = completedAssistantContent || rawAssistantContent
          if (hasIncompleteActionWrite()) {
            executionState.lastInterceptCode = 'action_block_not_closed'
            executionState.lastInterceptMessage = '模型正文写入未完成'
            executionState.lastInterceptGuidance = 'Continue by using execute_browser_javascript with the markflow document writing method from the skill manual.'
            enqueueSemanticContinuation(finalContent)
            startContinuationAssistantMessage(finalContent)
            continue
          }
          const control = resolveEffectiveControl(finalContent)
          syncExecutionPlanProgress(control)
          const currentStepKey = currentPlanStepKey(executionState, control)
          const currentStepRequiresWrite = currentStepRequiresDocumentWrite(
            session.runtimePlan,
            executionState,
            control,
          )
          const awaitingPlanConfirmation = isAwaitingPlanConfirmation(executionState, control)
          const writeAlreadySatisfied = Boolean(currentStepKey && writeSatisfiedStepKey === currentStepKey)
          if (
            currentStepRequiresWrite
            && !awaitingPlanConfirmation
            && !writeAlreadySatisfied
            && !roundDocumentWriteObserved
            && !hasSuccessfulMutationToolCall(roundToolCalls)
          ) {
            throw new Error('当前步骤要求执行正文修改，但本轮没有完成有效的正文写入。请通过 execute_browser_javascript 调用 markflow.appendCurrentDocumentContent / markflow.replaceCurrentDocumentContent，或调用 rewriteDocumentSection / replaceDocumentBlock(s) / swapDocumentSections。')
          }
          if (!planStepAdvancedThisRound) {
            planStepAdvancedThisRound = advanceExecutionPlanStep(session.runtimePlan, executionState, control, {
              wroteDocument: roundDocumentWriteObserved || wroteDocument,
              savedDocument: saveToolSucceededThisTurn,
              mutationCompleted: hasSuccessfulMutationToolCall(roundToolCalls),
            })
          }
          const shouldContinue = shouldTriggerSemanticContinuation(finalContent, control)
          const progressSignatureAfterRound = buildExecutionProgressSignature(executionState, control)
          const roundMadeProgress = Boolean(
            planStepAdvancedThisRound
            || roundDocumentWriteObserved
            || saveToolSucceededThisTurn
            || progressSignatureAfterRound !== progressSignatureBeforeRound,
          )
          if (shouldContinue && !roundMadeProgress) {
            idleSemanticContinuationRounds += 1
          } else {
            idleSemanticContinuationRounds = 0
          }
          if (shouldContinue && idleSemanticContinuationRounds >= 3) {
            throw new Error('计划执行空转：连续多轮没有新的工具结果、正文写入或步骤推进。请重新规划当前步骤后再继续。')
          }
          logAgentDebugGroup(`${roundLabel} response`, {
            events: roundEvents,
            finalContent,
            finalControl: control,
            roundToolCalls,
            roundDocumentWriteObserved,
            saveToolAttemptedThisTurn,
            saveToolSucceededThisTurn,
            roundMadeProgress,
            idleSemanticContinuationRounds,
            progressSignatureBeforeRound,
            progressSignatureAfterRound,
            shouldContinue,
            breakReason: shouldContinue ? 'semantic_continuation' : 'model_finished_without_tool_calls',
            executionSnapshotAfterRound: buildDebugExecutionSnapshot(session, executionState, control),
          })
          if (shouldContinue) {
            enqueueSemanticContinuation(finalContent)
            startContinuationAssistantMessage(finalContent)
            continue
          }
          break
        }

        toolCallRounds += 1
        if (toolCallRounds > MAX_TOOL_CALL_ROUNDS) {
          throw new Error('工具调用轮次过多，已停止本次生成，请补充更明确的目标或范围')
        }

        const toolSignature = requiredToolCalls
          .map((call) => `${call.name}:${(call.arguments || '').trim()}`)
          .sort()
          .join('||')
        if (toolSignature) {
          const hitCount = (toolCallSignatureHits.get(toolSignature) || 0) + 1
          toolCallSignatureHits.set(toolSignature, hitCount)
          if (!cycleReceivedDelta && hitCount > MAX_REPEAT_TOOL_SIGNATURE_HITS) {
            throw new Error('检测到重复工具调用循环，已停止本次生成，请调整指令后重试')
          }
        }

        if (!pendingToolOutputs) {
          pendingToolOutputs = await executeRoundToolCalls(requiredToolCalls)
        }
        logAgentDebugGroup(`${roundLabel} tool_calls`, {
          requiredToolCalls,
          toolOutputs: pendingToolOutputs,
          executionSnapshotAfterTools: buildDebugExecutionSnapshot(session, executionState, finalControl),
        })
        const toolDrivenControl: AgentControlBlock | null = latestToolDrivenControl as AgentControlBlock | null
        const structuredResponseForHostBatch: AgentStructuredResponse | null = latestStructuredResponse as AgentStructuredResponse | null
        const hostOnlyBatch = requiredToolCalls.length > 0 && requiredToolCalls.every((call) => isHostStateTool(call.name))
        const awaitingConfirmationAfterHostSync = hostOnlyBatch
          && toolDrivenControl?.currentMode === 'plan'
          && toolDrivenControl?.awaiting === 'user_input'
          && Boolean(session.pendingPlan?.trim() || session.runtimePlan?.steps?.length)
        if (awaitingConfirmationAfterHostSync) {
          liveAssistantContent.value = structuredResponseForHostBatch?.message?.trim()
            || buildPendingPlanConfirmationMessage()
          pendingToolOutputs = null
          break
        }
        if (pendingToolOutputs.length) {
          resetSemanticContinuationBudget()
          collectedPlanToolOutputs = mergePendingPlanToolOutputs(collectedPlanToolOutputs, pendingToolOutputs)
        }
        roundToolCalls = summarizeToolCallBatch(requiredToolCalls, pendingToolOutputs)
        const saveCallOutcomes = roundToolCalls.filter((call) => call.name === 'save_current_document')
        if (saveCallOutcomes.length) {
          saveToolAttemptedThisTurn = true
          if (saveCallOutcomes.some((call) => call.outcome === 'success' || call.outcome === 'noop')) {
            saveToolSucceededThisTurn = true
            executionState.saveRequested = false
          }
        }
        executionState.recentToolCalls = appendRecentToolCalls(
          executionState.recentToolCalls,
          roundToolCalls,
        )
        if (roundToolCalls.some((call) => isDocumentMutationTool(call.name) && call.outcome === 'success')) {
          executionState.documentWriteObserved = true
          executionState.saveAttemptWithoutDocumentChange = false
          executionState.writeCompleted = true
          writeSatisfiedStepKey = currentPlanStepKey(executionState) || writeSatisfiedStepKey
          roundDocumentWriteObserved = true
        }
        appendToolEventsToSession(session, roundToolCalls)
        const saveNoopDetected = !wroteDocument && extractSaveNoopState(pendingToolOutputs)
        if (saveNoopDetected) {
          executionState.saveAttemptWithoutDocumentChange = true
        }
        const controlAfterToolRound = resolveEffectiveControl(completedAssistantContent || rawAssistantContent)
        await maybeAutoSaveAfterDocumentWrite(controlAfterToolRound)
        ensureVisibleAssistantRoundSummary(resolveEffectiveControl(completedAssistantContent || rawAssistantContent))
        if (
          pendingPlan
          && !executionState.documentWriteObserved
          && isReadOrSaveOnlyBatch(roundToolCalls)
        ) {
          nonWritingPlanRounds += 1
        } else {
          nonWritingPlanRounds = 0
        }
        if (saveNoopDetected && nonWritingPlanRounds >= 3) {
          throw new Error('计划执行停滞：连续多轮只读取或保存当前文档，但没有真正写入正文。请重新规划当前步骤后再继续执行。')
        }
        if (runtimeSummaryAppendedThisRequest || liveAssistantRoundSummary.trim()) {
          startFollowupAssistantMessageForCompleted(completedAssistantContent || rawAssistantContent)
        }
      }
    } catch (error: any) {
      if (error?.name === 'AbortError') {
        streamAborted = true
        ElMessage.info('已停止生成')
        logAgentDebugGroup(`session=${session.id} aborted`, {
          previousResponseId,
          liveAssistantContent: liveAssistantContent.value,
          liveAssistantReasoning: liveAssistantReasoning.value,
        })
      } else {
        streamFailed = true
        const message = error?.message || '智能体请求失败'
        const intercepted = isAgentInternalInterceptError(message)
        interceptedFailure = intercepted ? resolveAgentInterceptDetails(message) : null
        if (interceptedFailure) {
          executionState.lastInterceptCode = interceptedFailure.code
          executionState.lastInterceptMessage = message
          executionState.lastInterceptGuidance = interceptedFailure.modelGuidance
        }
        logAgentPanelError('send_message', error, {
          sessionId: session.id,
          providerId: provider.id,
          model: session.model,
          docId: props.docId,
          docName: props.docName,
          intercepted,
        })
        logAgentDebugGroup(`session=${session.id} error`, {
          error,
          intercepted,
          previousResponseId,
          liveAssistantContent: liveAssistantContent.value,
          liveAssistantReasoning: liveAssistantReasoning.value,
          executionState,
          runtimePlan: session.runtimePlan,
          executionSnapshotAtError: buildDebugExecutionSnapshot(session, executionState, finalControl),
        })
        if (!intercepted) {
          ElMessage.error(message)
        }
      }
    } finally {
      window.removeEventListener(AGENT_WRITER_RESULT_EVENT, handleWriterResult as EventListener)
      activeStreamController = null
      if (!streamFailed) {
        try {
          finalizePendingAssistantOutput()
        } catch (finalizeError) {
          console.error('agent stream finalize failed', finalizeError)
        }
      }
      const finalAssistantContent = completedAssistantContent || rawAssistantContent || assistantMessage.content || liveAssistantContent.value
      const structuredResponse: AgentStructuredResponse | null = latestStructuredResponse as AgentStructuredResponse | null
      const effectiveStructuredPlan = structuredResponse?.plan || latestToolDrivenPlan
      const structuredPlanText = effectiveStructuredPlan
        ? runtimePlanToPlanText(effectiveStructuredPlan)
        : ''
      const extractedPlan = structuredPlanText || extractPlanBlock(finalAssistantContent)
      if (extractedPlan) {
        session.lastPlan = extractedPlan
        session.runtimePlan = effectiveStructuredPlan || buildRuntimePlanFromText(
          extractedPlan,
          userMessage?.content || pendingPlan || '',
          session.runtimePlan,
        ) || session.runtimePlan
      } else if (pendingPlan && !session.lastPlan) {
        session.lastPlan = pendingPlan
      }
      finalControl = structuredResponse?.state
        || latestToolDrivenControl
        || (
          (
            hostStateUpdatedThisRound
            || session.pendingPlan?.trim()
            || session.runtimePlan?.steps?.length
            || executionState.confirmationRequired
            || executionState.pendingPlan
            || executionState.planCurrentStep
            || executionState.planStepIndex
            || executionState.planCompletedSteps.length
            || executionState.saveRequested
            || executionState.writeCompleted
            || executionState.documentWriteObserved
            || roundToolCalls.length
            || roundDocumentWriteObserved
            || wroteDocument
          )
            ? buildControlFromExecutionState(executionState, session.taskAnalysis)
            : null
        )
      syncExecutionPlanProgress(finalControl)
      syncRuntimePlanStatus(session, finalControl, executionState)
      if (!streamFailed && !streamAborted) {
        if (
          executionState.semanticContinuation
          || finalControl?.awaiting === 'user_confirm_write'
          || finalControl?.currentMode === 'normal'
        ) {
          executionState.confirmationRequired = false
        }
        executionState.lastInterceptCode = null
        executionState.lastInterceptMessage = null
        executionState.lastInterceptGuidance = null
      }
      const finalStepRequiresSave = currentStepRequiresDocumentSave(session.runtimePlan, executionState, finalControl)
      const shouldAutoSaveAfterWrite = (
        !streamFailed
        && !streamAborted
        && props.docType === 'doc'
        && props.docId
        && finalStepRequiresSave
        && (wroteDocument || executionState.writeCompleted || roundDocumentWriteObserved)
        && currentDocumentHasUnsavedChanges()
        && !saveToolSucceededThisTurn
      )
      if (shouldAutoSaveAfterWrite) {
        const autoSaveOutputs = await executeAgentToolCalls([{
          call_id: genId(),
          name: 'save_current_document',
          arguments: JSON.stringify({ doc_id: props.docId }),
        }])
        const autoSaveOutput = autoSaveOutputs[0]
        const autoSaveResult = autoSaveOutput?.output && typeof autoSaveOutput.output === 'object'
          ? autoSaveOutput.output as Record<string, any>
          : null
        const autoSavePayload = autoSaveResult?.result && typeof autoSaveResult.result === 'object'
          ? autoSaveResult.result as Record<string, any>
          : null
        saveToolAttemptedThisTurn = true
        if (autoSaveResult?.ok === true && (autoSavePayload?.saved === true || autoSavePayload?.already_saved === true)) {
          saveToolSucceededThisTurn = true
          if (!planStepAdvancedThisRound) {
            planStepAdvancedThisRound = advanceExecutionPlanStep(session.runtimePlan, executionState, finalControl, {
              wroteDocument: roundDocumentWriteObserved || wroteDocument,
              savedDocument: true,
              mutationCompleted: true,
            })
          }
        } else if (currentDocumentHasUnsavedChanges()) {
          streamFailed = true
          const errorMessage = typeof autoSaveResult?.error === 'string' && autoSaveResult.error.trim()
            ? autoSaveResult.error.trim()
            : '模型已完成正文修改，但未成功保存当前文档。'
          ElMessage.error(errorMessage)
        }
      }
      if (!streamFailed) {
        appendUnsavedDraftNotice(finalControl)
      }
      const runtimePlanText = runtimePlanToPlanText(session.runtimePlan)
      const finalPendingPlanCandidate = (
        extractedPlan
        || runtimePlanText
        || session.lastPlan?.trim()
        || pendingPlan
      ).trim()
      const runtimePlanStillPending = session.runtimePlan?.status === 'pending'
      const finalHasPlanSignal = Boolean(extractedPlan || sawPlanSignalThisRound)
      const finalPhase = typeof finalControl?.phase === 'string' ? finalControl.phase : null
      const finalStillNeedsConfirmation = finalPhase !== 'completed'
        && shouldTreatPlanAsPending({
          planText: finalPendingPlanCandidate,
          control: finalControl,
          session,
          executionState,
          sawPlanSignal: finalHasPlanSignal || runtimePlanStillPending,
        })
      if (finalStillNeedsConfirmation) {
        session.pendingPlan = finalPendingPlanCandidate
        session.pendingPlanToolOutputs = [...collectedPlanToolOutputs]
      } else if (session.pendingPlan) {
        session.pendingPlan = null
        session.pendingPlanToolOutputs = []
      } else if (session.pendingPlanToolOutputs.length) {
        session.pendingPlanToolOutputs = []
      }
      finalizeAssistantMessageForDisplay(finalAssistantContent, { includeStructuredMessage: false })
      const finalLoopResultContent = structuredResponse?.message?.trim()
        || stripProtocolContent(finalAssistantContent).trim()
      appendFinalLoopResultMessage(finalLoopResultContent)
      if ((!streamFailed || Boolean(interceptedFailure)) && !streamAborted) {
        const memoryPlan = session.lastPlan || null
        const memorySummary = compactMessageText(assistantMessage.content || finalAssistantContent, 400) || null
        const memoryHasSignals = Boolean(
          memorySummary
          || executionState.recentToolCalls.length
          || executionState.documentWriteObserved
          || executionState.lastInterceptCode,
        )
        if (memoryHasSignals) {
          session.lastExecutionMemory = {
            currentMode: executionState.currentMode,
            awaiting: executionState.awaiting,
            currentActionKind: executionState.currentActionKind,
            currentActionStatus: executionState.currentActionStatus,
            currentActionMode: executionState.currentActionMode,
            currentActionTarget: executionState.currentActionTarget,
            confirmationRequired: executionState.confirmationRequired,
            plan: memoryPlan,
            assistantSummary: memorySummary,
            controlPhase: typeof finalControl?.phase === 'string' && finalControl.phase.trim() ? finalControl.phase.trim() : null,
            taskKind: executionState.taskKind,
            editIntent: executionState.editIntent,
            editStage: executionState.editStage,
            saveRequested: executionState.saveRequested,
            writeCompleted: executionState.writeCompleted,
            planStepIndex: executionState.planStepIndex,
            planTotalSteps: executionState.planTotalSteps,
            planCurrentStep: executionState.planCurrentStep,
            planCompletedSteps: [...executionState.planCompletedSteps],
            documentWriteObserved: executionState.documentWriteObserved,
            saveAttemptWithoutDocumentChange: executionState.saveAttemptWithoutDocumentChange,
            lastInterceptCode: executionState.lastInterceptCode,
            lastInterceptMessage: executionState.lastInterceptMessage,
            lastInterceptGuidance: executionState.lastInterceptGuidance,
            recentToolCalls: [...executionState.recentToolCalls],
          }
        }
        session.sessionMemory = buildSessionMemory(session)
      }
      // 把整个 turn 聚合的 token 消耗挂到最终 assistant 消息上，供卡片展示。
      if (turnUsage) {
        assistantMessage.usage = turnUsage
      }
      if (!assistantMessage.content.trim() && !assistantMessage.reasoning?.trim() && !assistantMessage.toolEvents?.length) {
        session.messages = session.messages.filter((message) => message.id !== assistantMessage.id)
      }
      session.previousResponseId = previousResponseId
      session.lastSyncedMessageCount = 0
      session.updatedAt = Date.now()
      logAgentDebugGroup(`session=${session.id} final`, {
        finalAssistantContent,
        finalControl,
        previousResponseId,
        executionState,
        runtimePlan: session.runtimePlan,
        pendingPlan: session.pendingPlan,
        lastExecutionMemory: session.lastExecutionMemory,
        saveToolAttemptedThisTurn,
        saveToolSucceededThisTurn,
        streamFailed,
        streamAborted,
        executionSnapshotFinal: buildDebugExecutionSnapshot(session, executionState, finalControl),
      })
      streaming.value = false
      streamingAssistantId.value = ''
      liveAssistantContent.value = ''
      liveAssistantReasoning.value = ''
      sessions.value = [...sessions.value]
      persistSessions()
    }
  }
  return {
    stopStreaming,
    sendMessage,
  }
}
