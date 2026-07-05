<script setup lang="ts">
// 单个模型的运行参数编辑表单（受控组件）。
//
// 输入 `config`（AgentModelConfig），输出编辑后的 config（v-model:config）。
// 数字字段用文本输入，空串表示「不设置」（缺省沿用上游默认）。
// 字段按 provider 协议显隐：openai 显示传输模式 / reasoning，
// anthropic 显示 thinking budget。

import { computed, reactive, watch } from 'vue'
import { QuestionFilled } from '@element-plus/icons-vue'
import type { AgentModelConfig, OpenAiApi, ProviderKind } from '@/api/agentProvider'
import { normalizeOpenAiApi, pruneModelConfig } from '@/api/agentProvider'

const props = defineProps<{
  config: AgentModelConfig
  kind: ProviderKind
  /** 供应商级 api，用于「沿用供应商」的显示提示。 */
  providerApi: OpenAiApi
}>()

const emit = defineEmits<{
  (event: 'update:config', value: AgentModelConfig): void
}>()

const booleanOptions = [
  { value: '', label: '默认' },
  { value: 'true', label: '开启' },
  { value: 'false', label: '关闭' },
]
const apiOptions = [
  { value: '', label: '沿用供应商' },
  { value: 'responses', label: 'Responses' },
  { value: 'completions', label: 'Chat Completions' },
]
const reasoningEffortOptions = ['', 'minimal', 'low', 'medium', 'high', 'xhigh'].map((value) => ({
  value,
  label: value || '默认',
}))
const reasoningSummaryOptions = ['', 'auto', 'concise', 'detailed'].map((value) => ({
  value,
  label: value || '默认',
}))
const modalityOptions = [
  { value: 'text', label: '文本' },
  { value: 'image', label: '图片' },
  { value: 'audio', label: '音频' },
  { value: 'video', label: '视频' },
  { value: 'file', label: '文件' },
]

const help: Record<string, string> = {
  api: '模型级传输模式覆盖。留空则沿用供应商配置。仅 OpenAI 协议生效。',
  tools_enabled: '是否允许该模型调用工具（前端工具与 MCP）。缺省开启。',
  temperature: '采样温度，留空使用上游默认。',
  top_p: '核采样阈值，留空使用上游默认。',
  top_k: 'Top-K 采样，留空使用上游默认。仅部分协议生效。',
  max_tokens: '单次回复最大 token 数，留空使用上游默认。',
  context_window: '模型上下文窗口大小，用于历史裁剪估算，留空使用默认。',
  reasoning_enabled: '是否开启推理 / 思考。',
  reasoning_effort: 'OpenAI reasoning effort 档位。仅在开启推理时生效。',
  reasoning_summary: 'OpenAI reasoning summary 模式。仅在开启推理时生效。',
  thinking_budget_tokens: 'Anthropic thinking 预算 token 数。仅在开启推理时生效。',
  modalities: '声明该模型可接收的输入模态。',
  stop_sequences: '停止序列，每行一个，留空表示不设置。',
  additional_params: '高级透传参数（JSON 对象），原样并入上游请求。',
}

// 表单草稿：数字字段用文本，避免 0 / 空串歧义。
interface Draft {
  api: '' | OpenAiApi
  tools_enabled: '' | 'true' | 'false'
  temperatureText: string
  topPText: string
  topKText: string
  maxTokensText: string
  contextWindowText: string
  reasoning_enabled: '' | 'true' | 'false'
  reasoningEffort: string
  reasoningSummary: string
  thinkingBudgetText: string
  modalities: string[]
  stopSequencesText: string
  additionalParamsText: string
}

const draft = reactive<Draft>(fromConfig(props.config))

const isOpenAi = computed(() => props.kind === 'openai')
const isAnthropic = computed(() => props.kind === 'anthropic')
const reasoningOn = computed(() => draft.reasoning_enabled === 'true')

function fromConfig(config: AgentModelConfig): Draft {
  const numText = (value: number | null | undefined) =>
    value === null || value === undefined ? '' : String(value)
  const boolText = (value: boolean | null | undefined): '' | 'true' | 'false' =>
    value === null || value === undefined ? '' : value ? 'true' : 'false'
  return {
    api: config.api ?? '',
    tools_enabled: boolText(config.tools_enabled),
    temperatureText: numText(config.temperature),
    topPText: numText(config.top_p),
    topKText: numText(config.top_k),
    maxTokensText: numText(config.max_tokens),
    contextWindowText: numText(config.context_window),
    reasoning_enabled: boolText(config.reasoning_enabled),
    reasoningEffort: config.reasoning_effort ?? '',
    reasoningSummary: config.reasoning_summary ?? '',
    thinkingBudgetText: numText(config.thinking_budget_tokens),
    modalities: Array.isArray(config.modalities) ? [...config.modalities] : [],
    stopSequencesText: Array.isArray(config.stop_sequences) ? config.stop_sequences.join('\n') : '',
    additionalParamsText: config.additional_params
      ? JSON.stringify(config.additional_params, null, 2)
      : '',
  }
}

function parseNumber(text: string): number | undefined {
  const trimmed = text.trim()
  if (!trimmed) return undefined
  const value = Number(trimmed)
  return Number.isFinite(value) ? value : undefined
}

function parseBool(value: '' | 'true' | 'false'): boolean | undefined {
  if (value === 'true') return true
  if (value === 'false') return false
  return undefined
}

function toConfig(): AgentModelConfig {
  const config: AgentModelConfig = {}
  if (draft.api) config.api = normalizeOpenAiApi(draft.api)
  const toolsEnabled = parseBool(draft.tools_enabled)
  if (toolsEnabled !== undefined) config.tools_enabled = toolsEnabled
  const temperature = parseNumber(draft.temperatureText)
  if (temperature !== undefined) config.temperature = temperature
  const topP = parseNumber(draft.topPText)
  if (topP !== undefined) config.top_p = topP
  const topK = parseNumber(draft.topKText)
  if (topK !== undefined) config.top_k = topK
  const maxTokens = parseNumber(draft.maxTokensText)
  if (maxTokens !== undefined) config.max_tokens = maxTokens
  const contextWindow = parseNumber(draft.contextWindowText)
  if (contextWindow !== undefined) config.context_window = contextWindow
  const reasoningEnabled = parseBool(draft.reasoning_enabled)
  if (reasoningEnabled !== undefined) config.reasoning_enabled = reasoningEnabled
  if (draft.reasoningEffort.trim()) config.reasoning_effort = draft.reasoningEffort.trim()
  if (draft.reasoningSummary.trim()) config.reasoning_summary = draft.reasoningSummary.trim()
  const thinkingBudget = parseNumber(draft.thinkingBudgetText)
  if (thinkingBudget !== undefined) config.thinking_budget_tokens = thinkingBudget
  if (draft.modalities.length) config.modalities = [...draft.modalities]
  const stops = draft.stopSequencesText
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
  if (stops.length) config.stop_sequences = stops
  const additionalText = draft.additionalParamsText.trim()
  if (additionalText) {
    try {
      const parsed = JSON.parse(additionalText)
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        config.additional_params = parsed as Record<string, unknown>
      }
    } catch {
      // 无效 JSON 时忽略，交由父组件保存时统一校验提示。
    }
  }
  return config
}

// 值相等守卫：fromConfig/toConfig 每次都产生新对象，若仅靠引用比较，
// props.config -> draft -> emit -> props.config 会形成无限回环导致内存溢出。
// 用序列化后的值判等，值稳定后两个 watch 都不再触发副作用，回环自然断开。
function configKey(config: AgentModelConfig): string {
  return JSON.stringify(pruneModelConfig(config))
}

// 父组件切换编辑目标时，用新 config 重置草稿（仅当外部值与当前草稿不一致）。
watch(
  () => props.config,
  (next) => {
    if (configKey(next) === configKey(toConfig())) return
    Object.assign(draft, fromConfig(next))
  },
)

// 草稿变化即回写（仅当草稿值与外部 config 不一致）。
watch(
  draft,
  () => {
    if (configKey(toConfig()) === configKey(props.config)) return
    emit('update:config', toConfig())
  },
  { deep: true },
)

function toggleModality(value: string, checked: boolean) {
  if (checked) {
    if (!draft.modalities.includes(value)) draft.modalities = [...draft.modalities, value]
  } else {
    draft.modalities = draft.modalities.filter((item) => item !== value)
  }
}

/** 供父组件保存前校验 additional_params JSON 是否合法。 */
function additionalParamsError(): string | null {
  const text = draft.additionalParamsText.trim()
  if (!text) return null
  try {
    const parsed = JSON.parse(text)
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return '高级附加参数必须是 JSON 对象'
    }
    return null
  } catch {
    return '高级附加参数不是合法 JSON'
  }
}

defineExpose({ additionalParamsError })
</script>

<template>
  <div class="model-config-grid">
    <label v-if="isOpenAi" class="model-config-field">
      <span class="model-config-label">
        传输模式
        <el-tooltip :content="help.api" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-select v-model="draft.api">
        <el-option v-for="option in apiOptions" :key="`api-${option.value}`" :label="option.label" :value="option.value" />
      </el-select>
    </label>

    <label class="model-config-field">
      <span class="model-config-label">
        工具调用
        <el-tooltip :content="help.tools_enabled" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-select v-model="draft.tools_enabled">
        <el-option v-for="option in booleanOptions" :key="`tools-${option.value}`" :label="option.label" :value="option.value" />
      </el-select>
    </label>

    <label class="model-config-field">
      <span class="model-config-label">
        Temperature
        <el-tooltip :content="help.temperature" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-input v-model="draft.temperatureText" placeholder="留空表示默认，例如 0.7" />
    </label>

    <label class="model-config-field">
      <span class="model-config-label">
        Max Tokens
        <el-tooltip :content="help.max_tokens" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-input v-model="draft.maxTokensText" placeholder="留空表示默认，例如 4096" />
    </label>

    <label class="model-config-field">
      <span class="model-config-label">
        Context Window
        <el-tooltip :content="help.context_window" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-input v-model="draft.contextWindowText" placeholder="留空表示默认" />
    </label>

    <label class="model-config-field">
      <span class="model-config-label">
        Top P
        <el-tooltip :content="help.top_p" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-input v-model="draft.topPText" placeholder="留空表示默认" />
    </label>

    <label class="model-config-field">
      <span class="model-config-label">
        Top K
        <el-tooltip :content="help.top_k" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-input v-model="draft.topKText" placeholder="留空表示默认" />
    </label>

    <label class="model-config-field">
      <span class="model-config-label">
        推理 / 思考
        <el-tooltip :content="help.reasoning_enabled" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-select v-model="draft.reasoning_enabled">
        <el-option v-for="option in booleanOptions" :key="`reasoning-${option.value}`" :label="option.label" :value="option.value" />
      </el-select>
    </label>

    <label v-if="isOpenAi && reasoningOn" class="model-config-field">
      <span class="model-config-label">
        Reasoning Effort
        <el-tooltip :content="help.reasoning_effort" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-select v-model="draft.reasoningEffort">
        <el-option v-for="option in reasoningEffortOptions" :key="`effort-${option.value || 'unset'}`" :label="option.label" :value="option.value" />
      </el-select>
    </label>

    <label v-if="isOpenAi && reasoningOn" class="model-config-field">
      <span class="model-config-label">
        Reasoning Summary
        <el-tooltip :content="help.reasoning_summary" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-select v-model="draft.reasoningSummary">
        <el-option v-for="option in reasoningSummaryOptions" :key="`summary-${option.value || 'unset'}`" :label="option.label" :value="option.value" />
      </el-select>
    </label>

    <label v-if="isAnthropic && reasoningOn" class="model-config-field">
      <span class="model-config-label">
        Thinking Budget
        <el-tooltip :content="help.thinking_budget_tokens" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-input v-model="draft.thinkingBudgetText" placeholder="留空表示默认，例如 1024" />
    </label>

    <label class="model-config-field model-config-field-full">
      <span class="model-config-label">
        输入模态
        <el-tooltip :content="help.modalities" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <div class="model-modality-row">
        <label v-for="modality in modalityOptions" :key="modality.value" class="model-modality-item">
          <input
            type="checkbox"
            :checked="draft.modalities.includes(modality.value)"
            @change="toggleModality(modality.value, ($event.target as HTMLInputElement).checked)"
          />
          <span>{{ modality.label }}</span>
        </label>
      </div>
    </label>

    <label class="model-config-field model-config-field-full">
      <span class="model-config-label">
        Stop Sequences
        <el-tooltip :content="help.stop_sequences" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-input
        v-model="draft.stopSequencesText"
        type="textarea"
        :rows="3"
        placeholder="每行一个停止词，留空表示不设置"
      />
    </label>

    <label class="model-config-field model-config-field-full">
      <span class="model-config-label">
        高级附加参数
        <el-tooltip :content="help.additional_params" placement="top">
          <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
        </el-tooltip>
      </span>
      <el-input
        v-model="draft.additionalParamsText"
        type="textarea"
        :rows="6"
        placeholder='请输入 JSON 对象，例如 { "top_logprobs": 3 }'
      />
    </label>
  </div>
</template>

<style scoped>
.model-config-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px 18px;
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
  gap: 4px;
  font-size: 13px;
  color: var(--el-text-color-regular);
}
.param-help-icon {
  font-size: 13px;
  color: var(--el-text-color-placeholder);
  cursor: help;
}
.model-modality-row {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
}
.model-modality-item {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 13px;
  cursor: pointer;
}
</style>
