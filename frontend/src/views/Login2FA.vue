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
        <h1 class="auth-title">两步验证</h1>
        <p class="auth-subtitle">请输入身份验证器中的 6 位验证码</p>

        <div v-if="pendingUsername" class="auth-meta">账号：{{ pendingUsername }}</div>

        <div class="form-field">
          <label>2FA 验证码</label>
          <div class="input-wrap">
            <svg class="input-icon" width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M0 4.75C0 3.784.784 3 1.75 3h12.5c.966 0 1.75.784 1.75 1.75v6.5A1.75 1.75 0 0 1 14.25 13H1.75A1.75 1.75 0 0 1 0 11.25ZM1.75 4.5a.25.25 0 0 0-.25.25v6.5c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25v-6.5a.25.25 0 0 0-.25-.25Z"/></svg>
            <input
              v-model="code"
              class="field-input"
              maxlength="6"
              inputmode="numeric"
              autocomplete="one-time-code"
              placeholder="请输入 6 位验证码"
              @keydown.enter="verify2FA"
            />
          </div>
        </div>

        <div v-if="error" class="auth-error">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M6.457 1.047c.659-1.234 2.427-1.234 3.086 0l6.082 11.378A1.75 1.75 0 0 1 14.082 15H1.918a1.75 1.75 0 0 1-1.543-2.575Zm1.763.707a.25.25 0 0 0-.44 0L1.698 13.132a.25.25 0 0 0 .22.368h12.164a.25.25 0 0 0 .22-.368Zm.53 3.996v2.5a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0ZM9 11a1 1 0 1 1-2 0 1 1 0 0 1 2 0Z"/></svg>
          {{ error }}
        </div>

        <button class="auth-btn" :disabled="loading" @click="verify2FA">
          <span v-if="loading" class="spin">◌</span>
          {{ loading ? '验证中...' : '确认登录' }}
        </button>

        <button class="back-btn" :disabled="loading" @click="backToLogin">返回登录页</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import request from '@/utils/request'

const PENDING_2FA_KEY = 'markflow.pending_2fa'

type Pending2FA = {
  challenge_id: string
  username?: string
}

const route = useRoute()
const router = useRouter()
const auth = useAuthStore()

const code = ref('')
const loading = ref(false)
const error = ref('')
const pending = ref<Pending2FA | null>(null)

const pendingUsername = computed(() => pending.value?.username || '')

function readPendingFromStorage(): Pending2FA | null {
  try {
    const raw = sessionStorage.getItem(PENDING_2FA_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw) as Pending2FA
    if (!parsed?.challenge_id) return null
    return parsed
  } catch {
    return null
  }
}

function persistPending(data: Pending2FA) {
  sessionStorage.setItem(PENDING_2FA_KEY, JSON.stringify(data))
}

function clearPending() {
  sessionStorage.removeItem(PENDING_2FA_KEY)
}

function initPending() {
  const queryCid = String(route.query.cid || '').trim()
  const stored = readPendingFromStorage()

  if (queryCid) {
    pending.value = { challenge_id: queryCid, username: stored?.username }
    persistPending(pending.value)
    return
  }

  pending.value = stored
}

function backToLogin() {
  clearPending()
  router.replace('/login')
}

async function verify2FA() {
  if (!pending.value?.challenge_id) {
    error.value = '登录会话不存在或已失效，请重新登录'
    return
  }

  if (code.value.trim().length !== 6) {
    error.value = '请输入 6 位验证码'
    return
  }

  loading.value = true
  error.value = ''

  try {
    const data = (await request.post('/auth/login/2fa', {
      challenge_id: pending.value.challenge_id,
      totp_code: code.value.trim(),
    })) as any

    auth.setAuth(data.token, data.user)
    clearPending()
    router.replace('/')
  } catch (e: any) {
    const msg = e.response?.data?.error || '验证失败'
    error.value = msg
    if (msg.toLowerCase().includes('expired')) {
      clearPending()
    }
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  initPending()
  if (!pending.value?.challenge_id) {
    router.replace('/login')
  }
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
  margin-bottom: 10px;
}

.auth-meta {
  font-size: 12px;
  color: var(--text2);
  margin-bottom: 16px;
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

.back-btn {
  width: 100%;
  margin-top: 10px;
  padding: 10px;
  background: var(--bg3);
  border: 1px solid var(--border);
  border-radius: var(--r-sm);
  color: var(--text);
  font-size: 13px;
  cursor: pointer;
}

.back-btn:hover:not(:disabled) {
  border-color: var(--border2);
}

.spin {
  display: inline-block;
  animation: spin 0.8s linear infinite;
}

@keyframes spin { to { transform: rotate(360deg); } }
</style>
