// MCP 服务管理的 API 层。
//
// 从 AgentPanel 抽出，承载 MCP 相关类型、上游响应归一化与请求函数。
// 所有请求经 @/utils/request（axios 实例，已解包 response.data 并处理 401/403）。

import request from '@/utils/request'
import { genId } from '@/components/agent/id'

// -------- 基础枚举 --------

export type McpTransport = 'sse' | 'streamable-http' | 'stdio'
export type McpAuthType = 'none' | 'bearer' | 'basic' | 'header' | 'query'
export type McpSecretCollectionMode = 'keep' | 'replace' | 'clear'
export type McpSnapshotTab = 'tools' | 'resources' | 'prompts'

// -------- 运行能力 / 全局设置 --------

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

// -------- 密钥 / 认证 --------

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

// -------- 服务摘要 / 详情 --------

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

// -------- 表单草稿 --------

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

// -------- 通用小工具 --------

export function normalizeOptionalTimestamp(value: unknown): number | null {
  if (typeof value === 'string' && value.trim()) {
    const timestamp = new Date(value).getTime()
    return Number.isFinite(timestamp) ? timestamp : null
  }
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value
  }
  return null
}

// -------- 归一化 --------

export function normalizeMcpTransport(value: unknown): McpTransport | null {
  if (typeof value !== 'string') return null
  const normalized = value.trim().toLowerCase().replace(/_/g, '-')
  if (normalized === 'streamable-http') return 'streamable-http'
  if (normalized === 'stdio') return 'stdio'
  if (normalized === 'sse') return 'sse'
  return null
}

export function normalizeMcpAuthType(value: unknown): McpAuthType {
  if (typeof value !== 'string') return 'none'
  const normalized = value.trim().toLowerCase().replace(/_/g, '-')
  if (normalized === 'bearer' || normalized === 'basic' || normalized === 'header' || normalized === 'query') {
    return normalized
  }
  return 'none'
}

export function mcpTransportLabel(transport: string): string {
  if (transport === 'streamable-http') return 'Streamable HTTP'
  if (transport === 'stdio') return 'STDIO'
  return 'SSE'
}

export function mcpStatusLabel(status: string | null | undefined): string {
  if (!status) return '未检测'
  if (status === 'ready') return '已就绪'
  if (status === 'error') return '异常'
  if (status === 'disabled') return '已禁用'
  return status
}

function normalizeMcpSecretEntry(raw: unknown): McpSecretEntry | null {
  if (!raw || typeof raw !== 'object') return null
  const name = typeof (raw as any).name === 'string' ? (raw as any).name.trim() : ''
  if (!name) return null
  return {
    name,
    hasValue: (raw as any).has_value === true || (raw as any).hasValue === true,
    value: typeof (raw as any).value === 'string' ? (raw as any).value : '',
  }
}

function normalizeMcpAuthDetail(raw: unknown): McpAuthDetail | null {
  if (!raw || typeof raw !== 'object') return null
  return {
    authType: normalizeMcpAuthType((raw as any).auth_type ?? (raw as any).authType),
    scheme: typeof (raw as any).scheme === 'string' ? (raw as any).scheme : '',
    username: typeof (raw as any).username === 'string' ? (raw as any).username : '',
    headerName: typeof (raw as any).header_name === 'string' ? (raw as any).header_name : '',
    queryName: typeof (raw as any).query_name === 'string' ? (raw as any).query_name : '',
    tokenHasValue: (raw as any).token?.has_value === true || (raw as any).token?.hasValue === true,
    passwordHasValue: (raw as any).password?.has_value === true || (raw as any).password?.hasValue === true,
    valueHasValue: (raw as any).value?.has_value === true || (raw as any).value?.hasValue === true,
    tokenValue: typeof (raw as any).token?.value === 'string' ? (raw as any).token.value : '',
    passwordValue: typeof (raw as any).password?.value === 'string' ? (raw as any).password.value : '',
    valueText: typeof (raw as any).value?.value === 'string' ? (raw as any).value.value : '',
  }
}

export function normalizeMcpServerSummary(raw: unknown): McpServerSummary | null {
  if (!raw || typeof raw !== 'object') return null
  const transport = normalizeMcpTransport((raw as any).transport)
  if (!transport) return null
  return {
    id: `${(raw as any).id ?? ''}`.trim() || genId(),
    name: typeof (raw as any).name === 'string' && (raw as any).name.trim() ? (raw as any).name.trim() : '未命名 MCP',
    enabled: (raw as any).enabled === true,
    transport,
    url: typeof (raw as any).url === 'string' ? (raw as any).url.trim() : '',
    command: typeof (raw as any).command === 'string' ? (raw as any).command.trim() : '',
    authType: normalizeMcpAuthType((raw as any).auth_type ?? (raw as any).authType),
    lastStatus: typeof (raw as any).last_status === 'string'
      ? (raw as any).last_status
      : typeof (raw as any).lastStatus === 'string'
        ? (raw as any).lastStatus
        : null,
    lastError: typeof (raw as any).last_error === 'string'
      ? (raw as any).last_error
      : typeof (raw as any).lastError === 'string'
        ? (raw as any).lastError
        : null,
    configVersion: Number.isFinite((raw as any).config_version) ? Number((raw as any).config_version) : Number((raw as any).configVersion) || 1,
    createdAt: normalizeOptionalTimestamp((raw as any).created_at ?? (raw as any).createdAt) || Date.now(),
    updatedAt: normalizeOptionalTimestamp((raw as any).updated_at ?? (raw as any).updatedAt) || Date.now(),
  }
}

export function normalizeMcpServerDetail(raw: unknown): McpServerDetail | null {
  const summary = normalizeMcpServerSummary(raw)
  if (!summary || !raw || typeof raw !== 'object') return null
  return {
    ...summary,
    args: Array.isArray((raw as any).args)
      ? (raw as any).args
        .filter((value: unknown): value is string => typeof value === 'string')
        .map((value: string) => value.trim())
        .filter(Boolean)
      : [],
    auth: normalizeMcpAuthDetail((raw as any).auth),
    customHeaders: Array.isArray((raw as any).custom_headers ?? (raw as any).customHeaders)
      ? ((raw as any).custom_headers ?? (raw as any).customHeaders)
        .map((item: unknown) => normalizeMcpSecretEntry(item))
        .filter((item: McpSecretEntry | null): item is McpSecretEntry => Boolean(item))
      : [],
    stdioEnv: Array.isArray((raw as any).stdio_env ?? (raw as any).stdioEnv)
      ? ((raw as any).stdio_env ?? (raw as any).stdioEnv)
        .map((item: unknown) => normalizeMcpSecretEntry(item))
        .filter((item: McpSecretEntry | null): item is McpSecretEntry => Boolean(item))
      : [],
    toolsSnapshot: Array.isArray((raw as any).tools_snapshot ?? (raw as any).toolsSnapshot) ? ((raw as any).tools_snapshot ?? (raw as any).toolsSnapshot) : [],
    resourcesSnapshot: Array.isArray((raw as any).resources_snapshot ?? (raw as any).resourcesSnapshot) ? ((raw as any).resources_snapshot ?? (raw as any).resourcesSnapshot) : [],
    promptsSnapshot: Array.isArray((raw as any).prompts_snapshot ?? (raw as any).promptsSnapshot) ? ((raw as any).prompts_snapshot ?? (raw as any).promptsSnapshot) : [],
    lastSyncAt: normalizeOptionalTimestamp((raw as any).last_sync_at ?? (raw as any).lastSyncAt),
  }
}

export function normalizeMcpRuntimeCapabilities(raw: unknown): McpRuntimeCapabilities {
  const transports = Array.isArray((raw as any)?.transports)
    ? (raw as any).transports
      .map((value: unknown) => normalizeMcpTransport(value))
      .filter((value: McpTransport | null, index: number, list: Array<McpTransport | null>): value is McpTransport => Boolean(value) && list.indexOf(value) === index)
    : ['sse', 'streamable-http']
  return {
    transports: transports.length ? transports : ['sse', 'streamable-http'],
    stdioEnabled: (raw as any)?.stdio_enabled === true || (raw as any)?.stdioEnabled === true,
    stdioAllowedCommands: Array.isArray((raw as any)?.stdio_allowed_commands ?? (raw as any)?.stdioAllowedCommands)
      ? ((raw as any).stdio_allowed_commands ?? (raw as any).stdioAllowedCommands)
        .filter((value: unknown): value is string => typeof value === 'string')
        .map((value: string) => value.trim())
        .filter(Boolean)
      : [],
  }
}

// -------- 请求函数 --------

export function fetchMcpRuntimeCapabilities(): Promise<McpRuntimeCapabilitiesApiResponse> {
  return request.get('/agent/mcps/runtime-capabilities')
}

export function fetchMcpSettings(): Promise<McpSettingsApiResponse> {
  return request.get('/agent/mcps/settings')
}

export function fetchMcpServers(): Promise<McpServersResponse> {
  return request.get('/agent/mcps')
}

export function fetchMcpServer(serverId: string): Promise<McpServerResponse> {
  return request.get(`/agent/mcps/${serverId}`)
}

export function updateMcpSettings(enabled: boolean): Promise<McpSettingsApiResponse> {
  return request.post('/agent/mcps/settings', { enabled })
}

export function saveMcpServer(payload: unknown): Promise<McpServerResponse> {
  return request.post('/agent/mcps', payload)
}

export function deleteMcpServer(serverId: string): Promise<unknown> {
  return request.delete(`/agent/mcps/${serverId}`)
}

export function runMcpDraftAction(endpoint: 'test' | 'refresh', payload: unknown): Promise<McpServerResponse> {
  return request.post(`/agent/mcps/draft/${endpoint}`, payload)
}


// -------- draft 工厂 --------
// 纯函数：不依赖组件状态。count 由调用方（composable）传入用于生成默认名。

function normalizePersistedMcpDraftId(value: string | null | undefined): string | null {
  if (!value) return null
  const numeric = Number(value)
  if (!Number.isFinite(numeric) || numeric <= 0) return null
  return `${numeric}`
}

export function createMcpDraft(existingCount = 0, seed = ''): McpDraft {
  return {
    id: null,
    name: seed || `MCP 服务 ${existingCount + 1}`,
    enabled: true,
    transport: 'sse',
    url: '',
    command: '',
    argsText: '',
    authType: 'none',
    bearerScheme: 'Bearer',
    bearerToken: '',
    authUsername: '',
    authPassword: '',
    authHeaderName: 'Authorization',
    authHeaderValue: '',
    authQueryName: 'token',
    authQueryValue: '',
    customHeadersMode: 'replace',
    customHeadersText: '',
    stdioEnvMode: 'replace',
    stdioEnvText: '',
    existingCustomHeaders: [],
    existingStdioEnv: [],
    authSecretFlags: { token: false, password: false, value: false },
    lastStatus: null,
    lastError: null,
    configVersion: 1,
    toolsSnapshot: [],
    resourcesSnapshot: [],
    promptsSnapshot: [],
    lastSyncAt: null,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }
}

export function createMcpDraftFromDetail(detail: McpServerDetail): McpDraft {
  const customHeadersText = detail.customHeaders
    .map((entry) => `${entry.name}: ${entry.value}`)
    .join('\n')
  const stdioEnvText = detail.stdioEnv
    .map((entry) => `${entry.name}=${entry.value}`)
    .join('\n')
  return {
    id: normalizePersistedMcpDraftId(detail.id),
    name: detail.name,
    enabled: detail.enabled,
    transport: detail.transport,
    url: detail.url,
    command: detail.command,
    argsText: detail.args.join('\n'),
    authType: detail.auth?.authType || 'none',
    bearerScheme: detail.auth?.scheme || 'Bearer',
    bearerToken: detail.auth?.tokenValue || '',
    authUsername: detail.auth?.username || '',
    authPassword: detail.auth?.passwordValue || '',
    authHeaderName: detail.auth?.headerName || 'Authorization',
    authHeaderValue: detail.auth?.authType === 'header' ? detail.auth.valueText : '',
    authQueryName: detail.auth?.queryName || 'token',
    authQueryValue: detail.auth?.authType === 'query' ? detail.auth.valueText : '',
    customHeadersMode: 'replace',
    customHeadersText,
    stdioEnvMode: 'replace',
    stdioEnvText,
    existingCustomHeaders: [...detail.customHeaders],
    existingStdioEnv: [...detail.stdioEnv],
    authSecretFlags: {
      token: detail.auth?.tokenHasValue === true,
      password: detail.auth?.passwordHasValue === true,
      value: detail.auth?.valueHasValue === true,
    },
    lastStatus: detail.lastStatus,
    lastError: detail.lastError,
    configVersion: detail.configVersion,
    toolsSnapshot: [...detail.toolsSnapshot],
    resourcesSnapshot: [...detail.resourcesSnapshot],
    promptsSnapshot: [...detail.promptsSnapshot],
    lastSyncAt: detail.lastSyncAt,
    createdAt: detail.createdAt,
    updatedAt: detail.updatedAt,
  }
}

export function normalizePersistedDraftId(value: string | null | undefined): string | null {
  return normalizePersistedMcpDraftId(value)
}
