<script setup lang="ts">
// 某供应商下的模型管理：已保存模型列表（编辑 / 删除）+ 拉取远程模型选择保存。
//
// 保存 / 删除后 emit('changed', response)，父组件直接采用后端返回的最新列表。

import { computed, reactive, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus, Refresh } from '@element-plus/icons-vue'

import {
  deleteModel,
  fetchRemoteModels,
  normalizeOpenAiApi,
  normalizeProviderKind,
  pruneModelConfig,
  saveModel,
  type AgentModelConfig,
  type ModelSummary,
  type ProviderSummary,
  type ProvidersResponse,
  type RemoteModel,
} from '@/api/agentProvider'
import ModelConfigForm from './ModelConfigForm.vue'

const props = defineProps<{
  provider: ProviderSummary
}>()

const emit = defineEmits<{
  (event: 'changed', response: ProvidersResponse): void
}>()

const kind = computed(() => normalizeProviderKind(props.provider.kind))
const providerApi = computed(() => normalizeOpenAiApi(props.provider.api))

// 远程模型拉取
const remoteModels = ref<RemoteModel[]>([])
const remoteLoading = ref(false)
const remoteQuery = ref('')

const filteredRemoteModels = computed(() => {
  const query = remoteQuery.value.trim().toLowerCase()
  const savedIds = new Set(props.provider.models.map((model) => model.model_id))
  return remoteModels.value.filter((model) => {
    if (savedIds.has(model.id)) return false
    if (!query) return true
    return model.id.toLowerCase().includes(query)
  })
})

async function pullRemoteModels() {
  if (!props.provider.has_api_key) {
    ElMessage.warning('当前供应商缺少 API Key')
    return
  }
  remoteLoading.value = true
  try {
    const data = await fetchRemoteModels(props.provider.id)
    remoteModels.value = [...(data.models || [])].sort((a, b) => a.id.localeCompare(b.id))
    ElMessage.success(`已拉取 ${remoteModels.value.length} 个远程模型`)
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '拉取模型失败')
  } finally {
    remoteLoading.value = false
  }
}

// 模型编辑草稿
interface ModelDraft {
  id: number | null
  alias: string
  model_id: string
  display_name: string
  config: AgentModelConfig
}

const editing = ref(false)
const saving = ref(false)
const draft = reactive<ModelDraft>(createDraft())
const configFormRef = ref<InstanceType<typeof ModelConfigForm> | null>(null)

function createDraft(): ModelDraft {
  return { id: null, alias: '', model_id: '', display_name: '', config: {} }
}

/** 从远程模型开一个新草稿。 */
function startFromRemote(model: RemoteModel) {
  Object.assign(draft, {
    id: null,
    alias: model.id,
    model_id: model.id,
    display_name: '',
    config: {},
  })
  editing.value = true
}

/** 手动新建（不依赖远程列表）。 */
function startBlank() {
  Object.assign(draft, createDraft())
  editing.value = true
}

/** 编辑已保存模型。 */
function startEdit(model: ModelSummary) {
  Object.assign(draft, {
    id: model.id,
    alias: model.alias,
    model_id: model.model_id,
    display_name: model.display_name ?? '',
    config: { ...model.config },
  })
  editing.value = true
}

function cancelEdit() {
  editing.value = false
  Object.assign(draft, createDraft())
}

function onConfigUpdate(config: AgentModelConfig) {
  draft.config = config
}

async function submit() {
  const alias = draft.alias.trim()
  const modelId = draft.model_id.trim()
  if (!alias || !modelId) {
    ElMessage.warning('请填写模型别名与模型 ID')
    return
  }
  const configError = configFormRef.value?.additionalParamsError()
  if (configError) {
    ElMessage.error(configError)
    return
  }
  saving.value = true
  try {
    const response = await saveModel({
      id: draft.id,
      provider_id: props.provider.id,
      alias,
      model_id: modelId,
      display_name: draft.display_name.trim() || null,
      config: pruneModelConfig(draft.config),
    })
    ElMessage.success('模型已保存')
    editing.value = false
    Object.assign(draft, createDraft())
    emit('changed', response)
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '保存模型失败')
  } finally {
    saving.value = false
  }
}

async function remove(model: ModelSummary) {
  try {
    await ElMessageBox.confirm(`确认删除模型「${model.alias}」？`, '删除确认', {
      type: 'warning',
      confirmButtonText: '删除',
      cancelButtonText: '取消',
    })
  } catch {
    return
  }
  try {
    const response = await deleteModel(props.provider.id, model.id)
    ElMessage.success('模型已删除')
    emit('changed', response)
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '删除模型失败')
  }
}

// 供应商切换时收起编辑态、清空远程列表
watch(
  () => props.provider.id,
  () => {
    cancelEdit()
    remoteModels.value = []
    remoteQuery.value = ''
  },
)
</script>

<template>
  <div class="model-list">
    <!-- 上：模型列表两栏（已添加 | 供应商远程） -->
    <div class="model-columns">
      <section class="model-col">
        <div class="model-col-head">
          <span class="model-col-title">已添加模型（{{ provider.models.length }}）</span>
          <el-button size="small" :icon="Plus" @click="startBlank">新增模型</el-button>
        </div>
        <div v-if="!provider.models.length" class="model-col-empty">
          还没有保存模型。可从右侧远程列表选用，或点「新增模型」。
        </div>
        <ul v-else class="model-col-list">
          <li
            v-for="model in provider.models"
            :key="model.id"
            class="model-card"
            :class="{ active: editing && draft.id === model.id }"
            @click="startEdit(model)"
          >
            <div class="model-card-main">
              <span class="model-card-alias">{{ model.alias }}</span>
              <span class="model-card-id">{{ model.model_id }}</span>
            </div>
            <button type="button" class="model-card-del" @click.stop="remove(model)">删除</button>
          </li>
        </ul>
      </section>

      <section class="model-col">
        <div class="model-col-head">
          <span class="model-col-title">供应商模型</span>
          <div class="model-col-head-actions">
            <el-input v-model="remoteQuery" size="small" clearable placeholder="搜索" class="model-remote-search" />
            <el-button size="small" :icon="Refresh" :loading="remoteLoading" @click="pullRemoteModels">刷新</el-button>
          </div>
        </div>
        <div v-if="!remoteModels.length" class="model-col-empty">
          点「刷新」从上游拉取可用模型清单。
        </div>
        <div v-else-if="!filteredRemoteModels.length" class="model-col-empty">
          没有匹配的未保存模型。
        </div>
        <ul v-else class="model-col-list">
          <li
            v-for="model in filteredRemoteModels"
            :key="model.id"
            class="model-card"
            @click="startFromRemote(model)"
          >
            <div class="model-card-main">
              <span class="model-card-alias">{{ model.id }}</span>
              <span class="model-card-id">{{ model.id }}</span>
            </div>
            <span class="model-card-pick">选用</span>
          </li>
        </ul>
      </section>
    </div>

    <!-- 下：模型参数编辑 -->
    <section v-if="editing" class="model-edit">
      <div class="model-edit-head">
        <span class="model-edit-title">模型参数</span>
        <el-button size="small" text @click="cancelEdit">取消</el-button>
      </div>
      <div class="model-edit-fields">
        <label class="model-edit-field">
          <span>供应商</span>
          <el-input :model-value="provider.name" disabled />
        </label>
        <label class="model-edit-field">
          <span>别名</span>
          <el-input v-model="draft.alias" placeholder="展示用别名，例如 GPT-5.4" />
        </label>
        <label class="model-edit-field">
          <span>模型 ID</span>
          <el-input v-model="draft.model_id" placeholder="下发给上游的模型名，例如 gpt-5.4" />
        </label>
        <label class="model-edit-field">
          <span>显示名（可选）</span>
          <el-input v-model="draft.display_name" placeholder="留空则使用别名" />
        </label>
      </div>

      <div class="model-edit-config">
        <ModelConfigForm
          ref="configFormRef"
          :config="draft.config"
          :kind="kind"
          :provider-api="providerApi"
          @update:config="onConfigUpdate"
        />
      </div>

      <div class="model-edit-actions">
        <el-button @click="cancelEdit">取消</el-button>
        <el-button type="primary" :loading="saving" @click="submit">保存模型</el-button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.model-list {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

/* 两栏模型列表 */
.model-columns {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
}
.model-col {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}
.model-col-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.model-col-head-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}
.model-remote-search {
  width: 130px;
}
.model-col-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}
.model-col-empty {
  padding: 14px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  background: var(--el-fill-color-light);
  border-radius: 8px;
}
.model-col-list {
  list-style: none;
  margin: 0;
  padding: 2px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  height: 168px;
  overflow-y: auto;
  scrollbar-width: thin;
}
.model-col-list::-webkit-scrollbar {
  width: 8px;
}
.model-col-list::-webkit-scrollbar-thumb {
  background: var(--el-border-color);
  border-radius: 999px;
}
.model-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 7px 10px;
  border: 1px solid var(--el-border-color);
  border-radius: 8px;
  cursor: pointer;
  transition: border-color 0.15s ease, background 0.15s ease;
}
.model-card:hover {
  border-color: var(--el-color-primary-light-5);
}
.model-card.active {
  border-color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
}
.model-card-main {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.model-card-alias {
  font-size: 13px;
  font-weight: 600;
  color: var(--el-text-color-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.model-card-id {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  font-family: var(--el-font-family-mono, monospace);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.model-card-del {
  flex-shrink: 0;
  border: none;
  background: transparent;
  color: var(--el-color-danger);
  font-size: 12px;
  cursor: pointer;
  padding: 0;
}
.model-card-pick {
  flex-shrink: 0;
  font-size: 12px;
  color: var(--el-color-primary);
}

/* 模型参数编辑 */
.model-edit {
  display: flex;
  flex-direction: column;
  gap: 14px;
  border-top: 1px solid var(--el-border-color-lighter);
  padding-top: 16px;
}
.model-edit-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.model-edit-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}
.model-edit-fields {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 14px 16px;
}
.model-edit-field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
}
.model-edit-config {
  margin-top: 4px;
}
.model-edit-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
