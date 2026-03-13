import protocol from '@/agent/agent-protocol.json'
import type { AgentWriterMode } from '@/utils/agentWriter'

type AgentRouteRecord = (typeof protocol.routes)[number]
type AgentWriteActionRecord = (typeof protocol.writeActions)[number]
type AgentControlRecord = typeof protocol.control
type AgentTaskAnalysisRecord = typeof protocol.taskAnalysis

export type AgentPageScope = 'overview' | 'editor' | 'dir'
export type AgentPageState =
  | 'project_overview'
  | 'document_editor'
  | 'directory_detail'
  | 'project_workspace'
export type AgentEditorSnapshotSource =
  | 'editor_bridge'
  | 'agent_snapshot'
  | 'draft_cache'
  | 'saved_document'
export type AgentRouteName = AgentRouteRecord['route']

export interface AgentRouteDefinition {
  route: AgentRouteName
  aliases: string[]
  path: string
  description: string
  params: string[]
  nodeType?: 'doc' | 'dir'
}

export const DEFAULT_AGENT_BASE_URL = protocol.defaultBaseUrl
export const AGENT_WRITE_ACTIONS = protocol.writeActions as readonly AgentWriteActionRecord[]
export const AGENT_WRITE_ACTION_MODES = AGENT_WRITE_ACTIONS.map((item) => item.mode) as AgentWriterMode[]
export const AGENT_ACTION_CLOSE_MARKER = '[[/ACTION]]'
export const AGENT_CONTROL_OPEN_MARKER = protocol.control.openMarker
export const AGENT_CONTROL_CLOSE_MARKER = protocol.control.closeMarker
export const AGENT_CONTROL_PHASES = protocol.control.phases as readonly AgentControlRecord['phases'][number][]
export const AGENT_TASK_ANALYSIS_MODES = protocol.taskAnalysis.modes as readonly AgentTaskAnalysisRecord['modes'][number][]
export const AGENT_TASK_ANALYSIS_COMPLEXITIES = protocol.taskAnalysis.complexities as readonly AgentTaskAnalysisRecord['complexities'][number][]
export const AGENT_TASK_ANALYSIS_INTENTS = protocol.taskAnalysis.intents as readonly AgentTaskAnalysisRecord['intents'][number][]
export const AGENT_TASK_ANALYSIS_WRITE_SCOPES = protocol.taskAnalysis.writeScopes as readonly AgentTaskAnalysisRecord['writeScopes'][number][]

export type AgentControlPhase = AgentControlRecord['phases'][number]
export type AgentTaskAnalysisMode = AgentTaskAnalysisRecord['modes'][number]
export type AgentTaskAnalysisComplexity = AgentTaskAnalysisRecord['complexities'][number]
export type AgentTaskAnalysisIntent = AgentTaskAnalysisRecord['intents'][number]
export type AgentTaskAnalysisWriteScope = AgentTaskAnalysisRecord['writeScopes'][number]

export interface AgentControlBlock {
  phase?: AgentControlPhase | string | null
  pendingPlan?: boolean
  autoContinue?: boolean
  needsSave?: boolean
  writeScope?: string | null
  preferredWriteAction?: string | null
  taskKind?: string | null
  editIntent?: string | null
  editStage?: string | null
  saveRequested?: boolean
  writeCompleted?: boolean
  planStepIndex?: number | null
  planTotalSteps?: number | null
  planCurrentStep?: string | null
  planCompletedSteps?: string[]
}

export const AGENT_WRITE_ACTION_OPEN_MARKERS = AGENT_WRITE_ACTIONS.map((item) => ({
  marker: item.marker,
  mode: item.mode as AgentWriterMode,
}))
export const AGENT_ROUTE_DEFINITIONS = protocol.routes as readonly AgentRouteDefinition[]

function escapeRegex(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

export function getAgentControlBlockRegex(flags = 'i') {
  return new RegExp(
    `${escapeRegex(AGENT_CONTROL_OPEN_MARKER)}([\\s\\S]*?)${escapeRegex(AGENT_CONTROL_CLOSE_MARKER)}`,
    flags,
  )
}

export function resolveAgentPageScope(
  showProjectOverview: boolean,
  nodeType: 'doc' | 'dir' | null | undefined,
): AgentPageScope {
  if (showProjectOverview) return 'overview'
  if (nodeType === 'doc') return 'editor'
  return 'dir'
}

export function resolveAgentPageState(
  showProjectOverview: boolean,
  nodeType: 'doc' | 'dir' | null | undefined,
): AgentPageState {
  if (showProjectOverview) return 'project_overview'
  if (nodeType === 'doc') return 'document_editor'
  if (nodeType === 'dir') return 'directory_detail'
  return 'project_workspace'
}

export function resolveAgentEditorSnapshotSource(options: {
  hasLiveEditor: boolean
  hasAgentSnapshot?: boolean
  hasDraftCache: boolean
}): AgentEditorSnapshotSource {
  if (options.hasLiveEditor) return 'editor_bridge'
  if (options.hasAgentSnapshot) return 'agent_snapshot'
  if (options.hasDraftCache) return 'draft_cache'
  return 'saved_document'
}

export function resolveAgentRouteDefinition(routeName: string) {
  const normalized = routeName.trim()
  if (!normalized) return null
  return AGENT_ROUTE_DEFINITIONS.find((route) => route.aliases.includes(normalized)) || null
}

export function resolveAgentWriteMode(marker: string): AgentWriterMode | null {
  const normalized = marker.trim().toUpperCase()
  const matched = AGENT_WRITE_ACTION_OPEN_MARKERS.find(
    (item) => item.marker.toUpperCase() === normalized,
  )
  return matched?.mode || null
}

export function extractAgentControlBlock(content: string): AgentControlBlock | null {
  const match = content.match(getAgentControlBlockRegex())
  if (!match?.[1]) return null

  try {
    const parsed = JSON.parse(match[1])
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null
    const raw = parsed as Record<string, unknown>
    const phase = typeof raw.phase === 'string' && raw.phase.trim() ? raw.phase.trim() : null
    const pendingPlan = raw.pending_plan === true || raw.pendingPlan === true
    const autoContinue = raw.auto_continue === true || raw.autoContinue === true
    const needsSave = raw.needs_save === true || raw.needsSave === true
    const writeScope = typeof raw.write_scope === 'string' && raw.write_scope.trim()
      ? raw.write_scope.trim()
      : typeof raw.writeScope === 'string' && raw.writeScope.trim()
        ? raw.writeScope.trim()
        : null
    const preferredWriteAction = typeof raw.preferred_write_action === 'string' && raw.preferred_write_action.trim()
      ? raw.preferred_write_action.trim()
      : typeof raw.preferredWriteAction === 'string' && raw.preferredWriteAction.trim()
        ? raw.preferredWriteAction.trim()
        : null
    const taskKind = typeof raw.task_kind === 'string' && raw.task_kind.trim()
      ? raw.task_kind.trim()
      : typeof raw.taskKind === 'string' && raw.taskKind.trim()
        ? raw.taskKind.trim()
        : null
    const editIntent = typeof raw.edit_intent === 'string' && raw.edit_intent.trim()
      ? raw.edit_intent.trim()
      : typeof raw.editIntent === 'string' && raw.editIntent.trim()
        ? raw.editIntent.trim()
        : null
    const editStage = typeof raw.edit_stage === 'string' && raw.edit_stage.trim()
      ? raw.edit_stage.trim()
      : typeof raw.editStage === 'string' && raw.editStage.trim()
        ? raw.editStage.trim()
        : null
    const saveRequested = raw.save_requested === true || raw.saveRequested === true
    const writeCompleted = raw.write_completed === true || raw.writeCompleted === true
    const planStepIndex = Number.isFinite(raw.plan_step_index)
      ? Number(raw.plan_step_index)
      : Number.isFinite(raw.planStepIndex)
        ? Number(raw.planStepIndex)
        : null
    const planTotalSteps = Number.isFinite(raw.plan_total_steps)
      ? Number(raw.plan_total_steps)
      : Number.isFinite(raw.planTotalSteps)
        ? Number(raw.planTotalSteps)
        : null
    const planCurrentStep = typeof raw.plan_current_step === 'string' && raw.plan_current_step.trim()
      ? raw.plan_current_step.trim()
      : typeof raw.planCurrentStep === 'string' && raw.planCurrentStep.trim()
        ? raw.planCurrentStep.trim()
        : null
    const planCompletedSteps = Array.isArray(raw.plan_completed_steps)
      ? raw.plan_completed_steps
        .filter((item): item is string => typeof item === 'string' && Boolean(item.trim()))
        .map((item) => item.trim())
      : Array.isArray(raw.planCompletedSteps)
        ? raw.planCompletedSteps
          .filter((item): item is string => typeof item === 'string' && Boolean(item.trim()))
          .map((item) => item.trim())
        : []

    return {
      phase,
      pendingPlan,
      autoContinue,
      needsSave,
      writeScope,
      preferredWriteAction,
      taskKind,
      editIntent,
      editStage,
      saveRequested,
      writeCompleted,
      planStepIndex,
      planTotalSteps,
      planCurrentStep,
      planCompletedSteps,
    }
  } catch {
    return null
  }
}

export function controlRequestsPlanConfirmation(control: AgentControlBlock | null) {
  return Boolean(control?.pendingPlan || control?.phase === 'await_user_confirmation')
}

export function controlRequestsAutoContinuation(control: AgentControlBlock | null) {
  return Boolean(
    control?.autoContinue
    || control?.phase === 'auto_continue'
    || control?.phase === 'in_progress',
  )
}

export function controlNeedsSave(control: AgentControlBlock | null) {
  return Boolean(control?.needsSave || control?.phase === 'needs_save')
}
