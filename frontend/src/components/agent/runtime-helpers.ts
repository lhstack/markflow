// AgentPanel 会话运行时的纯 helper 函数。
//
// 从 AgentPanel.vue 抽出，全部为纯函数（不依赖组件响应式状态 / props / ElMessage）：
// SSE / 计划 / 工具调用 / 执行状态 / 会话记忆的解析、归一化与摘要构建。
// 依赖仅限类型、agent 运行常量与 genId。

import {
  AGENT_TASK_ANALYSIS_COMPLEXITIES,
  AGENT_TASK_ANALYSIS_INTENTS,
  AGENT_TASK_ANALYSIS_MODES,
  AGENT_TASK_ANALYSIS_WRITE_SCOPES,
  toolHasOnlyCapabilities,
} from '@/agent/protocol'
import type { AgentControlBlock } from '@/agent/protocol'
import type { AgentToolCall, AgentToolOutputPayload } from '@/utils/agentTools'
import { genId } from '@/components/agent/id'
import { normalizeAttachment } from '@/composables/useComposerAttachments'
import type { AgentAttachment } from '@/components/agent/types'
import type {
  AgentArtifact,
  AgentExecutionMemory,
  AgentExecutionState,
  AgentExecutionToolCallSummary,
  AgentMessage,
  AgentRequestMessage,
  AgentRuntimePlan,
  AgentRuntimePlanStep,
  AgentSession,
  AgentSessionMemory,
  AgentStructuredResponse,
  AgentTaskAnalysis,
  AgentToolEvent,
} from '@/components/agent/types'

// SSE / 计划 / 摘要相关常量。
export const REQUEST_SUMMARY_MAX_ITEMS = 6
export const REQUEST_SUMMARY_ITEM_CHARS = 240


function normalizeTimestamp(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string' && value.trim()) {
    const parsedNumber = Number(value)
    if (Number.isFinite(parsedNumber)) return parsedNumber
    const parsedDate = new Date(value).getTime()
    if (Number.isFinite(parsedDate)) return parsedDate
  }
  return null
}

export function normalizeMessage(raw: any): AgentMessage | null {
  if (!raw || typeof raw !== 'object') return null
  const role = raw.role === 'assistant'
    ? 'assistant'
    : raw.role === 'user'
      ? 'user'
      : raw.role === 'system'
        ? 'system'
        : null
  if (!role) return null

  const createdAt = normalizeTimestamp(raw.createdAt ?? raw.created_at) ?? Date.now()
  const updatedAt = normalizeTimestamp(raw.updatedAt ?? raw.updated_at) ?? createdAt

  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : genId(),
    role,
    content: typeof raw.content === 'string' ? raw.content : '',
    createdAt,
    updatedAt,
    reasoning: typeof raw.reasoning === 'string' ? raw.reasoning : '',
    internalStatus: raw.internalStatus === true || raw.internal_status === true,
    attachments: Array.isArray(raw.attachments)
      ? raw.attachments
        .map((attachment: any) => normalizeAttachment(attachment))
        .filter((attachment: AgentAttachment | null): attachment is AgentAttachment => Boolean(attachment))
      : [],
    toolEvents: Array.isArray(raw.toolEvents ?? raw.tool_events)
      ? (raw.toolEvents ?? raw.tool_events)
        .map((event: any) => normalizeToolEvent(event))
        .filter((event: AgentToolEvent | null): event is AgentToolEvent => Boolean(event))
      : [],
  }
}

export function normalizeTaskAnalysis(raw: any): AgentTaskAnalysis | null {
  if (!raw || typeof raw !== 'object') return null
  const intent = typeof raw.intent === 'string' && raw.intent.trim()
    ? raw.intent.trim()
    : AGENT_TASK_ANALYSIS_INTENTS[0] || ''
  const complexity = typeof raw.complexity === 'string' && AGENT_TASK_ANALYSIS_COMPLEXITIES.includes(raw.complexity.trim() as any)
    ? raw.complexity.trim()
    : AGENT_TASK_ANALYSIS_COMPLEXITIES[0] || ''
  const mode = typeof raw.mode === 'string' && AGENT_TASK_ANALYSIS_MODES.includes(raw.mode.trim() as any)
    ? raw.mode.trim()
    : AGENT_TASK_ANALYSIS_MODES[0] || 'chat'
  const writeScope = typeof raw.writeScope === 'string' && AGENT_TASK_ANALYSIS_WRITE_SCOPES.includes(raw.writeScope.trim() as any)
    ? raw.writeScope.trim()
    : typeof raw.write_scope === 'string' && AGENT_TASK_ANALYSIS_WRITE_SCOPES.includes(raw.write_scope.trim() as any)
      ? raw.write_scope.trim()
      : null
  return {
    intent,
    complexity,
    mode,
    requiresTools: raw.requiresTools === true || raw.requires_tools === true,
    requiresUserConfirmation: raw.requiresUserConfirmation === true || raw.requires_user_confirmation === true,
    writeScope,
    preferredWriteAction: typeof raw.preferredWriteAction === 'string' && raw.preferredWriteAction.trim()
      ? raw.preferredWriteAction.trim()
      : typeof raw.preferred_write_action === 'string' && raw.preferred_write_action.trim()
        ? raw.preferred_write_action.trim()
        : null,
    deliverable: typeof raw.deliverable === 'string' && raw.deliverable.trim() ? raw.deliverable.trim() : null,
  }
}

export function normalizeSessionMemory(raw: any): AgentSessionMemory | null {
  if (!raw || typeof raw !== 'object') return null
  const normalizeItems = (value: unknown) => Array.isArray(value)
    ? value
      .filter((item: unknown): item is string => typeof item === 'string' && Boolean(item.trim()))
      .map((item: string) => item.trim())
    : []
  const summary = typeof raw.summary === 'string' && raw.summary.trim() ? raw.summary.trim() : null
  const activeUserGoals = normalizeItems(raw.activeUserGoals ?? raw.active_user_goals)
  const completedFacts = normalizeItems(raw.completedFacts ?? raw.completed_facts)
  const openLoops = normalizeItems(raw.openLoops ?? raw.open_loops)
  if (!summary && !activeUserGoals.length && !completedFacts.length && !openLoops.length) return null
  return {
    summary,
    activeUserGoals,
    completedFacts,
    openLoops,
    updatedAt: typeof raw.updatedAt === 'string'
      ? raw.updatedAt
      : typeof raw.updated_at === 'string'
        ? raw.updated_at
        : null,
  }
}

export function normalizeRuntimePlanStep(raw: any, fallbackIndex = 0): AgentRuntimePlanStep | null {
  if (!raw || typeof raw !== 'object') return null
  const title = typeof raw.title === 'string' && raw.title.trim() ? raw.title.trim() : ''
  if (!title) return null
  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : `step_${fallbackIndex + 1}`,
    title,
    kind: typeof raw.kind === 'string' && raw.kind.trim() ? raw.kind.trim() : 'edit',
    description: typeof raw.description === 'string' ? raw.description.trim() : title,
    status: typeof raw.status === 'string' && raw.status.trim() ? raw.status.trim() : 'pending',
    toolHints: Array.isArray(raw.toolHints ?? raw.tool_hints)
      ? (raw.toolHints ?? raw.tool_hints)
        .filter((item: unknown): item is string => typeof item === 'string' && Boolean(item.trim()))
        .map((item: string) => item.trim())
      : [],
    requiresConfirmation: raw.requiresConfirmation === true || raw.requires_confirmation === true,
    requiresDocumentWrite: raw.requiresDocumentWrite === true || raw.requires_document_write === true,
    requiresDocumentSave: raw.requiresDocumentSave === true || raw.requires_document_save === true,
  }
}

export function normalizeRuntimePlan(raw: any): AgentRuntimePlan | null {
  if (!raw || typeof raw !== 'object') return null
  const goal = typeof raw.goal === 'string' && raw.goal.trim() ? raw.goal.trim() : ''
  const steps = Array.isArray(raw.steps)
    ? raw.steps
      .map((step: any, index: number) => normalizeRuntimePlanStep(step, index))
      .filter((step: AgentRuntimePlanStep | null): step is AgentRuntimePlanStep => Boolean(step))
    : []
  if (!goal && !steps.length) return null
  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : genId(),
    goal: goal || '执行计划',
    summary: typeof raw.summary === 'string' && raw.summary.trim() ? raw.summary.trim() : null,
    status: typeof raw.status === 'string' && raw.status.trim() ? raw.status.trim() : 'pending',
    steps,
    createdAt: typeof raw.createdAt === 'string' ? raw.createdAt : typeof raw.created_at === 'string' ? raw.created_at : null,
    updatedAt: typeof raw.updatedAt === 'string' ? raw.updatedAt : typeof raw.updated_at === 'string' ? raw.updated_at : null,
  }
}

export function normalizeStructuredResponse(raw: any): AgentStructuredResponse | null {
  if (!raw || typeof raw !== 'object') return null
  const message = typeof raw.message === 'string' ? raw.message : ''
  const state = raw.state && typeof raw.state === 'object'
    ? normalizeControlPayload(raw.state)
    : null
  const plan = normalizeRuntimePlan(raw.plan)
  if (!message.trim() && !state && !plan) return null
  return {
    message,
    state,
    plan,
  }
}

export function normalizeArtifact(raw: any): AgentArtifact | null {
  if (!raw || typeof raw !== 'object') return null
  const title = typeof raw.title === 'string' && raw.title.trim() ? raw.title.trim() : ''
  if (!title) return null
  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : genId(),
    type: typeof raw.type === 'string' && raw.type.trim() ? raw.type.trim() : 'markdown_doc',
    title,
    status: typeof raw.status === 'string' && raw.status.trim() ? raw.status.trim() : 'drafting',
    content: typeof raw.content === 'string' ? raw.content : '',
    relatedDocId: Number.isFinite(raw.relatedDocId) ? Number(raw.relatedDocId) : Number.isFinite(raw.related_doc_id) ? Number(raw.related_doc_id) : null,
  }
}

export function normalizeToolEvent(raw: any): AgentToolEvent | null {
  if (!raw || typeof raw !== 'object') return null
  const tool = typeof raw.tool === 'string' && raw.tool.trim() ? raw.tool.trim() : ''
  const summary = typeof raw.summary === 'string' && raw.summary.trim() ? raw.summary.trim() : ''
  if (!tool && !summary) return null
  const argumentsText = typeof raw.arguments === 'string'
    ? raw.arguments
    : raw.arguments && typeof raw.arguments === 'object'
      ? JSON.stringify(raw.arguments, null, 2)
      : ''
  const outputText = typeof raw.output === 'string'
    ? raw.output
    : raw.output && typeof raw.output === 'object'
      ? JSON.stringify(raw.output, null, 2)
      : ''
  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : genId(),
    tool: tool || 'tool',
    status: typeof raw.status === 'string' && raw.status.trim() ? raw.status.trim() : 'completed',
    summary: summary || `已执行工具 ${tool}`,
    source: typeof raw.source === 'string' && raw.source.trim() ? raw.source.trim() : undefined,
    callId: typeof raw.call_id === 'string' && raw.call_id.trim() ? raw.call_id.trim() : undefined,
    arguments: argumentsText || undefined,
    output: outputText || undefined,
  }
}

export function normalizePlanStepTitle(value: string) {
  return value.trim()
}

export function resolvePlanStepIndicesByTitles(
  steps: AgentRuntimePlanStep[],
  titles: string[],
  options: { skipIndices?: Set<number> } = {},
) {
  const skipIndices = options.skipIndices || new Set<number>()
  const usedIndices = new Set<number>()
  const resolved: number[] = []

  for (const title of titles.map(normalizePlanStepTitle).filter(Boolean)) {
    const index = steps.findIndex((step, stepIndex) => (
      !skipIndices.has(stepIndex)
      && !usedIndices.has(stepIndex)
      && normalizePlanStepTitle(step.title) === title
    ))
    if (index === -1) continue
    usedIndices.add(index)
    resolved.push(index)
  }

  return resolved
}

export function resolveCurrentPlanStepIndexByTitle(
  steps: AgentRuntimePlanStep[],
  title: string,
  completedIndices: Set<number>,
) {
  const normalized = normalizePlanStepTitle(title)
  if (!normalized) return -1
  const nextIncompleteMatch = steps.findIndex((step, index) => (
    !completedIndices.has(index) && normalizePlanStepTitle(step.title) === normalized
  ))
  if (nextIncompleteMatch >= 0) return nextIncompleteMatch
  return steps.findIndex((step) => normalizePlanStepTitle(step.title) === normalized)
}

export function buildRuntimePlanFromText(planText: string, goal: string, templatePlan: AgentRuntimePlan | null = null) {
  const parsedTitles = parsePlanSteps(planText)
  const reusesTemplateShape = Boolean(
    templatePlan?.steps?.length === parsedTitles.length
    && parsedTitles.every((title, index) => normalizePlanStepTitle(templatePlan?.steps?.[index]?.title || '') === normalizePlanStepTitle(title)),
  )
  const steps = parsedTitles.map((title, index) => {
    const templateStep = templatePlan?.steps?.[index] || null
    const isSameStep = normalizePlanStepTitle(templateStep?.title || '') === normalizePlanStepTitle(title)
    return {
      id: isSameStep ? (templateStep?.id || `step_${index + 1}`) : `step_${index + 1}`,
      title,
      kind: isSameStep ? (templateStep?.kind || 'edit') : 'edit',
      description: title,
      status: isSameStep ? (templateStep?.status || 'pending') : 'pending',
      toolHints: isSameStep ? (templateStep?.toolHints || []) : [],
      requiresConfirmation: isSameStep && templateStep?.requiresConfirmation === true,
      requiresDocumentWrite: isSameStep && templateStep?.requiresDocumentWrite === true,
      requiresDocumentSave: isSameStep && templateStep?.requiresDocumentSave === true,
    }
  })

  if (!steps.length) return null
  const now = new Date().toISOString()
  return {
    id: reusesTemplateShape ? (templatePlan?.id || genId()) : genId(),
    goal: goal.trim() || '执行计划',
    summary: '由模型输出的正式执行计划',
    status: reusesTemplateShape ? (templatePlan?.status || 'pending') : 'pending',
    steps,
    createdAt: reusesTemplateShape ? (templatePlan?.createdAt || now) : now,
    updatedAt: now,
  } satisfies AgentRuntimePlan
}

export function runtimePlanToPlanText(runtimePlan: AgentRuntimePlan | null) {
  if (!runtimePlan?.steps?.length) return ''
  return runtimePlan.steps
    .map((step, index) => `${index + 1}. ${step.title.trim()}`)
    .join('\n')
    .trim()
}

export function stripProtocolContent(content: string) {
  if (!content) return ''
  return content
    .replace(/[ \t]+\n/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

export function summarizeAgentEventForDebug(event: string, data: any) {
  if (event === 'agent.debug.model_request') {
    return {
      event,
      model: data?.model || null,
      transportMode: data?.transport_mode || null,
      protocol: data?.provider?.protocol || null,
      historyCount: Array.isArray(data?.history) ? data.history.length : 0,
      toolCount: Array.isArray(data?.tools) ? data.tools.length : 0,
      preamblePreview: compactMessageText(typeof data?.preamble === 'string' ? data.preamble : '', 220) || '',
      promptPreview: compactMessageText(JSON.stringify(data?.prompt || {}), 220) || '',
    }
  }

  if (event === 'agent.debug.model_response') {
    return {
      event,
      model: data?.model || null,
      transportMode: data?.transport_mode || null,
      protocol: data?.protocol || null,
      responseId: data?.response_id || null,
      toolCallCount: Array.isArray(data?.tool_calls) ? data.tool_calls.length : 0,
      textPreview: compactMessageText(typeof data?.text === 'string' ? data.text : '', 220) || '',
    }
  }

  if (event === 'agent.debug.model_error') {
    return {
      event,
      model: data?.model || null,
      transportMode: data?.transport_mode || null,
      protocol: data?.protocol || null,
      error: data?.error || null,
      partialTextPreview: compactMessageText(typeof data?.partial_text === 'string' ? data.partial_text : '', 220) || '',
      toolCallCount: Array.isArray(data?.tool_calls) ? data.tool_calls.length : 0,
    }
  }

  if (event === 'message.delta') {
    const content = typeof data?.content === 'string' ? data.content : ''
    return {
      event,
      contentLength: content.length,
      contentPreview: compactMessageText(content, 120) || '',
    }
  }

  if (event === 'reasoning.delta') {
    const delta = typeof data?.delta === 'string'
      ? data.delta
      : typeof data?.content === 'string'
        ? data.content
        : ''
    return {
      event,
      contentLength: delta.length,
      contentPreview: compactMessageText(delta, 120) || '',
    }
  }

  if (event === 'tool.calls.required') {
    return {
      event,
      responseId: data?.response_id || null,
      calls: Array.isArray(data?.calls)
        ? data.calls.map((call: any) => ({
          callId: call?.call_id || null,
          name: call?.name || null,
          arguments: call?.arguments || null,
        }))
        : [],
      contentPreview: compactMessageText(typeof data?.content === 'string' ? data.content : '', 220) || '',
    }
  }

  if (event === 'message.completed') {
    return {
      event,
      responseId: data?.response_id || null,
      contentPreview: compactMessageText(typeof data?.content === 'string' ? data.content : '', 220) || '',
    }
  }

  return {
    event,
    data,
  }
}

export function isAgentInternalInterceptError(message: string) {
  const normalized = message.trim()
  if (!normalized) return false
  return [
    '当前步骤要求执行正文修改，但本轮没有调用有效的正文写入方法。',
    '模型在正文动作未完整闭合时请求了后续工具',
    '计划执行停滞：连续多轮只读取或保存当前文档，但没有真正写入正文。',
    '计划执行空转：连续多轮没有新的工具结果、正文写入或步骤推进。',
    '检测到重复工具调用循环，已停止本次生成，请调整指令后重试',
    '工具调用轮次过多，已停止本次生成，请补充更明确的目标或范围',
  ].some((prefix) => normalized.startsWith(prefix))
}

export function resolveAgentInterceptDetails(message: string) {
  const normalized = message.trim()
  if (!normalized) return null

  if (normalized.startsWith('当前步骤要求执行正文修改，但本轮没有调用有效的正文写入方法。')) {
    return {
      code: 'document_write_method_missing',
      userMessage: '本轮执行已被拦截：当前步骤要求真正写正文，但模型没有调用文档写入或局部编辑方法。下一轮需要先完成正文修改，再继续后续动作。',
      modelGuidance: 'The current step requires a document write. Use execute_browser_javascript with markflow.appendCurrentDocumentContent / markflow.replaceCurrentDocumentContent for full-body writes, or partial-edit markflow methods for scoped edits.'
    }
  }


  if (normalized.startsWith('模型在正文动作未完整闭合时请求了后续工具')) {
    return {
      code: 'action_block_not_closed',
      userMessage: '本轮执行已被拦截：模型在正文写入未完成时请求了后续工具。下一轮需要先通过 markflow 文档写入方法完成正文修改。',
      modelGuidance: 'Complete the document write with execute_browser_javascript and the markflow method from the skill manual before requesting follow-up tools or claiming completion.',
    }
  }

  if (normalized.startsWith('计划执行停滞：连续多轮只读取或保存当前文档，但没有真正写入正文。')) {
    return {
      code: 'plan_stalled_without_write',
      userMessage: '本轮执行已被拦截：模型连续多轮只做读取或保存，没有真正写入正文。下一轮需要重新规划当前步骤，并明确产出正文修改。',
      modelGuidance: 'The current execution is stalled. Re-plan the current step and produce an actual document write instead of repeating read-only or save-only rounds.',
    }
  }

  if (normalized.startsWith('计划执行空转：连续多轮没有新的工具结果、正文写入或步骤推进。')) {
    return {
      code: 'plan_idle_without_progress',
      userMessage: '本轮执行已被拦截：模型连续多轮没有新的工具结果、正文写入或步骤推进，已经进入空转。下一轮需要重规划当前步骤，基于已有结果直接推进，不要继续重复无进展回复。',
      modelGuidance: 'The execution entered an idle loop with no new tool results, document writes, or step advancement. Re-plan the current step from existing state and move execution forward instead of repeating empty continuation rounds.',
    }
  }

  if (normalized.startsWith('检测到重复工具调用循环，已停止本次生成，请调整指令后重试')) {
    return {
      code: 'tool_loop_detected',
      userMessage: '本轮执行已被拦截：模型进入了重复工具调用循环。下一轮需要改变策略，不要再重复同一批工具调用。',
      modelGuidance: 'A repeated tool loop was detected. Change strategy, use prior tool results, and do not repeat the same tool batch unchanged.',
    }
  }

  if (normalized.startsWith('工具调用轮次过多，已停止本次生成，请补充更明确的目标或范围')) {
    return {
      code: 'tool_round_limit_reached',
      userMessage: '本轮执行已被拦截：工具调用轮次过多。下一轮需要收缩目标范围，基于已有结果直接推进，不要继续无界扩张步骤。',
      modelGuidance: 'The tool-call round limit was reached. Narrow the scope, reuse existing results, and continue with fewer steps instead of expanding the plan.',
    }
  }

  return null
}

export function compactMessageText(content: string, maxChars = REQUEST_SUMMARY_ITEM_CHARS) {
  const compact = content.replace(/\s+/g, ' ').trim()
  if (!compact) return ''
  if (compact.length <= maxChars) return compact
  return `${compact.slice(0, maxChars)}...`
}

export function summarizeMessageAttachments(attachments: AgentAttachment[]) {
  if (!attachments.length) return ''
  const imageCount = attachments.filter((attachment) => attachment.kind === 'image').length
  const documentCount = attachments.length - imageCount
  const parts: string[] = []
  if (imageCount) parts.push(`${imageCount} 张图片`)
  if (documentCount) parts.push(`${documentCount} 个文件`)
  return parts.join('、')
}

export function compactMessageSummary(message: AgentMessage, maxChars = REQUEST_SUMMARY_ITEM_CHARS) {
  const compact = compactMessageText(message.content, maxChars)
  const attachmentSummary = summarizeMessageAttachments(message.attachments || [])
  if (compact && attachmentSummary) {
    return compactMessageText(`${compact}（附件：${attachmentSummary}）`, maxChars)
  }
  if (compact) return compact
  if (attachmentSummary) return `附件：${attachmentSummary}`
  return ''
}

export function compactJsonLike(value: unknown, maxChars = REQUEST_SUMMARY_ITEM_CHARS) {
  if (value === null || value === undefined) return ''
  const raw = typeof value === 'string' ? value : JSON.stringify(value)
  return compactMessageText(raw || '', maxChars)
}

export function isInternalExecutionLedgerMessage(content: string) {
  const normalized = content.trim()
  if (!normalized) return false
  return [
    '以下是本轮当前请求的执行账本更新。',
    '执行账本更新：',
    '执行账本提示：',
    '执行账本纠偏：',
  ].some((prefix) => normalized.startsWith(prefix))
}

export function summarizeToolCallBatch(calls: AgentToolCall[], outputs: AgentToolOutputPayload[]) {
  if (!calls.length) return []

  const outputByCallId = new Map(outputs.map((output) => [output.call_id, output]))
  return calls.map((call) => {
    const output = outputByCallId.get(call.call_id)
    const payload = output?.output && typeof output.output === 'object'
      ? output.output as Record<string, any>
      : null
    const ok = typeof payload?.ok === 'boolean' ? payload.ok : null
    const result = payload?.result && typeof payload.result === 'object'
      ? payload.result as Record<string, any>
      : null
    let outcome: AgentExecutionToolCallSummary['outcome'] = ok === false ? 'error' : 'unknown'

    if (call.name === 'save_current_document') {
      const savePerformed = result?.saved === true
      const saveNoop =
        result?.save_action === 'noop'
        || result?.already_saved === true
        || result?.alreadySaved === true
        || result?.unsaved_changes_before_save === false
      if (savePerformed) outcome = 'success'
      else if (saveNoop) outcome = 'noop'
      else if (ok === false) outcome = 'error'
    } else if (ok === true) {
      outcome = 'success'
    }

    return {
      name: call.name,
      arguments: compactMessageText(call.arguments || '', 320) || null,
      output: output ? compactJsonLike(output.output, 360) || null : null,
      ok,
      outcome,
      stagePolicy: typeof call.stage_policy === 'string' ? call.stage_policy : null,
      capabilities: Array.isArray(call.capabilities)
        ? call.capabilities.filter((item): item is string => typeof item === 'string' && Boolean(item.trim()))
        : [],
    }
  })
}

export function buildRoundToolCallSummaryFromToolEvent(raw: any): AgentExecutionToolCallSummary | null {
  if (!raw || typeof raw !== 'object') return null
  const status = typeof raw.status === 'string' && raw.status.trim() ? raw.status.trim() : ''
  if (!['completed', 'failed', 'noop'].includes(status)) return null
  const toolName = typeof raw.tool === 'string' && raw.tool.trim() ? raw.tool.trim() : ''
  if (!toolName) return null

  const payload = raw.output && typeof raw.output === 'object'
    ? raw.output as Record<string, any>
    : null
  const ok = typeof payload?.ok === 'boolean' ? payload.ok : null
  const result = payload?.result && typeof payload.result === 'object'
    ? payload.result as Record<string, any>
    : null
  const rawArguments = typeof raw.arguments === 'string'
    ? raw.arguments
    : raw.arguments && typeof raw.arguments === 'object'
      ? JSON.stringify(raw.arguments)
      : ''

  let outcome: AgentExecutionToolCallSummary['outcome'] = ok === false ? 'error' : 'unknown'
  if (toolName === 'save_current_document') {
    const savePerformed = result?.saved === true
    const saveNoop =
      result?.save_action === 'noop'
      || result?.already_saved === true
      || result?.alreadySaved === true
      || result?.unsaved_changes_before_save === false
    if (savePerformed) outcome = 'success'
    else if (saveNoop) outcome = 'noop'
    else if (ok === false) outcome = 'error'
  } else if (ok === true) {
    outcome = 'success'
  }

  return {
    name: toolName,
    arguments: compactMessageText(rawArguments || '', 320) || null,
    output: compactJsonLike(payload, 360) || null,
    ok,
    outcome,
    stagePolicy: 'mutation',
    capabilities: ['update'],
  }
}

export function parseToolArguments(argumentsText: string | null) {
  if (!argumentsText) return null
  try {
    const parsed = JSON.parse(argumentsText)
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? parsed as Record<string, any>
      : null
  } catch {
    return null
  }
}

export function coerceBooleanLike(value: unknown): boolean | null {
  if (typeof value === 'boolean') return value
  if (typeof value === 'string') {
    const normalized = value.trim().toLowerCase()
    if (normalized === 'true') return true
    if (normalized === 'false') return false
  }
  return null
}

export function coerceNumberLike(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string' && value.trim()) {
    const parsed = Number(value.trim())
    if (Number.isFinite(parsed)) return parsed
  }
  return null
}

export function parseJsonLikeValue(value: unknown) {
  if (typeof value !== 'string') return value
  const trimmed = value.trim()
  if (!trimmed) return value
  if (
    (trimmed.startsWith('{') && trimmed.endsWith('}'))
    || (trimmed.startsWith('[') && trimmed.endsWith(']'))
  ) {
    try {
      return JSON.parse(trimmed)
    } catch {
      return value
    }
  }
  return value
}

export function coerceStringListLike(value: unknown): string[] {
  const normalized = parseJsonLikeValue(value)
  if (Array.isArray(normalized)) {
    return normalized
      .filter((item): item is string => typeof item === 'string' && Boolean(item.trim()))
      .map((item) => item.trim())
  }
  return []
}

export function normalizeControlPayload(raw: any): AgentControlBlock | null {
  if (!raw || typeof raw !== 'object') return null
  const normalized = {
    current_mode: typeof raw.current_mode === 'string' && raw.current_mode.trim()
      ? raw.current_mode.trim()
      : typeof raw.currentMode === 'string' && raw.currentMode.trim()
        ? raw.currentMode.trim()
        : null,
    awaiting: typeof raw.awaiting === 'string' && raw.awaiting.trim() ? raw.awaiting.trim() : null,
    current_action_kind: typeof raw.current_action_kind === 'string' && raw.current_action_kind.trim()
      ? raw.current_action_kind.trim()
      : typeof raw.currentActionKind === 'string' && raw.currentActionKind.trim()
        ? raw.currentActionKind.trim()
        : null,
    current_action_status: typeof raw.current_action_status === 'string' && raw.current_action_status.trim()
      ? raw.current_action_status.trim()
      : typeof raw.currentActionStatus === 'string' && raw.currentActionStatus.trim()
        ? raw.currentActionStatus.trim()
        : null,
    current_action_mode: typeof raw.current_action_mode === 'string' && raw.current_action_mode.trim()
      ? raw.current_action_mode.trim()
      : typeof raw.currentActionMode === 'string' && raw.currentActionMode.trim()
        ? raw.currentActionMode.trim()
        : null,
    current_action_target: typeof raw.current_action_target === 'string' && raw.current_action_target.trim()
      ? raw.current_action_target.trim()
      : typeof raw.currentActionTarget === 'string' && raw.currentActionTarget.trim()
        ? raw.currentActionTarget.trim()
        : null,
    confirmation_required: coerceBooleanLike(raw.confirmation_required ?? raw.confirmationRequired),
    phase: typeof raw.phase === 'string' && raw.phase.trim() ? raw.phase.trim() : null,
    pending_plan: coerceBooleanLike(raw.pending_plan ?? raw.pendingPlan),
    auto_continue: coerceBooleanLike(raw.auto_continue ?? raw.autoContinue),
    needs_save: coerceBooleanLike(raw.needs_save ?? raw.needsSave),
    write_scope: typeof raw.write_scope === 'string' && raw.write_scope.trim()
      ? raw.write_scope.trim()
      : typeof raw.writeScope === 'string' && raw.writeScope.trim()
        ? raw.writeScope.trim()
        : null,
    preferred_write_action: typeof raw.preferred_write_action === 'string' && raw.preferred_write_action.trim()
      ? raw.preferred_write_action.trim()
      : typeof raw.preferredWriteAction === 'string' && raw.preferredWriteAction.trim()
        ? raw.preferredWriteAction.trim()
        : null,
    task_kind: typeof raw.task_kind === 'string' && raw.task_kind.trim()
      ? raw.task_kind.trim()
      : typeof raw.taskKind === 'string' && raw.taskKind.trim()
        ? raw.taskKind.trim()
        : null,
    edit_intent: typeof raw.edit_intent === 'string' && raw.edit_intent.trim()
      ? raw.edit_intent.trim()
      : typeof raw.editIntent === 'string' && raw.editIntent.trim()
        ? raw.editIntent.trim()
        : null,
    edit_stage: typeof raw.edit_stage === 'string' && raw.edit_stage.trim()
      ? raw.edit_stage.trim()
      : typeof raw.editStage === 'string' && raw.editStage.trim()
        ? raw.editStage.trim()
        : null,
    save_requested: coerceBooleanLike(raw.save_requested ?? raw.saveRequested),
    write_completed: coerceBooleanLike(raw.write_completed ?? raw.writeCompleted),
    plan_step_index: coerceNumberLike(raw.plan_step_index ?? raw.planStepIndex),
    plan_total_steps: coerceNumberLike(raw.plan_total_steps ?? raw.planTotalSteps),
    plan_current_step: typeof raw.plan_current_step === 'string' && raw.plan_current_step.trim()
      ? raw.plan_current_step.trim()
      : typeof raw.planCurrentStep === 'string' && raw.planCurrentStep.trim()
        ? raw.planCurrentStep.trim()
        : null,
    plan_completed_steps: coerceStringListLike(raw.plan_completed_steps ?? raw.planCompletedSteps),
  }
  return {
    currentMode: normalized.current_mode,
    awaiting: normalized.awaiting,
    currentActionKind: normalized.current_action_kind,
    currentActionStatus: normalized.current_action_status,
    currentActionMode: normalized.current_action_mode,
    currentActionTarget: normalized.current_action_target,
    confirmationRequired: normalized.confirmation_required === true,
    phase: normalized.phase,
    pendingPlan: normalized.pending_plan === true,
    autoContinue: normalized.auto_continue === true,
    needsSave: normalized.needs_save === true,
    writeScope: normalized.write_scope,
    preferredWriteAction: normalized.preferred_write_action,
    taskKind: normalized.task_kind,
    editIntent: normalized.edit_intent,
    editStage: normalized.edit_stage,
    saveRequested: normalized.save_requested === true,
    writeCompleted: normalized.write_completed === true,
    planStepIndex: normalized.plan_step_index,
    planTotalSteps: normalized.plan_total_steps,
    planCurrentStep: normalized.plan_current_step,
    planCompletedSteps: normalized.plan_completed_steps,
  }
}

export function buildControlFromExecutionState(
  executionState: AgentExecutionState,
  taskAnalysis: AgentTaskAnalysis | null,
): AgentControlBlock {
  return {
    currentMode: executionState.currentMode,
    awaiting: executionState.awaiting,
    currentActionKind: executionState.currentActionKind,
    currentActionStatus: executionState.currentActionStatus,
    currentActionMode: executionState.currentActionMode,
    currentActionTarget: executionState.currentActionTarget,
    confirmationRequired: executionState.confirmationRequired,
    phase: null,
    pendingPlan: Boolean(executionState.confirmationRequired && executionState.pendingPlan),
    autoContinue: executionState.semanticContinuation,
    needsSave: executionState.saveRequested,
    writeScope: taskAnalysis?.writeScope || null,
    preferredWriteAction: taskAnalysis?.preferredWriteAction || null,
    taskKind: executionState.taskKind,
    editIntent: executionState.editIntent,
    editStage: executionState.editStage,
    saveRequested: executionState.saveRequested,
    writeCompleted: executionState.writeCompleted,
    planStepIndex: executionState.planStepIndex,
    planTotalSteps: executionState.planTotalSteps,
    planCurrentStep: executionState.planCurrentStep,
    planCompletedSteps: [...executionState.planCompletedSteps],
  }
}

export function isHostStateTool(name: string) {
  return (
    name === 'plan_start'
    || name === 'plan_step_update'
    || name === 'plan_complete'
    || name === 'plan_cancel'
    || name === 'write_start'
    || name === 'write_end'
  )
}

export function summarizeRoundActions(roundToolCalls: AgentExecutionToolCallSummary[]) {
  if (!roundToolCalls.length) return []

  const createdDirs: string[] = []
  const createdDocs: string[] = []
  const writtenDocs: string[] = []
  let createdProject: string | null = null
  const otherActions: string[] = []

  for (const call of roundToolCalls) {
    const args = parseToolArguments(call.arguments)
    const outputPayload = parseToolArguments(call.output)
    const outputResult = outputPayload?.result && typeof outputPayload.result === 'object'
      ? outputPayload.result as Record<string, any>
      : null
    const outputResultText = typeof outputPayload?.result === 'string' ? outputPayload.result.trim() : ''
    switch (call.name) {
      case 'create_project': {
        const name = typeof args?.name === 'string' && args.name.trim() ? args.name.trim() : ''
        createdProject = name || '新项目'
        break
      }
      case 'create_tree_node': {
        const name = typeof args?.name === 'string' && args.name.trim() ? args.name.trim() : ''
        const nodeType = typeof args?.node_type === 'string' ? args.node_type.trim() : ''
        if (name) {
          if (nodeType === 'dir') createdDirs.push(name)
          else if (nodeType === 'doc') createdDocs.push(name)
        }
        break
      }
      case 'open_tree_node': {
        break
      }
      case 'read_document':
      case 'read_editor_snapshot':
      case 'get_project_tree':
      case 'list_projects':
      case 'get_current_page_state':
        break
      default:
        otherActions.push(call.name)
        break
    }
  }

  const parts: string[] = []
  if (createdProject) {
    parts.push(`已创建项目《${createdProject}》。`)
  }
  if (createdDirs.length) {
    parts.push(
      createdDirs.length <= 3
        ? `已创建目录：${createdDirs.map((name) => `《${name}》`).join('、')}。`
        : `已创建 ${createdDirs.length} 个目录。`,
    )
  }
  if (createdDocs.length) {
    parts.push(
      createdDocs.length <= 3
        ? `已创建文档：${createdDocs.map((name) => `《${name}》`).join('、')}。`
        : `已创建 ${createdDocs.length} 篇文档。`,
    )
  }
  const uniqueWrittenDocs = [...new Set(writtenDocs)]
  if (uniqueWrittenDocs.length) {
    parts.push(
      uniqueWrittenDocs.length <= 4
        ? `已写入正文：${uniqueWrittenDocs.map((name) => `《${name}》`).join('、')}。`
        : `已写入 ${uniqueWrittenDocs.length} 篇文档正文。`,
    )
  }
  return parts
}

export function appendRecentToolCalls(
  existing: AgentExecutionToolCallSummary[],
  nextBatch: AgentExecutionToolCallSummary[],
) {
  if (!nextBatch.length) return existing
  return [...existing, ...nextBatch].slice(-8)
}

export function extractSaveNoopState(outputs: AgentToolOutputPayload[]) {
  for (const output of outputs) {
    if (output.name !== 'save_current_document') continue
    const payload = output.output
    if (!payload || typeof payload !== 'object') continue
    const result = (payload as Record<string, any>).result
    if (!result || typeof result !== 'object') continue
    const saved = result.saved === true
    const saveAction = result.save_action
    const alreadySaved = result.already_saved === true || result.alreadySaved === true
    const unsavedChangesBeforeSave = result.unsaved_changes_before_save
    if (!saved && (saveAction === 'noop' || alreadySaved || unsavedChangesBeforeSave === false)) {
      return true
    }
  }
  return false
}

export function isReadOrSaveOnlyBatch(roundToolCalls: AgentExecutionToolCallSummary[]) {
  if (!roundToolCalls.length) return false
  return roundToolCalls.every((call) =>
    toolHasOnlyCapabilities(call.name, ['read', 'save'], {
      stagePolicy: call.stagePolicy,
      capabilities: call.capabilities,
    }),
  )
}

export function hasSuccessfulMutationToolCall(roundToolCalls: AgentExecutionToolCallSummary[]) {
  return roundToolCalls.some((call) => (
    call.outcome === 'success'
    && !isHostStateTool(call.name)
    && !toolHasOnlyCapabilities(call.name, ['read', 'save'], {
      stagePolicy: call.stagePolicy,
      capabilities: call.capabilities,
    })
  ))
}

export function buildHistorySummary(messages: AgentMessage[]) {
  const userItems: string[] = []
  const assistantItems: string[] = []

  for (const message of messages) {
    const compact = compactMessageSummary(message)
    if (!compact) continue

    if (message.role === 'user') {
      if (userItems.length < REQUEST_SUMMARY_MAX_ITEMS) {
        userItems.push(`- ${compact}`)
      }
      continue
    }

    if (message.role === 'assistant') {
      if (assistantItems.length < REQUEST_SUMMARY_MAX_ITEMS) {
        assistantItems.push(`- ${compact}`)
      }
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

export function buildSessionMemory(session: AgentSession): AgentSessionMemory | null {
  const recentUserGoals = session.messages
    .filter((message) => message.role === 'user')
    .slice(-3)
    .map((message) => compactMessageText(message.content, 180))
    .filter((item): item is string => Boolean(item))

  const completedFacts: string[] = []
  if (session.lastExecutionMemory?.assistantSummary) {
    completedFacts.push(session.lastExecutionMemory.assistantSummary)
  }
  if (session.lastExecutionMemory?.planCompletedSteps?.length) {
    completedFacts.push(...session.lastExecutionMemory.planCompletedSteps.slice(-3))
  }

  const openLoops: string[] = []
  if (session.pendingPlan) {
    openLoops.push(...parsePlanSteps(session.pendingPlan))
  } else if (session.runtimePlan?.status === 'running') {
    openLoops.push(
      ...session.runtimePlan.steps
        .filter((step) => step.status === 'running' || step.status === 'pending')
        .map((step) => step.title)
        .slice(0, 3),
    )
  }

  const recentSummary = compactMessageText(buildHistorySummary(session.messages.slice(-6)), 400) || null
  if (!recentSummary && !recentUserGoals.length && !completedFacts.length && !openLoops.length) {
    return null
  }

  return {
    summary: recentSummary,
    activeUserGoals: recentUserGoals,
    completedFacts: completedFacts.slice(-4),
    openLoops: openLoops.slice(0, 4),
    updatedAt: new Date().toISOString(),
  }
}

export function buildConversationMessages(
  messages: AgentMessage[],
  options: { preserveFullHistory?: boolean } = {},
): AgentRequestMessage[] {
  const nonEmptyMessages = messages
    .map((message) => ({
      ...message,
      content: message.content.trim(),
    }))
    .filter((message) => (Boolean(message.content) || Boolean(message.attachments?.length)) && message.role !== 'system')
  const normalized = nonEmptyMessages.map((message) => ({
    role: message.role,
    content: message.content,
    attachments: message.attachments || [],
  }))

  if (!normalized.length) return []
  return normalized
}

export function extractPlanBlock(content: string) {
  const match = content.match(/\[\[PLAN\]\]([\s\S]*?)\[\[\/PLAN\]\]/i)
  return match?.[1]?.trim() || ''
}

export function parsePlanSteps(plan: string) {
  return plan
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => line.replace(/^\d+\s*[\.\)、]\s*/, '').trim())
    .filter(Boolean)
}

export function isLikelySaveFollowUpReply(text: string) {
  const normalized = text.trim().toLowerCase()
  if (!normalized) return false
  const compact = normalized.replace(/\s+/g, '')
  const negativePatterns = [
    '不保存',
    '先不保存',
    '暂不保存',
    '不用保存',
    '别保存',
    '取消保存',
    '稍后保存',
    "don'tsave",
    'notsave',
  ]
  if (negativePatterns.some((pattern) => compact.includes(pattern))) {
    return false
  }
  const explicitSavePatterns = [
    '保存',
    '存一下',
    '请保存',
    '确认保存',
    'save',
    'submit',
    'apply',
  ]
  if (explicitSavePatterns.some((pattern) => compact.includes(pattern))) {
    return true
  }
  return [
    '好',
    '好的',
    '行',
    '行的',
    '可以',
    '确认',
    '确定',
    '是',
    '是的',
    '嗯',
    '嗯嗯',
    'ok',
    'okay',
    'yes',
    'y',
    'sure',
  ].includes(compact)
}

export function shouldTreatPlanAsPending(options: {
  planText: string
  control: AgentControlBlock | null
  session: AgentSession
  executionState: AgentExecutionState
  sawPlanSignal: boolean
}) {
  const planText = options.planText.trim()
  if (!planText) return false
  if (options.control?.awaiting === 'user_confirm_write') return false
  if (options.executionState.saveRequested) return false
  if (!options.executionState.confirmationRequired) return false
  if (options.executionState.pendingPlanUserReply?.trim()) return false
  if (options.executionState.planConfirmationDecision === 'approved') return false
  if (options.executionState.semanticContinuation) return false
  if (options.control?.currentMode === 'plan' && options.control?.awaiting === 'user_input') return true
  const runtimePlanStatus = options.session.runtimePlan?.status?.trim()
  const hasPlanRuntime = options.session.taskAnalysis?.mode === 'plan'
    || runtimePlanStatus === 'pending'
  return options.sawPlanSignal && hasPlanRuntime
}

export function isPartialWriteTask(taskAnalysis: AgentTaskAnalysis | null) {
  return taskAnalysis?.writeScope === 'partial'
}

export function isDocumentMutationTool(name: string) {
  return name === 'rewrite_document_section'
    || name === 'replace_document_block'
    || name === 'replace_document_blocks'
    || name === 'swap_document_sections'
}

export function buildExecutionProgressSignature(
  executionState: AgentExecutionState,
  control: AgentControlBlock | null,
) {
  const mode = typeof control?.currentMode === 'string' && control.currentMode.trim()
    ? control.currentMode.trim()
    : executionState.currentMode
  const awaiting = typeof control?.awaiting === 'string' && control.awaiting.trim()
    ? control.awaiting.trim()
    : executionState.awaiting || ''
  const currentStep = typeof control?.planCurrentStep === 'string' && control.planCurrentStep.trim()
    ? control.planCurrentStep.trim()
    : executionState.planCurrentStep?.trim() || ''
  const completedSteps = executionState.planCompletedSteps
    .map((step) => step.trim())
    .filter(Boolean)
    .join('||')
  return [
    mode,
    awaiting,
    String(executionState.planStepIndex || ''),
    String(executionState.planTotalSteps || ''),
    currentStep,
    completedSteps,
    executionState.writeCompleted ? 'write:1' : 'write:0',
    executionState.saveRequested ? 'save:1' : 'save:0',
    executionState.documentWriteObserved ? 'doc:1' : 'doc:0',
  ].join('::')
}

export function parseSseBlock(block: string) {
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
