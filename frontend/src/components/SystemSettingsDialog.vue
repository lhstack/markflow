<template>
  <el-dialog v-model="visible" title="系统配置" width="460px" append-to-body destroy-on-close>
    <div class="settings-shell">
      <div v-loading="system.loadingAdmin" class="settings-form">
        <div class="settings-row">
          <div>
            <div class="settings-label">允许新用户注册</div>
            <div class="settings-desc">关闭后，注册页会展示停用状态，后端也会拒绝注册请求。</div>
          </div>
          <el-switch v-model="form.registration_enabled" />
        </div>

        <div class="settings-block">
          <div class="settings-label">上传大小限制</div>
          <div class="settings-desc">限制所有附件上传请求的单文件大小。</div>
          <div class="settings-input-row">
            <el-input-number
              v-model="form.upload_max_mb"
              :min="1"
              :max="1024"
              :step="1"
              controls-position="right"
            />
            <span class="settings-unit">MB</span>
          </div>
        </div>
      </div>
    </div>

    <template #footer>
      <el-button @click="visible = false">取消</el-button>
      <el-button type="primary" :loading="saving" @click="save">保存</el-button>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { useSystemStore } from '@/stores/system'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()

const visible = ref(props.modelValue)
const saving = ref(false)
const system = useSystemStore()
const form = reactive({
  registration_enabled: true,
  upload_max_mb: 20,
})

function syncForm() {
  const source = system.adminSettings || system.publicSettings
  form.registration_enabled = source.registration_enabled
  form.upload_max_mb = source.upload_max_mb
}

async function loadSettings() {
  try {
    await system.fetchAdminSettings()
    syncForm()
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || '读取系统配置失败')
  }
}

async function save() {
  saving.value = true
  try {
    await system.updateAdminSettings({
      registration_enabled: form.registration_enabled,
      upload_max_mb: form.upload_max_mb,
    })
    syncForm()
    ElMessage.success('系统配置已更新')
    visible.value = false
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || '保存系统配置失败')
  } finally {
    saving.value = false
  }
}

watch(() => props.modelValue, (value) => {
  visible.value = value
  if (value) {
    void loadSettings()
  }
})

watch(visible, (value) => {
  emit('update:modelValue', value)
})
</script>

<style scoped>
.settings-shell {
  min-height: 160px;
}

.settings-form {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.settings-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
}

.settings-block {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.settings-input-row {
  display: inline-flex;
  align-items: center;
  gap: 10px;
}

.settings-label {
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
}

.settings-desc {
  font-size: 12px;
  line-height: 1.6;
  color: var(--text2);
  max-width: 320px;
}

.settings-unit {
  font-size: 13px;
  font-weight: 500;
  color: var(--text2);
  line-height: 1;
}
</style>
