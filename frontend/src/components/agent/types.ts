// AgentPanel 会话运行时的共享类型定义。
//
// 从 AgentPanel.vue 抽出，仅承载类型（无运行时代码）。外部依赖三处：
// - AgentPageScope       @/agent/protocol
// - AgentWriterMode      @/utils/agentWriter
// - AgentToolOutputPayload @/utils/agentTools

import type { AgentControlBlock, AgentPageScope } from '@/agent/protocol'
import type { AgentWriterMode } from '@/utils/agentWriter'
import type { AgentToolOutputPayload } from '@/utils/agentTools'

export type PageScope = AgentPageScope
export type DocType = 'doc' | 'dir' | null
export type SessionRole = 'user' | 'assistant' | 'system'
export type RequestRole = SessionRole
export type StreamAction = 'chat' | AgentWriterMode
export type RouteKind = 'overview' | 'project' | 'doc'
export type ProviderKind = 'openai' | 'anthropic' | 'gemini'
export type OpenAiApiMode = 'auto' | 'responses' | 'chat'

export interface AgentRouteTarget {
  kind: RouteKind
  name?: string
}

export interface AgentTokenUsage {
  input_tokens?: number
  output_tokens?: number
  total_tokens?: number
  cached_input_tokens?: number
  cache_creation_input_tokens?: number
  tool_use_prompt_tokens?: number
  reasoning_tokens?: number
}

export interface AgentMessage {
  id: string
  role: SessionRole
  content: string
  createdAt: number
  updatedAt: number
  reasoning?: string
  attachments?: AgentAttachment[]
  internalStatus?: boolean
  toolEvents?: AgentToolEvent[]
  usage?: AgentTokenUsage
}

export interface AgentRequestMessage {
  role: RequestRole
  content: string
  attachments?: AgentAttachment[]
}

export interface AgentAttachment {
  localId: string
  uploadId: number
  kind: 'image' | 'document'
  name: string
  url: string
  contentType: string | null
  size: number
}

export interface AgentRuntimePlanStep {
  id: string
  title: string
  kind: string
  description: string
  status: 'pending' | 'running' | 'completed' | 'failed' | 'blocked' | string
  toolHints: string[]
  requiresConfirmation: boolean
  requiresDocumentWrite: boolean
  requiresDocumentSave: boolean
}

export interface AgentRuntimePlan {
  id: string
  goal: string
  summary: string | null
  status: 'pending' | 'running' | 'completed' | 'failed' | 'blocked' | string
  steps: AgentRuntimePlanStep[]
  createdAt: string | null
  updatedAt: string | null
}

export interface AgentTaskAnalysis {
  intent: string
  complexity: string
  mode: 'chat' | 'plan' | string
  requiresTools: boolean
  requiresUserConfirmation: boolean
  writeScope: 'partial' | 'full' | string | null
  preferredWriteAction: AgentWriterMode | string | null
  deliverable: string | null
}

export interface AgentSessionMemory {
  summary: string | null
  activeUserGoals: string[]
  completedFacts: string[]
  openLoops: string[]
  updatedAt: string | null
}

export interface AgentArtifact {
  id: string
  type: 'markdown_doc' | 'folder_plan' | 'project_outline' | string
  title: string
  status: 'drafting' | 'ready' | 'applied' | 'failed' | string
  content: string
  relatedDocId: number | null
}

export interface AgentToolEvent {
  id: string
  tool: string
  status: 'requested' | 'running' | 'completed' | 'failed' | 'noop' | string
  summary: string
  source?: string
  callId?: string
  arguments?: string
  output?: string
}

export interface AgentExecutionToolCallSummary {
  name: string
  arguments: string | null
  output: string | null
  ok: boolean | null
  outcome: 'success' | 'noop' | 'error' | 'unknown'
  stagePolicy?: string | null
  capabilities?: string[]
}

export interface AgentExecutionState {
  currentMode: 'normal' | 'plan' | string
  awaiting: string | null
  currentActionKind: string | null
  currentActionStatus: string | null
  currentActionMode: string | null
  currentActionTarget: string | null
  confirmationRequired: boolean
  pendingPlan: string | null
  pendingPlanUserReply: string | null
  planConfirmationDecision: 'approved' | 'rejected' | null
  compositeWriteThenSave: boolean
  semanticContinuation: boolean
  semanticContinuationRound: number
  previousAssistantSummary: string | null
  taskKind: string | null
  editIntent: string | null
  editStage: string | null
  saveRequested: boolean
  writeCompleted: boolean
  planStepIndex: number | null
  planTotalSteps: number | null
  planCurrentStep: string | null
  planCompletedSteps: string[]
  documentWriteObserved: boolean
  saveAttemptWithoutDocumentChange: boolean
  lastInterceptCode: string | null
  lastInterceptMessage: string | null
  lastInterceptGuidance: string | null
  recentToolCalls: AgentExecutionToolCallSummary[]
}

export interface AgentExecutionMemory {
  currentMode: 'normal' | 'plan' | string
  awaiting: string | null
  currentActionKind: string | null
  currentActionStatus: string | null
  currentActionMode: string | null
  currentActionTarget: string | null
  confirmationRequired: boolean
  plan: string | null
  assistantSummary: string | null
  controlPhase: string | null
  taskKind: string | null
  editIntent: string | null
  editStage: string | null
  saveRequested: boolean
  writeCompleted: boolean
  planStepIndex: number | null
  planTotalSteps: number | null
  planCurrentStep: string | null
  planCompletedSteps: string[]
  documentWriteObserved: boolean
  saveAttemptWithoutDocumentChange: boolean
  lastInterceptCode: string | null
  lastInterceptMessage: string | null
  lastInterceptGuidance: string | null
  recentToolCalls: AgentExecutionToolCallSummary[]
}

export interface AgentStructuredResponse {
  message: string
  state: AgentControlBlock | null
  plan: AgentRuntimePlan | null
}

export interface AgentSession {
  id: string
  title: string
  messages: AgentMessage[]
  taskAnalysis: AgentTaskAnalysis | null
  runtimePlan: AgentRuntimePlan | null
  artifacts: AgentArtifact[]
  toolEvents: AgentToolEvent[]
  providerId: number | null
  model: string
  previousResponseId: string | null
  pendingPlan: string | null
  pendingPlanToolOutputs: AgentToolOutputPayload[]
  lastPlan: string | null
  lastExecutionMemory: AgentExecutionMemory | null
  sessionMemory: AgentSessionMemory | null
  lastSyncedMessageCount: number
  createdAt: number
  updatedAt: number
}

export type McpTransport = 'sse' | 'streamable-http' | 'stdio'
export type McpAuthType = 'none' | 'bearer' | 'basic' | 'header' | 'query'
export type McpSecretCollectionMode = 'keep' | 'replace' | 'clear'
export type McpSnapshotTab = 'tools' | 'resources' | 'prompts'

export interface McpRuntimeCapabilities {
  transports: McpTransport[]
  stdioEnabled: boolean
  stdioAllowedCommands: string[]
}

export interface McpRuntimeCapabilitiesApiResponse {
  transports?: string[]
  stdio_enabled?: boolean
  stdio_allowed_commands?: string[]
}

export interface McpSettingsState {
  enabled: boolean
  updatedAt: number | null
}

export interface McpSettingsApiResponse {
  settings?: {
    enabled?: boolean
    updated_at?: string | null
  }
}

export interface McpSecretValueResponse {
  has_value?: boolean
  value?: string | null
}

export interface McpSecretEntry {
  name: string
  hasValue: boolean
  value: string
}

export interface McpSecretEntryApiResponse {
  name?: string
  has_value?: boolean
  value?: string | null
}

export interface McpAuthDetail {
  authType: McpAuthType
  scheme: string
  username: string
  headerName: string
  queryName: string
  tokenHasValue: boolean
  passwordHasValue: boolean
  valueHasValue: boolean
  tokenValue: string
  passwordValue: string
  valueText: string
}

export interface McpAuthDetailApiResponse {
  auth_type?: string
  scheme?: string | null
  username?: string | null
  header_name?: string | null
  query_name?: string | null
  token?: McpSecretValueResponse
  password?: McpSecretValueResponse
  value?: McpSecretValueResponse
}

export interface McpServerSummary {
  id: string
  name: string
  enabled: boolean
  transport: McpTransport
  url: string
  command: string
  authType: McpAuthType
  lastStatus: string | null
  lastError: string | null
  configVersion: number
  createdAt: number
  updatedAt: number
}

export interface McpServerSummaryApiResponse {
  id: number | string
  name?: string
  enabled?: boolean
  transport?: string
  url?: string | null
  command?: string | null
  auth_type?: string
  last_status?: string | null
  last_error?: string | null
  config_version?: number
  created_at?: string
  updated_at?: string
}

export interface McpServerDetail extends McpServerSummary {
  args: string[]
  auth: McpAuthDetail | null
  customHeaders: McpSecretEntry[]
  stdioEnv: McpSecretEntry[]
  toolsSnapshot: unknown[]
  resourcesSnapshot: unknown[]
  promptsSnapshot: unknown[]
  lastSyncAt: number | null
}

export interface McpServerDetailApiResponse extends McpServerSummaryApiResponse {
  args?: string[]
  auth?: McpAuthDetailApiResponse | null
  custom_headers?: McpSecretEntryApiResponse[]
  stdio_env?: McpSecretEntryApiResponse[]
  tools_snapshot?: unknown
  resources_snapshot?: unknown
  prompts_snapshot?: unknown
  last_sync_at?: string | null
}

export interface McpServersResponse {
  servers?: McpServerSummaryApiResponse[]
}

export interface McpServerResponse {
  server?: McpServerDetailApiResponse
}

export interface McpDraft {
  id: string | null
  name: string
  enabled: boolean
  transport: McpTransport
  url: string
  command: string
  argsText: string
  authType: McpAuthType
  bearerScheme: string
  bearerToken: string
  authUsername: string
  authPassword: string
  authHeaderName: string
  authHeaderValue: string
  authQueryName: string
  authQueryValue: string
  customHeadersMode: McpSecretCollectionMode
  customHeadersText: string
  stdioEnvMode: McpSecretCollectionMode
  stdioEnvText: string
  existingCustomHeaders: McpSecretEntry[]
  existingStdioEnv: McpSecretEntry[]
  authSecretFlags: {
    token: boolean
    password: boolean
    value: boolean
  }
  lastStatus: string | null
  lastError: string | null
  configVersion: number
  toolsSnapshot: unknown[]
  resourcesSnapshot: unknown[]
  promptsSnapshot: unknown[]
  lastSyncAt: number | null
  createdAt: number
  updatedAt: number
}
