// 供应商 / 模型状态 composable。
//
// 从 AgentPanel 抽出：拥有 providers 列表、激活供应商，以及聊天运行时
// 需要的派生接口（可选模型、模型 config、模态）。数据来自新的两表后端
// （见 @/api/agentProvider），模型从 provider.models 派生，不再有旧的
// enabledModels / customModels / modelConfigs 概念。

import { computed, ref } from 'vue'

import {
  activateProvider as apiActivateProvider,
  listProviders,
  normalizedBaseUrl,
  type AgentModelConfig,
  type ModelSummary,
  type ProviderSummary,
  type ProvidersResponse,
} from '@/api/agentProvider'

export function useAgentProviders() {
  const providers = ref<ProviderSummary[]>([])
  const activeProviderId = ref<number | null>(null)

  const activeProvider = computed(
    () => providers.value.find((provider) => provider.id === activeProviderId.value) || null,
  )

  /** 采用后端返回的最新 providers 列表 + 激活项。 */
  function applyResponse(response: ProvidersResponse) {
    providers.value = response.providers || []
    const nextActive = response.active_provider_id
    if (nextActive !== null && providers.value.some((provider) => provider.id === nextActive)) {
      activeProviderId.value = nextActive
    } else if (activeProviderId.value === null || !providers.value.some((p) => p.id === activeProviderId.value)) {
      activeProviderId.value = providers.value[0]?.id ?? null
    }
  }

  async function loadProviders() {
    applyResponse(await listProviders())
  }

  async function activate(providerId: number) {
    applyResponse(await apiActivateProvider(providerId))
  }

  /** 某供应商下可选模型（用 alias 作为会话侧模型标识）。 */
  function modelOptionsForProvider(provider: ProviderSummary | null): string[] {
    return provider ? provider.models.map((model) => model.alias) : []
  }

  /** 在指定供应商下按 alias 定位一条已保存模型。 */
  function findModel(provider: ProviderSummary | null, alias: string): ModelSummary | null {
    if (!provider) return null
    const trimmed = alias.trim()
    return provider.models.find((model) => model.alias === trimmed) || null
  }

  /** 会话默认模型：取该供应商第一条已保存模型的 alias。 */
  function fallbackModel(provider: ProviderSummary | null): string {
    return provider?.models[0]?.alias || ''
  }

  const activeModelOptions = computed(() => modelOptionsForProvider(activeProvider.value))

  return {
    providers,
    activeProviderId,
    activeProvider,
    activeModelOptions,
    applyResponse,
    loadProviders,
    activate,
    modelOptionsForProvider,
    findModel,
    fallbackModel,
    normalizedBaseUrl,
  }
}

export type { AgentModelConfig, ModelSummary, ProviderSummary }
