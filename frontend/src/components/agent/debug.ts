// AgentPanel 调试日志与快照构建工具。
//
// 纯函数，不依赖组件状态；只吃参数、写 console 或返回可序列化快照。
// `summarizeAgentEventForDebug` 因依赖 compactMessageText（运行时文本压缩）
// 仍留在组件内。

import type {
  AgentControlBlock,
} from '@/agent/protocol'
import type {
  AgentExecutionState,
  AgentRuntimePlan,
  AgentSession,
} from './types'

const AGENT_DEBUG_STORAGE_KEY = 'markflow:agent-debug'

export function logAgentPanelError(scope: string, error: unknown, extra?: Record<string, unknown>) {
  console.error(`agent panel error: ${scope}`, {
    error,
    ...extra,
  })
}

export function agentDebugEnabled() {
  try {
    return localStorage.getItem(AGENT_DEBUG_STORAGE_KEY) !== '0'
  } catch {
    return true
  }
}

export function logAgentDebugGroup(title: string, payload: Record<string, unknown>) {
  if (!agentDebugEnabled()) return
  console.groupCollapsed(`[agent-debug] ${title}`)
  for (const [key, value] of Object.entries(payload)) {
    console.debug(key, value)
  }
  console.groupEnd()
}

export function logAgentModelIo(title: string, payload: Record<string, unknown>) {
  if (!agentDebugEnabled()) return
  console.group(`[agent-debug] ${title}`)
  const preferredOrder = [
    'provider',
    'model',
    'transport_mode',
    'response_id',
    'error',
    'system_prompt',
    'preamble',
    'history',
    'prompt',
    'tools',
    'additional_params',
    'text',
    'partial_text',
    'tool_calls',
  ]
  const seen = new Set<string>()
  for (const key of preferredOrder) {
    if (key in payload) {
      console.log(key, payload[key])
      seen.add(key)
    }
  }
  for (const [key, value] of Object.entries(payload)) {
    if (seen.has(key)) continue
    console.log(key, value)
  }
  console.groupEnd()
}

export function buildDebugRuntimePlanSnapshot(runtimePlan: AgentRuntimePlan | null) {
  if (!runtimePlan) return null
  return {
    id: runtimePlan.id,
    status: runtimePlan.status,
    goal: runtimePlan.goal,
    steps: runtimePlan.steps.map((step, index) => ({
      index: index + 1,
      id: step.id,
      title: step.title,
      status: step.status,
      requiresConfirmation: step.requiresConfirmation,
      requiresDocumentWrite: step.requiresDocumentWrite,
      requiresDocumentSave: step.requiresDocumentSave,
      toolHints: step.toolHints,
    })),
  }
}

export function buildDebugExecutionSnapshot(
  session: AgentSession,
  executionState: AgentExecutionState,
  control: AgentControlBlock | null = null,
) {
  return {
    confirmationRequired: executionState.confirmationRequired,
    pendingPlan: session.pendingPlan,
    pendingPlanToolOutputs: session.pendingPlanToolOutputs.map((output) => ({
      call_id: output.call_id,
      name: output.name || null,
      arguments: output.arguments || null,
      output: output.output,
    })),
    lastPlan: session.lastPlan,
    currentPlanNeedsConfirmation: Boolean(session.pendingPlan?.trim()),
    semanticContinuation: executionState.semanticContinuation,
    semanticContinuationRound: executionState.semanticContinuationRound,
    planStepIndex: executionState.planStepIndex,
    planTotalSteps: executionState.planTotalSteps,
    planCurrentStep: executionState.planCurrentStep,
    planCompletedSteps: [...executionState.planCompletedSteps],
    writeCompleted: executionState.writeCompleted,
    saveRequested: executionState.saveRequested,
    documentWriteObserved: executionState.documentWriteObserved,
    saveAttemptWithoutDocumentChange: executionState.saveAttemptWithoutDocumentChange,
    lastInterceptCode: executionState.lastInterceptCode,
    lastInterceptMessage: executionState.lastInterceptMessage,
    control: control ? {
      phase: control.phase || null,
      pendingPlan: control.pendingPlan === true,
      autoContinue: control.autoContinue === true,
      needsSave: control.needsSave === true,
      writeScope: control.writeScope || null,
      preferredWriteAction: control.preferredWriteAction || null,
      taskKind: control.taskKind || null,
      editIntent: control.editIntent || null,
      editStage: control.editStage || null,
      saveRequested: control.saveRequested === true,
      writeCompleted: control.writeCompleted === true,
      planStepIndex: control.planStepIndex ?? null,
      planTotalSteps: control.planTotalSteps ?? null,
      planCurrentStep: control.planCurrentStep || null,
      planCompletedSteps: control.planCompletedSteps || [],
    } : null,
    runtimePlan: buildDebugRuntimePlanSnapshot(session.runtimePlan),
    lastExecutionMemory: session.lastExecutionMemory ? {
      confirmationRequired: session.lastExecutionMemory.confirmationRequired,
      plan: session.lastExecutionMemory.plan,
      assistantSummary: session.lastExecutionMemory.assistantSummary,
      controlPhase: session.lastExecutionMemory.controlPhase,
      planStepIndex: session.lastExecutionMemory.planStepIndex,
      planTotalSteps: session.lastExecutionMemory.planTotalSteps,
      planCurrentStep: session.lastExecutionMemory.planCurrentStep,
      planCompletedSteps: [...session.lastExecutionMemory.planCompletedSteps],
      writeCompleted: session.lastExecutionMemory.writeCompleted,
      saveRequested: session.lastExecutionMemory.saveRequested,
      documentWriteObserved: session.lastExecutionMemory.documentWriteObserved,
      saveAttemptWithoutDocumentChange: session.lastExecutionMemory.saveAttemptWithoutDocumentChange,
      lastInterceptCode: session.lastExecutionMemory.lastInterceptCode,
    } : null,
  }
}
