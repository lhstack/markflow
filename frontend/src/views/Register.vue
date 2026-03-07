<template>
  <div class="auth-page">
    <div class="auth-bg">
      <div class="bg-grid"></div>
      <div class="bg-glow"></div>
    </div>
    
    <div class="auth-container">
      <div class="auth-logo">
        <svg width="40" height="40" viewBox="0 0 40 40" fill="none">
          <rect width="40" height="40" rx="10" fill="#2ea043"/>
          <path d="M8 28 L8 12 L16 20 L24 12 L24 28" stroke="white" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
          <path d="M26 16 L32 16 M26 20 L30 20 M26 24 L32 24" stroke="white" stroke-width="2" stroke-linecap="round"/>
        </svg>
        <span class="logo-text">MarkFlow</span>
      </div>
      
      <div class="auth-card">
        <template v-if="system.registrationEnabled">
          <h2 class="auth-title">创建账号</h2>
          <p class="auth-subtitle">开始你的文档管理之旅</p>

          <div class="avatar-picker">
            <div class="avatar-preview" @click="triggerFileInput">
              <img v-if="avatarPreview" :src="avatarPreview" class="avatar-img" />
              <el-icon v-else class="avatar-icon"><Camera /></el-icon>
              <div class="avatar-overlay">
                <el-icon><Camera /></el-icon>
              </div>
            </div>
            <input ref="fileInput" type="file" accept="image/*" @change="handleAvatarChange" style="display: none" />
            <p class="avatar-hint">点击上传头像（可选），最大 {{ system.uploadLimitLabel }}</p>
            <el-progress
              v-if="avatarUploadTask && avatarUploadTask.status === 'uploading'"
              :percentage="avatarUploadTask.progress"
              :stroke-width="8"
            />
            <p v-if="avatarUploadTask && avatarUploadTask.status === 'error'" class="avatar-error">
              {{ avatarUploadTask.error || '上传失败' }}
            </p>
          </div>

          <el-form :model="form" @submit.prevent="handleRegister" class="auth-form">
            <el-form-item>
              <el-input
                v-model="form.username"
                placeholder="用户名 (3-32位)"
                size="large"
                prefix-icon="User"
              />
            </el-form-item>
            
            <el-form-item>
              <el-input
                v-model="form.password"
                type="password"
                placeholder="密码 (至少6位)"
                size="large"
                prefix-icon="Lock"
                show-password
              />
            </el-form-item>

            <el-form-item>
              <el-input
                v-model="form.confirm"
                type="password"
                placeholder="确认密码"
                size="large"
                prefix-icon="Lock"
                show-password
              />
            </el-form-item>

            <el-button
              type="primary"
              size="large"
              class="auth-submit"
              :loading="loading"
              native-type="submit"
              @click="handleRegister"
            >
              注册
            </el-button>
          </el-form>
        </template>
        <template v-else>
          <h2 class="auth-title">注册已关闭</h2>
          <p class="auth-subtitle">当前系统不允许新用户注册，请联系管理员。</p>
        </template>

        <div class="auth-footer">
          已有账号？
          <router-link to="/login">立即登录</router-link>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import { useAuthStore } from '@/stores/auth'
import { createManagedUploadTask, removeManagedUpload, type ManagedUploadTask } from '@/utils/managedUploads'
import request from '@/utils/request'
import { uploadImage } from '@/utils/uploads'
import { useSystemStore } from '@/stores/system'
import { mapAuthErrorMessage } from '@/utils/authErrors'

const router = useRouter()
const auth = useAuthStore()
const system = useSystemStore()
const loading = ref(false)
const avatarPreview = ref('')
const avatarFile = ref<File | null>(null)
const fileInput = ref<HTMLInputElement>()
const avatarUploadTask = ref<ManagedUploadTask | null>(null)

const form = ref({
  username: '',
  password: '',
  confirm: '',
})

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
  if (file.size > system.uploadMaxBytes) {
    ElMessage.warning(`头像文件不能超过 ${system.uploadLimitLabel}`)
    return
  }
  avatarFile.value = file
  if (avatarUploadTask.value) {
    removeManagedUpload(avatarUploadTask.value.id)
    avatarUploadTask.value = null
  }
  const reader = new FileReader()
  reader.onload = (ev) => {
    const result = ev.target?.result as string
    avatarPreview.value = result
  }
  reader.readAsDataURL(file)
}

async function handleRegister() {
  if (!system.registrationEnabled) {
    ElMessage.warning('当前已关闭注册功能')
    return
  }
  if (!form.value.username || !form.value.password) {
    ElMessage.warning('请填写必要信息')
    return
  }
  if (form.value.password !== form.value.confirm) {
    ElMessage.error('两次密码不一致')
    return
  }
  
  loading.value = true
  try {
    const data = await request.post('/auth/register', {
      username: form.value.username,
      password: form.value.password,
    }) as any

    auth.setAuth(data.token, data.user)

    if (avatarFile.value) {
      try {
        avatarUploadTask.value = createManagedUploadTask('avatar', avatarFile.value)
        const avatarUrl = await uploadImage(avatarFile.value, 'avatar', { task: avatarUploadTask.value })
        const updated = await request.put('/auth/profile', { avatar: avatarUrl }) as any
        auth.updateUser({ avatar: updated.user.avatar })
      } catch (uploadErr: any) {
        ElMessage.warning(mapAuthErrorMessage(uploadErr.response?.data?.error, '账号已创建，但头像上传失败'))
      }
    }

    ElMessage.success('注册成功！')
    router.push('/')
  } catch (err: any) {
    ElMessage.error(mapAuthErrorMessage(err.response?.data?.error, '注册失败'))
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  void system.fetchPublicSettings().catch(() => {})
})
</script>

<style scoped>
.auth-page {
  position: relative;
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--color-bg);
  overflow: hidden;
}

.auth-bg {
  position: absolute;
  inset: 0;
  overflow: hidden;
}

.bg-grid {
  position: absolute;
  inset: 0;
  background-image: 
    linear-gradient(var(--color-border) 1px, transparent 1px),
    linear-gradient(90deg, var(--color-border) 1px, transparent 1px);
  background-size: 48px 48px;
  opacity: 0.3;
}

.bg-glow {
  position: absolute;
  top: -200px;
  left: 50%;
  transform: translateX(-50%);
  width: 600px;
  height: 600px;
  background: radial-gradient(circle, rgba(88, 166, 255, 0.12) 0%, transparent 70%);
  border-radius: 50%;
}

.auth-container {
  position: relative;
  z-index: 1;
  width: 100%;
  max-width: 420px;
  padding: 24px;
}

.auth-logo {
  display: flex;
  align-items: center;
  gap: 12px;
  justify-content: center;
  margin-bottom: 32px;
}

.logo-text {
  font-size: 24px;
  font-weight: 600;
  color: var(--color-text);
  letter-spacing: -0.5px;
}

.auth-card {
  background: var(--color-bg-secondary);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  padding: 32px;
  box-shadow: var(--shadow-lg);
}

.auth-title {
  font-size: 22px;
  font-weight: 600;
  color: var(--color-text);
  margin-bottom: 6px;
}

.auth-subtitle {
  font-size: 14px;
  color: var(--color-text-secondary);
  margin-bottom: 24px;
}

.avatar-picker {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  margin-bottom: 24px;
}

.avatar-preview {
  width: 80px;
  height: 80px;
  border-radius: 50%;
  background: var(--color-bg-tertiary);
  border: 2px dashed var(--color-border);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  position: relative;
  overflow: hidden;
  transition: border-color 0.2s;
}

.avatar-preview:hover {
  border-color: var(--color-accent);
}

.avatar-img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.avatar-icon {
  font-size: 28px;
  color: var(--color-text-muted);
}

.avatar-overlay {
  position: absolute;
  inset: 0;
  background: rgba(0,0,0,0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  transition: opacity 0.2s;
  color: white;
  font-size: 20px;
}

.avatar-preview:hover .avatar-overlay {
  opacity: 1;
}

.avatar-hint {
  font-size: 12px;
  color: var(--color-text-muted);
}

.avatar-error {
  font-size: 12px;
  color: #dd4d4d;
}

.auth-form {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.auth-submit {
  width: 100%;
  margin-top: 8px;
  font-size: 15px;
  font-weight: 500;
}

.auth-footer {
  margin-top: 20px;
  text-align: center;
  font-size: 14px;
  color: var(--color-text-secondary);
}

.auth-footer a {
  color: var(--color-accent);
  text-decoration: none;
  font-weight: 500;
}

:deep(.el-form-item) {
  margin-bottom: 16px;
}
</style>
