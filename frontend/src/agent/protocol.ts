import protocol from '@/agent/agent-protocol.json'

type AgentRouteRecord = (typeof protocol.routes)[number]
type AgentTaskAnalysisRecord = typeof protocol.taskAnalysis
type AgentToolPolicyRecord = (typeof protocol.toolPolicies)[number]

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
export type AgentToolStagePolicy = AgentToolPolicyRecord['stagePolicy']
export type AgentToolCapability = AgentToolPolicyRecord['capabilities'][number]

export interface AgentRouteDefinition {
  route: AgentRouteName
  aliases: string[]
  path: string
  description: string
  params: string[]
  nodeType?: 'doc' | 'dir'
}

export interface AgentToolPolicyDefinition {
  name: string
  stagePolicy: AgentToolStagePolicy
  capabilities: AgentToolCapability[]
}

export interface AgentToolProtocolMetadata {
  stagePolicy?: string | null
  capabilities?: readonly (string | null | undefined)[] | null
}

export const DEFAULT_AGENT_BASE_URL = protocol.defaultBaseUrl
export const AGENT_TASK_ANALYSIS_MODES = protocol.taskAnalysis.modes as readonly AgentTaskAnalysisRecord['modes'][number][]
export const AGENT_TASK_ANALYSIS_COMPLEXITIES = protocol.taskAnalysis.complexities as readonly AgentTaskAnalysisRecord['complexities'][number][]
export const AGENT_TASK_ANALYSIS_INTENTS = protocol.taskAnalysis.intents as readonly AgentTaskAnalysisRecord['intents'][number][]
export const AGENT_TASK_ANALYSIS_WRITE_SCOPES = protocol.taskAnalysis.writeScopes as readonly AgentTaskAnalysisRecord['writeScopes'][number][]

export type AgentTaskAnalysisMode = AgentTaskAnalysisRecord['modes'][number]
export type AgentTaskAnalysisComplexity = AgentTaskAnalysisRecord['complexities'][number]
export type AgentTaskAnalysisIntent = AgentTaskAnalysisRecord['intents'][number]
export type AgentTaskAnalysisWriteScope = AgentTaskAnalysisRecord['writeScopes'][number]

export interface AgentControlBlock {
  currentMode?: string | null
  awaiting?: string | null
  currentActionKind?: string | null
  currentActionStatus?: string | null
  currentActionMode?: string | null
  currentActionTarget?: string | null
  confirmationRequired?: boolean
  phase?: string | null
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

export const AGENT_ROUTE_DEFINITIONS = protocol.routes as readonly AgentRouteDefinition[]
export const AGENT_TOOL_POLICIES = protocol.toolPolicies as readonly AgentToolPolicyDefinition[]

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

export function resolveAgentToolPolicy(name: string) {
  const normalized = name.trim()
  if (!normalized) return null
  return AGENT_TOOL_POLICIES.find((policy) => policy.name === normalized) || null
}

export function resolveAgentToolStagePolicy(name: string, metadata?: AgentToolProtocolMetadata) {
  const fromMetadata = typeof metadata?.stagePolicy === 'string'
    ? metadata.stagePolicy.trim()
    : ''
  if (fromMetadata === 'observation' || fromMetadata === 'mutation') {
    return fromMetadata as AgentToolStagePolicy
  }
  return resolveAgentToolPolicy(name)?.stagePolicy || null
}

export function resolveAgentToolCapabilities(name: string, metadata?: AgentToolProtocolMetadata) {
  const fromMetadata = Array.isArray(metadata?.capabilities)
    ? metadata.capabilities
      .filter((item): item is AgentToolCapability => (
        item === 'read'
        || item === 'save'
        || item === 'create'
        || item === 'update'
        || item === 'delete'
        || item === 'move'
        || item === 'navigate'
        || item === 'execute'
      ))
    : []
  if (fromMetadata.length) {
    return fromMetadata
  }
  return resolveAgentToolPolicy(name)?.capabilities || []
}

export function toolAllowsPreConfirmationExecution(name: string, metadata?: AgentToolProtocolMetadata) {
  return resolveAgentToolStagePolicy(name, metadata) === 'observation'
}

export function toolHasOnlyCapabilities(
  name: string,
  allowedCapabilities: readonly AgentToolCapability[],
  metadata?: AgentToolProtocolMetadata,
) {
  const capabilities = resolveAgentToolCapabilities(name, metadata)
  return Boolean(capabilities.length) && capabilities.every((capability) => allowedCapabilities.includes(capability))
}

