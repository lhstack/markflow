// 供应商 + 模型管理的 API 层。
//
// 严格对齐后端 `routes/agent/provider.rs` 与 `routes/agent/model_http.rs` 的 DTO。
// 只支持 openai / anthropic 两种协议。模型的全部运行参数收敛进单个 `config` 对象。
//
// 约定：所有请求都经 `@/utils/request`（axios 实例，拦截器已解包 response.data
// 并处理 401/403），因此这里拿到的就是响应体本身。

import request from '@/utils/request'

// --------- 基础枚举 ---------

export type ProviderKind = 'openai' | 'anthropic'
export type OpenAiApi = 'responses' | 'completions'

export function normalizeProviderKind(value?: string | null): ProviderKind {
  const v = (value ?? '').trim().toLowerCase()
  return v === 'anthropic' || v === 'claude' ? 'anthropic' : 'openai'
}

export function normalizeOpenAiApi(value?: string | null): OpenAiApi {
  return (value ?? '').trim().toLowerCase() === 'responses' ? 'responses' : 'completions'
}

export function defaultBaseUrl(kind: ProviderKind): string {
  return kind === 'anthropic' ? 'https://api.anthropic.com' : 'https://api.openai.com/v1'
}

/** 展示用途的 base_url 归一化：去掉尾部斜杠；空值返回空串。 */
export function normalizedBaseUrl(value?: string | null): string {
  const trimmed = (value ?? '').trim()
  return trimmed ? trimmed.replace(/\/+$/, '') : ''
}

// --------- 模型运行参数（对齐后端 AgentModelConfig）---------
//
// 全部字段可选，缺省即沿用上游默认。运行时后端按协议各取所需拼装 additional_params。

export interface AgentModelConfig {
  /** 模型级 OpenAI 传输模式覆盖；不填则沿用供应商级 api。 */
  api?: OpenAiApi | null
  /** 是否流式，缺省 true。 */
  stream?: boolean | null
  /** 是否允许工具调用，缺省 true。 */
  tools_enabled?: boolean | null
  temperature?: number | null
  top_p?: number | null
  top_k?: number | null
  max_tokens?: number | null
  context_window?: number | null
  /** 是否开启推理/思考。 */
  reasoning_enabled?: boolean | null
  /** OpenAI reasoning effort：minimal|low|medium|high。 */
  reasoning_effort?: string | null
  /** OpenAI reasoning summary：auto|concise|detailed。 */
  reasoning_summary?: string | null
  /** Anthropic thinking budget tokens。 */
  thinking_budget_tokens?: number | null
  /** 停止序列。 */
  stop_sequences?: string[]
  /** 输入模态提示。 */
  modalities?: string[]
  /** 高级透传参数，原样并入 additional_params。 */
  additional_params?: Record<string, unknown> | null
}

// --------- 服务端响应类型 ---------

export interface ModelSummary {
  id: number
  provider_id: number
  alias: string
  model_id: string
  display_name: string | null
  config: AgentModelConfig
  created_at: string
  updated_at: string
}

export interface ProviderSummary {
  id: number
  name: string
  kind: string
  api: string
  base_url: string
  anthropic_version: string | null
  remote_models: string[]
  models: ModelSummary[]
  is_active: boolean
  has_api_key: boolean
  created_at: string
  updated_at: string
}

export interface ProvidersResponse {
  providers: ProviderSummary[]
  active_provider_id: number | null
}

export interface ProviderDetailResponse {
  id: number
  name: string
  kind: string
  api: string
  base_url: string
  anthropic_version: string | null
  api_key: string
  remote_models: string[]
  models: ModelSummary[]
  is_active: boolean
}

export interface RemoteModel {
  id: string
  owned_by: string
  created: number
}

// --------- 请求体类型（对齐后端 Upsert 请求）---------

export interface ProviderUpsertPayload {
  id?: number | null
  name: string
  kind: ProviderKind
  /** openai 传输模式；anthropic 忽略。 */
  api?: OpenAiApi
  base_url?: string
  /** 留空表示不更新已存密钥（编辑场景）。新建时必填。 */
  api_key?: string
  anthropic_version?: string | null
  remote_models?: string[]
}

export interface ModelUpsertPayload {
  id?: number | null
  provider_id: number
  alias: string
  model_id: string
  display_name?: string | null
  config: AgentModelConfig
}

// --------- API 请求函数 ---------

/** 拉取当前用户的全部供应商（含各自已保存模型）。 */
export function listProviders(): Promise<ProvidersResponse> {
  return request.get('/agent/providers')
}

/** 读取单个供应商详情（含明文 api_key，用于编辑回填）。 */
export function getProvider(providerId: number): Promise<ProviderDetailResponse> {
  return request.get(`/agent/providers/${providerId}`)
}

/** 新建 / 更新供应商。返回刷新后的完整列表。 */
export function saveProvider(payload: ProviderUpsertPayload): Promise<ProvidersResponse> {
  return request.post('/agent/providers', payload)
}

/** 设为激活供应商。 */
export function activateProvider(providerId: number): Promise<ProvidersResponse> {
  return request.post(`/agent/providers/${providerId}/activate`)
}

/** 删除供应商（级联删除其模型）。 */
export function deleteProvider(providerId: number): Promise<ProvidersResponse> {
  return request.delete(`/agent/providers/${providerId}`)
}

/** 拉取指定供应商的远程可用模型清单（供选择后保存）。 */
export function fetchRemoteModels(providerId: number): Promise<{ models: RemoteModel[] }> {
  return request.post('/agent/models', { provider_id: providerId })
}

/** 新建 / 更新一条已保存模型。返回刷新后的完整列表。 */
export function saveModel(payload: ModelUpsertPayload): Promise<ProvidersResponse> {
  return request.post('/agent/models/save', payload)
}

/** 删除一条已保存模型。 */
export function deleteModel(providerId: number, modelId: number): Promise<ProvidersResponse> {
  return request.post('/agent/models/delete', { provider_id: providerId, model_id: modelId })
}

// --------- config 空值清理 ---------
//
// 表单里空字符串 / 未填字段统一压成 undefined，避免把 "" 或 0 误传给后端。

export function pruneModelConfig(config: AgentModelConfig): AgentModelConfig {
  const pruned: AgentModelConfig = {}
  const assignIf = <K extends keyof AgentModelConfig>(key: K, value: AgentModelConfig[K] | null | undefined) => {
    if (value !== null && value !== undefined) {
      pruned[key] = value
    }
  }
  assignIf('api', config.api ?? undefined)
  assignIf('stream', config.stream ?? undefined)
  assignIf('tools_enabled', config.tools_enabled ?? undefined)
  assignIf('temperature', config.temperature ?? undefined)
  assignIf('top_p', config.top_p ?? undefined)
  assignIf('top_k', config.top_k ?? undefined)
  assignIf('max_tokens', config.max_tokens ?? undefined)
  assignIf('context_window', config.context_window ?? undefined)
  assignIf('reasoning_enabled', config.reasoning_enabled ?? undefined)
  assignIf('reasoning_effort', config.reasoning_effort ?? undefined)
  assignIf('reasoning_summary', config.reasoning_summary ?? undefined)
  assignIf('thinking_budget_tokens', config.thinking_budget_tokens ?? undefined)
  if (config.stop_sequences && config.stop_sequences.length) {
    pruned.stop_sequences = config.stop_sequences
  }
  if (config.modalities && config.modalities.length) {
    pruned.modalities = config.modalities
  }
  if (config.additional_params && Object.keys(config.additional_params).length) {
    pruned.additional_params = config.additional_params
  }
  return pruned
}
