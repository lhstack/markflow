<template>
  <div class="auth-page">
    <div class="auth-bg">
      <div class="auth-grid"></div>
      <div class="auth-glow"></div>
    </div>
    <div class="auth-container">
      <div class="auth-logo">
        <svg width="40" height="40" viewBox="0 0 40 40" fill="none">
          <rect width="40" height="40" rx="10" fill="#238636"/>
          <path d="M8 28 L8 12 L16 20 L24 12 L24 28" stroke="white" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
          <path d="M26 16 L32 16 M26 20 L30 20 M26 24 L32 24" stroke="white" stroke-width="2" stroke-linecap="round"/>
        </svg>
        <span class="logo-text">MarkFlow</span>
      </div>

      <div class="auth-card">
        <h1 class="auth-title">欢迎回来</h1>
        <p class="auth-subtitle">登录以继续访问你的文档</p>

        <div class="form-field">
          <label>用户名</label>
          <div class="input-wrap">
            <svg class="input-icon" width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M10.561 8.073a6.005 6.005 0 0 1 3.432 5.142.75.75 0 1 1-1.498.07 4.5 4.5 0 0 0-8.99 0 .75.75 0 0 1-1.498-.07 6.004 6.004 0 0 1 3.431-5.142 3.999 3.999 0 1 1 5.123 0ZM10.5 5a2.5 2.5 0 1 0-5 0 2.5 2.5 0 0 0 5 0Z"/></svg>
            <input v-model="form.username" class="field-input" placeholder="请输入用户名" @keydown.enter="login" />
          </div>
        </div>

        <div class="form-field">
          <label>密码</label>
          <div class="input-wrap">
            <svg class="input-icon" width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M4 7.5V6a4 4 0 0 1 8 0v1.5h.25c.966 0 1.75.784 1.75 1.75v5.5A1.75 1.75 0 0 1 12.25 16h-8.5A1.75 1.75 0 0 1 2 14.75v-5.5C2 8.284 2.784 7.5 3.75 7.5Zm1.5-1.5v1.5h5V6a2.5 2.5 0 0 0-5 0Z"/></svg>
            <input v-model="form.password" class="field-input" type="password" placeholder="请输入密码" @keydown.enter="login" />
          </div>
        </div>

        <div class="form-field">
          <label>验证码</label>
          <div class="captcha-row">
            <div class="input-wrap" style="flex:1">
              <input v-model="form.captcha" class="field-input" placeholder="输入计算结果" @keydown.enter="login" />
            </div>
            <div class="captcha-img" @click="refreshCaptcha" title="点击刷新">
              <img v-if="captchaImg" :src="captchaImg" />
              <span v-else class="captcha-loading">加载中...</span>
            </div>
          </div>
        </div>

        <div v-if="error" class="auth-error">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M6.457 1.047c.659-1.234 2.427-1.234 3.086 0l6.082 11.378A1.75 1.75 0 0 1 14.082 15H1.918a1.75 1.75 0 0 1-1.543-2.575Zm1.763.707a.25.25 0 0 0-.44 0L1.698 13.132a.25.25 0 0 0 .22.368h12.164a.25.25 0 0 0 .22-.368Zm.53 3.996v2.5a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0ZM9 11a1 1 0 1 1-2 0 1 1 0 0 1 2 0Z"/></svg>
          {{ error }}
        </div>

        <button class="auth-btn" :disabled="loading" @click="login">
          <span v-if="loading" class="spin">◌</span>
          {{ loading ? '登录中...' : '登录' }}
        </button>

        <div class="auth-footer">
          <template v-if="system.registrationEnabled">
            还没有账号？<router-link to="/register">立即注册</router-link>
          </template>
          <template v-else>
            当前已关闭用户注册
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import request from '@/utils/request'
import { useSystemStore } from '@/stores/system'
import { mapAuthErrorMessage } from '@/utils/authErrors'

const PENDING_2FA_KEY = 'markflow.pending_2fa'

const router = useRouter()
const auth = useAuthStore()
const system = useSystemStore()

const form = ref({ username: '', password: '', captcha: '' })
const captchaId = ref('')
const captchaImg = ref('')
const loading = ref(false)
const error = ref('')

async function refreshCaptcha() {
  try {
    const data = (await request.get('/auth/captcha')) as any
    captchaId.value = data.captcha_id
    captchaImg.value = data.image
  } catch {
    // ignore
  }
}

async function login() {
  if (!form.value.username || !form.value.password || !form.value.captcha) {
    error.value = '请填写所有必填项'
    return
  }

  loading.value = true
  error.value = ''

  try {
    const data = (await request.post('/auth/login', {
      username: form.value.username,
      password: form.value.password,
      captcha_id: captchaId.value,
      captcha_answer: form.value.captcha,
    })) as any

    if (data?.require_2fa && data?.challenge_id) {
      sessionStorage.setItem(
        PENDING_2FA_KEY,
        JSON.stringify({ challenge_id: data.challenge_id, username: form.value.username })
      )
      await router.push({ name: 'Login2FA', query: { cid: data.challenge_id } })
      return
    }

    auth.setAuth(data.token, data.user)
    sessionStorage.removeItem(PENDING_2FA_KEY)
    router.push('/')
  } catch (e: any) {
    const msg = mapAuthErrorMessage(e.response?.data?.error, '登录失败')
    error.value = msg
    refreshCaptcha()
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  await system.fetchPublicSettings().catch(() => {})
  await refreshCaptcha()
})
</script>

<style scoped>
.auth-page {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg);
  position: relative;
  overflow: hidden;
}

.auth-bg {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.auth-grid {
  position: absolute;
  inset: 0;
  background-image: linear-gradient(var(--border) 1px, transparent 1px),
                    linear-gradient(90deg, var(--border) 1px, transparent 1px);
  background-size: 48px 48px;
  opacity: 0.2;
}

.auth-glow {
  position: absolute;
  top: -200px;
  left: 50%;
  transform: translateX(-50%);
  width: 600px;
  height: 600px;
  background: radial-gradient(circle, rgba(35,134,54,0.15) 0%, transparent 65%);
  border-radius: 50%;
}

.auth-container {
  position: relative;
  z-index: 1;
  width: 100%;
  max-width: 380px;
  padding: 24px;
}

.auth-logo {
  display: flex;
  align-items: center;
  gap: 12px;
  justify-content: center;
  margin-bottom: 28px;
}

.logo-text {
  font-size: 24px;
  font-weight: 700;
  letter-spacing: -0.5px;
}

.auth-card {
  background: var(--bg2);
  border: 1px solid var(--border);
  border-radius: var(--r-xl);
  padding: 28px;
  box-shadow: var(--shadow-xl);
}

.auth-title {
  font-size: 28px;
  font-weight: 700;
  margin-bottom: 6px;
  color: var(--text);
}

.auth-subtitle {
  font-size: 13px;
  color: var(--text3);
  margin-bottom: 22px;
}

.form-field {
  margin-bottom: 14px;
}

.form-field label {
  display: block;
  font-size: 12px;
  font-weight: 500;
  color: var(--text2);
  margin-bottom: 5px;
}

.input-wrap { position: relative; }

.input-icon {
  position: absolute;
  left: 11px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--text3);
  pointer-events: none;
}

.field-input {
  width: 100%;
  padding: 9px 12px 9px 32px;
  background: var(--bg3);
  border: 1px solid var(--border);
  border-radius: var(--r-sm);
  color: var(--text);
  font-size: 14px;
  font-family: var(--font);
  outline: none;
  transition: border-color 0.15s;
}

.field-input:focus { border-color: var(--blue); }
.field-input::placeholder { color: var(--text3); }

.captcha-row {
  display: flex;
  gap: 10px;
}

.captcha-img {
  width: 120px;
  height: 38px;
  background: transparent;
  border: none;
  border-radius: var(--r-sm);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  overflow: hidden;
  flex-shrink: 0;
}

.captcha-img img {
  width: 100%;
  height: 100%;
  object-fit: contain;
  display: block;
}

.captcha-loading { font-size: 11px; color: var(--text3); }

.auth-error {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  background: rgba(248,81,73,0.08);
  border: 1px solid rgba(248,81,73,0.25);
  border-radius: var(--r-sm);
  color: var(--red);
  font-size: 13px;
  margin-bottom: 12px;
}

.auth-btn {
  width: 100%;
  padding: 10px;
  background: var(--green);
  border: none;
  border-radius: var(--r-sm);
  color: #fff;
  font-size: 14px;
  font-weight: 600;
  font-family: var(--font);
  cursor: pointer;
  transition: all 0.15s;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  margin-top: 4px;
}

.auth-btn:hover:not(:disabled) { background: var(--green2); }
.auth-btn:disabled { opacity: 0.6; cursor: not-allowed; }

.auth-footer {
  text-align: center;
  font-size: 13px;
  color: var(--text3);
  margin-top: 16px;
}

.auth-footer a { color: var(--blue); text-decoration: none; }
.auth-footer a:hover { text-decoration: underline; }

.spin {
  display: inline-block;
  animation: spin 0.8s linear infinite;
}

@keyframes spin { to { transform: rotate(360deg); } }
</style>
