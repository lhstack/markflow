<script setup lang="ts">
// 供应商表单：新建 / 编辑。字段对齐后端 ProviderUpsertRequest。
//
// 编辑时通过 getProvider 拉取明文 api_key 回填；留空保存表示不更新已存密钥。
// 保存成功后 emit('saved', response)，由父组件用返回的最新列表刷新。
// active 表示当前供应商是否为激活项；「启用」开关打开时 emit('activate')。

import { reactive, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'

import {
  defaultBaseUrl,
  getProvider,
  normalizeOpenAiApi,
  normalizeProviderKind,
  saveProvider,
  type OpenAiApi,
  type ProviderKind,
  type ProviderSummary,
  type ProvidersResponse,
} from '@/api/agentProvider'

const props = defineProps<{
  /** 编辑目标；null 表示新建。 */
  provider: ProviderSummary | null
  /** 当前供应商是否为激活项。 */
  active?: boolean
}>()

const emit = defineEmits<{
  (event: 'saved', response: ProvidersResponse): void
  (event: 'activate'): void
}>()

interface Draft {
  id: number | null
  name: string
  kind: ProviderKind
  api: OpenAiApi
  baseUrl: string
  apiKey: string
  anthropicVersion: string
  baseUrlTouched: boolean
}

const draft = reactive<Draft>(createDraft())
const saving = ref(false)
const loading = ref(false)

function createDraft(): Draft {
  return {
    id: null,
    name: '',
    kind: 'openai',
    api: 'responses',
    baseUrl: defaultBaseUrl('openai'),
    apiKey: '',
    anthropicVersion: '',
    baseUrlTouched: false,
  }
}

async function loadFrom(provider: ProviderSummary | null) {
  if (!provider) {
    Object.assign(draft, createDraft())
    return
  }
  // 先用列表里的摘要回填，再异步拉明文 key。
  Object.assign(draft, {
    id: provider.id,
    name: provider.name,
    kind: normalizeProviderKind(provider.kind),
    api: normalizeOpenAiApi(provider.api),
    baseUrl: provider.base_url,
    apiKey: '',
    anthropicVersion: provider.anthropic_version ?? '',
    baseUrlTouched: true,
  })
  loading.value = true
  try {
    const detail = await getProvider(provider.id)
    draft.apiKey = detail.api_key || ''
    draft.baseUrl = detail.base_url || draft.baseUrl
    draft.anthropicVersion = detail.anthropic_version ?? ''
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '读取供应商详情失败')
  } finally {
    loading.value = false
  }
}

// kind 切换时，若用户没手动改过 base_url，则跟随默认值。
watch(
  () => draft.kind,
  (kind) => {
    if (!draft.baseUrlTouched || !draft.baseUrl.trim()) {
      draft.baseUrl = defaultBaseUrl(kind)
    }
  },
)

watch(
  () => props.provider,
  (provider) => {
    loadFrom(provider)
  },
  { immediate: true },
)

function onActivateChange(next: boolean) {
  // 只处理「打开」：激活当前供应商。关闭无意义（必须始终有一个激活项）。
  if (next) emit('activate')
}

async function submit() {
  const name = draft.name.trim()
  if (!name) {
    ElMessage.warning('请填写供应商名称')
    return
  }
  if (!draft.id && !draft.apiKey.trim()) {
    ElMessage.warning('请填写 API Key')
    return
  }
  saving.value = true
  try {
    const response = await saveProvider({
      id: draft.id,
      name,
      kind: draft.kind,
      api: draft.kind === 'openai' ? draft.api : undefined,
      base_url: draft.baseUrl.trim() || defaultBaseUrl(draft.kind),
      // 编辑时留空表示不更新密钥。
      api_key: draft.apiKey.trim() || undefined,
      anthropic_version:
        draft.kind === 'anthropic' ? draft.anthropicVersion.trim() || null : null,
      remote_models: props.provider?.remote_models || [],
    })
    ElMessage.success('供应商已保存')
    emit('saved', response)
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '保存供应商失败')
  } finally {
    saving.value = false
  }
}

function resetToNew() {
  Object.assign(draft, createDraft())
}

defineExpose({ submit, resetToNew })
</script>

<template>
  <div v-loading="loading" class="provider-form">
    <div class="provider-grid">
      <label class="provider-field">
        <span>名称</span>
        <el-input v-model="draft.name" placeholder="供应商名称，例如 OpenAI 官方" />
      </label>

      <label class="provider-field">
        <span>类型</span>
        <el-select v-model="draft.kind" placeholder="供应商类型">
          <el-option label="OpenAI" value="openai" />
          <el-option label="Anthropic" value="anthropic" />
        </el-select>
      </label>

      <div v-if="draft.id" class="provider-field">
        <span>启用</span>
        <div class="provider-switch">
          <el-switch :model-value="active" :disabled="active" @change="onActivateChange" />
          <span class="provider-switch-hint">{{ active ? '当前激活' : '设为激活' }}</span>
        </div>
      </div>

      <label class="provider-field">
        <span>Base URL</span>
        <el-input
          v-model="draft.baseUrl"
          placeholder="例如 https://api.openai.com/v1"
          @input="draft.baseUrlTouched = true"
        />
      </label>

      <label v-if="draft.kind === 'openai'" class="provider-field">
        <span>默认 API</span>
        <el-select v-model="draft.api" placeholder="OpenAI 传输模式">
          <el-option label="Responses" value="responses" />
          <el-option label="Chat Completions" value="completions" />
        </el-select>
      </label>

      <label v-if="draft.kind === 'anthropic'" class="provider-field">
        <span>Anthropic 版本（可选）</span>
        <el-input v-model="draft.anthropicVersion" placeholder="例如 2023-06-01，留空使用默认" />
      </label>

      <label class="provider-field">
        <span>API Key</span>
        <el-input
          v-model="draft.apiKey"
          type="password"
          show-password
          :placeholder="draft.id ? '留空表示不修改已存密钥' : '请输入 API Key'"
        />
      </label>
    </div>

    <div class="provider-form-actions">
      <el-button type="primary" :loading="saving" @click="submit">保存供应商</el-button>
    </div>
  </div>
</template>

<style scoped>
.provider-form {
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.provider-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px 16px;
}
.provider-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  min-width: 0;
}
.provider-switch {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 32px;
}
.provider-switch-hint {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}
.provider-form-actions {
  display: flex;
  justify-content: flex-end;
}
@media (max-width: 720px) {
  .provider-grid {
    grid-template-columns: 1fr;
  }
}
</style>
