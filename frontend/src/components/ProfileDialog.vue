<template>
  <el-dialog v-model="visible" title="个人信息" width="440px" append-to-body @close="emit('close')">
    <div class="profile-dialog">
      <div class="avatar-section">
        <div class="avatar-wrap" @click="triggerFileInput">
          <img v-if="avatarPreview || auth.user?.avatar" :src="avatarPreview || auth.user?.avatar" class="avatar-img" />
          <div v-else class="avatar-placeholder">
            <el-icon><UserFilled /></el-icon>
          </div>
          <div class="avatar-overlay"><el-icon><Camera /></el-icon></div>
        </div>
        <input ref="fileInput" type="file" accept="image/*" @change="handleAvatarChange" style="display:none" />
        <div class="user-info-text">
          <div class="username">{{ auth.user?.username }}</div>
          <div class="user-since">点击头像更换</div>
        </div>
      </div>

      <el-button v-if="avatarChanged" type="primary" @click="saveAvatar" :loading="savingAvatar" style="width:100%">
        保存头像
      </el-button>
      <el-progress
        v-if="avatarUploadTask && avatarUploadTask.status === 'uploading'"
        :percentage="avatarUploadTask.progress"
        :stroke-width="8"
      />
      <p v-if="avatarUploadTask && avatarUploadTask.status === 'error'" class="upload-error">
        {{ avatarUploadTask.error || '上传失败' }}
      </p>

      <el-divider />

      <div class="twofa-section">
        <div class="twofa-header">
          <div>
            <div class="twofa-title">两步验证 (2FA)</div>
            <div class="twofa-desc">使用身份验证器 App 扫描二维码</div>
          </div>
          <el-tag :type="auth.user?.totp_enabled ? 'success' : 'info'" size="small">
            {{ auth.user?.totp_enabled ? '已启用' : '未启用' }}
          </el-tag>
        </div>

        <div v-if="!auth.user?.totp_enabled">
          <el-button
            @click="setup2FA"
            :loading="setting2FA"
            type="primary"
            class="twofa-action-btn"
            style="width:100%; margin-top:8px"
          >
            启用两步验证
          </el-button>

          <div v-if="totpSetup.qr" class="totp-setup">
            <p class="totp-instruction">使用 Google Authenticator 或 Authy 扫描以下二维码：</p>
            <div class="totp-qr">
              <img :src="totpSetup.qr" width="160" height="160" />
            </div>
            <div class="totp-secret">
              <span class="secret-label">密钥</span>
              <code class="secret-value">{{ totpSetup.secret }}</code>
            </div>
            <el-input v-model="totpSetup.code" placeholder="输入6位验证码确认" maxlength="6" />
            <el-button
              type="primary"
              @click="confirm2FA"
              :loading="confirming"
              class="twofa-action-btn"
              style="width:100%; margin-top:8px"
            >
              确认启用
            </el-button>
          </div>
        </div>

        <div v-else>
          <el-input v-model="disableCode" placeholder="输入当前6位验证码" maxlength="6" style="margin-top:8px" />
          <el-button
            @click="disable2FA"
            :loading="disabling"
            type="danger"
            class="twofa-danger-btn"
            style="width:100%; margin-top:8px"
          >
            关闭两步验证
          </el-button>
        </div>
      </div>
    </div>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Camera, UserFilled } from '@element-plus/icons-vue'
import { useAuthStore } from '@/stores/auth'
import request from '@/utils/request'
import { createManagedUploadTask, removeManagedUpload, type ManagedUploadTask } from '@/utils/managedUploads'
import { uploadImage } from '@/utils/uploads'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [v: boolean]; close: [] }>()

const visible = ref(props.modelValue)
watch(() => props.modelValue, (v) => {
  visible.value = v
})
watch(visible, (v) => emit('update:modelValue', v))

const auth = useAuthStore()
const fileInput = ref<HTMLInputElement>()
const avatarPreview = ref('')
const avatarChanged = ref(false)
const savingAvatar = ref(false)
const selectedAvatarFile = ref<File | null>(null)
const avatarUploadTask = ref<ManagedUploadTask | null>(null)

const setting2FA = ref(false)
const confirming = ref(false)
const disabling = ref(false)
const disableCode = ref('')
const totpSetup = ref({ qr: '', secret: '', code: '' })

function triggerFileInput() {
  fileInput.value?.click()
}

function handleAvatarChange(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  if (!file.type.startsWith('image/')) {
    ElMessage.warning('请选择图片文件')
    return
  }
  if (file.size > 2 * 1024 * 1024) {
    ElMessage.warning('头像文件不能超过2MB')
    return
  }

  selectedAvatarFile.value = file
  if (avatarUploadTask.value) {
    removeManagedUpload(avatarUploadTask.value.id)
    avatarUploadTask.value = null
  }
  const reader = new FileReader()
  reader.onload = (ev) => {
    avatarPreview.value = ev.target?.result as string
    avatarChanged.value = true
  }
  reader.readAsDataURL(file)
}

async function saveAvatar() {
  if (!selectedAvatarFile.value) return
  savingAvatar.value = true
  avatarUploadTask.value = createManagedUploadTask('avatar', selectedAvatarFile.value)
  try {
    const avatarUrl = await uploadImage(selectedAvatarFile.value, 'avatar', { task: avatarUploadTask.value })
    const data = (await request.put('/auth/profile', { avatar: avatarUrl })) as any
    auth.updateUser({ avatar: data.user.avatar })
    avatarPreview.value = data.user.avatar
    avatarChanged.value = false
    selectedAvatarFile.value = null
    if (fileInput.value) {
      fileInput.value.value = ''
    }
    ElMessage.success('头像已更新')
  } catch (err: any) {
    ElMessage.error(err.response?.data?.error || '更新失败')
  } finally {
    savingAvatar.value = false
  }
}

async function setup2FA() {
  setting2FA.value = true
  try {
    const data = (await request.post('/auth/2fa/setup')) as any
    totpSetup.value = { qr: data.qr_code, secret: data.secret, code: '' }
  } catch {
    ElMessage.error('初始化2FA失败')
  } finally {
    setting2FA.value = false
  }
}

async function confirm2FA() {
  if (totpSetup.value.code.length !== 6) {
    ElMessage.warning('请输入6位验证码')
    return
  }

  confirming.value = true
  try {
    await request.post('/auth/2fa/confirm', { code: totpSetup.value.code })
    auth.updateUser({ totp_enabled: true })
    totpSetup.value = { qr: '', secret: '', code: '' }
    ElMessage.success('两步验证已启用')
  } catch (err: any) {
    ElMessage.error(err.response?.data?.error || '验证失败')
  } finally {
    confirming.value = false
  }
}

async function disable2FA() {
  if (!disableCode.value) {
    ElMessage.warning('请输入验证码')
    return
  }

  disabling.value = true
  try {
    await request.post('/auth/2fa/disable', { code: disableCode.value })
    auth.updateUser({ totp_enabled: false })
    disableCode.value = ''
    ElMessage.success('两步验证已关闭')
  } catch (err: any) {
    ElMessage.error(err.response?.data?.error || '关闭失败')
  } finally {
    disabling.value = false
  }
}
</script>

<style scoped>
.profile-dialog {
  display: flex;
  flex-direction: column;
  gap: 16px;
  color: var(--text);
}

.avatar-section {
  display: flex;
  align-items: center;
  gap: 16px;
}

.avatar-wrap {
  width: 72px;
  height: 72px;
  border-radius: 50%;
  overflow: hidden;
  position: relative;
  cursor: pointer;
  flex-shrink: 0;
  background: var(--bg3);
  border: 2px solid var(--border);
}

.avatar-img,
.avatar-placeholder {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 28px;
  color: var(--text3);
}

.avatar-overlay {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  transition: opacity 0.2s;
  color: #fff;
  font-size: 20px;
}

.avatar-wrap:hover .avatar-overlay {
  opacity: 1;
}

.username {
  font-size: 16px;
  font-weight: 600;
  color: var(--text);
}

.user-since {
  font-size: 12px;
  color: var(--text3);
  margin-top: 2px;
}

.upload-error {
  font-size: 12px;
  color: var(--red);
}

.twofa-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

.twofa-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
}

.twofa-desc {
  font-size: 12px;
  color: var(--text2);
}

.totp-setup {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.totp-instruction {
  font-size: 13px;
  color: var(--text2);
}

.totp-qr {
  display: flex;
  justify-content: center;
  padding: 12px;
  background: var(--bg3);
  border: 1px solid var(--border);
  border-radius: var(--r-md);
}

.totp-secret {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  background: var(--bg3);
  border-radius: var(--r-sm);
  border: 1px solid var(--border);
}

.secret-label {
  font-size: 12px;
  color: var(--text3);
  flex-shrink: 0;
}

.secret-value {
  font-family: var(--mono);
  font-size: 13px;
  color: var(--blue2);
  word-break: break-all;
}

:deep(.twofa-action-btn.el-button),
:deep(.twofa-danger-btn.el-button) {
  color: #fff !important;
  font-weight: 600;
}

:deep(.el-divider) {
  border-color: var(--border);
  margin: 4px 0;
}
</style>
