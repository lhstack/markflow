<script setup lang="ts">
// 供应商 + 模型管理弹窗（组合壳）。
//
// 左侧：供应商列表（选中 / 激活 / 删除 / 新建）。
// 右侧：上半 ProviderForm（当前选中供应商的表单），下半 ModelList（该供应商的模型管理）。
//
// 内部自持 providers 状态；任何变更（保存/激活/删除/模型增删）都会 emit('changed', response)，
// 由父组件（AgentPanel）据此同步会话的激活供应商与可用模型。

import { computed, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus } from '@element-plus/icons-vue'

import {
  activateProvider,
  deleteProvider,
  listProviders,
  type ProviderSummary,
  type ProvidersResponse,
} from '@/api/agentProvider'
import ProviderForm from './ProviderForm.vue'
import ModelList from './ModelList.vue'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void
  (event: 'changed', response: ProvidersResponse): void
}>()

const visible = computed({
  get: () => props.modelValue,
  set: (value) => emit('update:modelValue', value),
})

const providers = ref<ProviderSummary[]>([])
const activeProviderId = ref<number | null>(null)
const selectedId = ref<number | null>(null)
const loading = ref(false)
const creatingNew = ref(false)
const activeSections = ref<string[]>(['provider', 'model'])

const selectedProvider = computed<ProviderSummary | null>(() => {
  if (creatingNew.value) return null
  return providers.value.find((provider) => provider.id === selectedId.value) || null
})

function applyResponse(response: ProvidersResponse) {
  providers.value = response.providers || []
  activeProviderId.value = response.active_provider_id ?? null
  // 维持选中态：优先保留当前选中，否则回落到激活或第一个。
  if (!creatingNew.value) {
    const stillExists = providers.value.some((provider) => provider.id === selectedId.value)
    if (!stillExists) {
      selectedId.value = activeProviderId.value ?? providers.value[0]?.id ?? null
    }
  }
  emit('changed', response)
}

async function reload() {
  loading.value = true
  try {
    const response = await listProviders()
    applyResponse(response)
    if (selectedId.value === null && !creatingNew.value) {
      selectedId.value = activeProviderId.value ?? providers.value[0]?.id ?? null
    }
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '加载供应商失败')
  } finally {
    loading.value = false
  }
}

watch(
  visible,
  (open) => {
    if (open) reload()
  },
  { immediate: true },
)

function selectProvider(id: number) {
  creatingNew.value = false
  selectedId.value = id
}

function startCreate() {
  creatingNew.value = true
  selectedId.value = null
}

function onProviderSaved(response: ProvidersResponse) {
  // 新建成功后，选中新列表里第一个不在旧集合中的（或激活项）。
  const previousIds = new Set(providers.value.map((provider) => provider.id))
  applyResponse(response)
  if (creatingNew.value) {
    const created = response.providers.find((provider) => !previousIds.has(provider.id))
    creatingNew.value = false
    selectedId.value = created?.id ?? response.active_provider_id ?? response.providers[0]?.id ?? null
  }
}

async function activate(id: number) {
  try {
    applyResponse(await activateProvider(id))
    ElMessage.success('已切换激活供应商')
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '切换供应商失败')
  }
}

async function remove(provider: ProviderSummary) {
  try {
    await ElMessageBox.confirm(`确认删除供应商「${provider.name}」及其全部模型？`, '删除确认', {
      type: 'warning',
    })
  } catch {
    return
  }
  try {
    const response = await deleteProvider(provider.id)
    if (selectedId.value === provider.id) {
      selectedId.value = response.active_provider_id ?? response.providers[0]?.id ?? null
    }
    applyResponse(response)
    ElMessage.success('供应商已删除')
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || error.message || '删除供应商失败')
  }
}
</script>

<template>
  <el-dialog
    v-model="visible"
    class="agent-dialog provider-model-dialog"
    title="供应商与模型管理"
    width="1080px"
    align-center
    append-to-body
    destroy-on-close
  >
    <div v-loading="loading" class="pm-layout">
      <aside class="pm-sidebar">
        <div class="pm-sidebar-head">
          <span>供应商</span>
          <el-button size="small" :icon="Plus" @click="startCreate">新增</el-button>
        </div>
        <div class="pm-provider-list">
          <button
            v-for="provider in providers"
            :key="provider.id"
            class="pm-provider-item"
            :class="{ active: provider.id === selectedId && !creatingNew }"
            @click="selectProvider(provider.id)"
          >
            <div class="pm-provider-name">
              {{ provider.name }}
              <el-tag v-if="provider.id === activeProviderId" size="small" type="success">激活</el-tag>
            </div>
            <div class="pm-provider-meta">{{ provider.kind }} · {{ provider.models.length }} 个模型</div>
            <div class="pm-provider-actions">
              <el-button size="small" text type="danger" @click.stop="remove(provider)">删除</el-button>
            </div>
          </button>
          <div v-if="!providers.length" class="pm-empty">还没有供应商，点右上角新增。</div>
        </div>
      </aside>

      <div class="pm-main">
        <el-collapse v-model="activeSections" class="pm-collapse">
          <el-collapse-item name="provider" class="pm-collapse-item">
            <template #title>
              <span class="pm-collapse-title">{{ creatingNew ? '新建供应商' : '供应商配置' }}</span>
            </template>
            <ProviderForm
              :provider="selectedProvider"
              :active="!creatingNew && selectedProvider?.id === activeProviderId"
              @saved="onProviderSaved"
              @activate="selectedProvider && activate(selectedProvider.id)"
            />
          </el-collapse-item>

          <el-collapse-item v-if="selectedProvider" name="model" class="pm-collapse-item">
            <template #title>
              <span class="pm-collapse-title">模型管理</span>
            </template>
            <ModelList :provider="selectedProvider" @changed="applyResponse" />
          </el-collapse-item>
        </el-collapse>
      </div>
    </div>
  </el-dialog>
</template>

<style scoped>
.pm-layout {
  display: flex;
  gap: 16px;
  height: 70vh;
  max-height: 640px;
}
.pm-sidebar {
  width: 240px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  border-right: 1px solid var(--el-border-color-lighter);
  padding-right: 12px;
}
.pm-sidebar-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-weight: 600;
  flex-shrink: 0;
}
.pm-provider-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  overflow-y: auto;
  flex: 1;
}
.pm-provider-item {
  text-align: left;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  padding: 8px 10px;
  background: var(--el-bg-color);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.pm-provider-item.active {
  border-color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
}
.pm-provider-name {
  display: flex;
  align-items: center;
  gap: 6px;
  font-weight: 500;
}
.pm-provider-meta {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}
.pm-provider-actions {
  display: flex;
  gap: 4px;
}
.pm-main {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding-right: 4px;
}
.pm-collapse {
  border-top: none;
  border-bottom: none;
}
.pm-collapse-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}
.pm-empty {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  padding: 12px 0;
}
</style>
