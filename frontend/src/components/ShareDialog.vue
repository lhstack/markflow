<template>
  <Teleport to="body">
    <div v-if="modelValue" class="dialog-mask" @click.self="close">
      <div class="share-dialog">
        <div class="sd-header">
          <div class="sd-title">
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" style="color:var(--text3)">
              <path d="M11.013 1.427a1.75 1.75 0 0 1 2.474 0l1.086 1.086a1.75 1.75 0 0 1 0 2.474l-8.61 8.61c-.21.21-.47.364-.756.445l-3.251.93a.75.75 0 0 1-.927-.928l.929-3.25c.081-.286.235-.547.445-.758l8.61-8.61Z"/>
            </svg>
            <span>分享</span>
            <span class="sd-docname">{{ node?.name }}</span>
          </div>
          <button class="sd-close" @click="close" aria-label="close">
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M3.72 3.72a.75.75 0 0 1 1.06 0L8 6.94l3.22-3.22a.749.749 0 0 1 1.275.326.749.749 0 0 1-.215.734L9.06 8l3.22 3.22a.749.749 0 0 1-.326 1.275.749.749 0 0 1-.734-.215L8 9.06l-3.22 3.22a.751.751 0 0 1-1.042-.018.751.751 0 0 1-.018-1.042L6.94 8 3.72 4.78a.75.75 0 0 1 0-1.06Z"/></svg>
          </button>
        </div>

        <div class="sd-section">
          <div class="sd-section-title">创建新链接</div>
          <div class="sd-form">
            <div class="sd-row">
              <div class="sd-row-left">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" style="color:var(--text3)">
                  <path d="M4 7.5V6a4 4 0 0 1 8 0v1.5h.25c.966 0 1.75.784 1.75 1.75v5.5A1.75 1.75 0 0 1 12.25 16h-8.5A1.75 1.75 0 0 1 2 14.75v-5.5C2 8.284 2.784 7.5 3.75 7.5Zm1.5-1.5v1.5h5V6a2.5 2.5 0 0 0-5 0Z"/>
                </svg>
                <span>密码保护</span>
              </div>
              <label class="toggle">
                <input type="checkbox" v-model="form.hasPassword" />
                <span class="toggle-track"></span>
              </label>
            </div>

            <transition name="expand">
              <div v-if="form.hasPassword" class="sd-pw-input">
                <div class="field-wrap">
                  <svg class="field-icon" width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M4 7.5V6a4 4 0 0 1 8 0v1.5h.25c.966 0 1.75.784 1.75 1.75v5.5A1.75 1.75 0 0 1 12.25 16h-8.5A1.75 1.75 0 0 1 2 14.75v-5.5C2 8.284 2.784 7.5 3.75 7.5Zm1.5-1.5v1.5h5V6a2.5 2.5 0 0 0-5 0Z"/></svg>
                  <input v-model="form.password" class="sd-input" type="password" placeholder="设置访问密码" />
                </div>
              </div>
            </transition>

            <div class="sd-row">
              <div class="sd-row-left">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" style="color:var(--text3)">
                  <path d="M4.75 0a.75.75 0 0 1 .75.75V2h5V.75a.75.75 0 0 1 1.5 0V2h1.25c.966 0 1.75.784 1.75 1.75v10.5A1.75 1.75 0 0 1 13.25 16H2.75A1.75 1.75 0 0 1 1 14.25V3.75C1 2.784 1.784 2 2.75 2H4V.75A.75.75 0 0 1 4.75 0ZM2.5 7.5v6.75c0 .138.112.25.25.25h10.5a.25.25 0 0 0 .25-.25V7.5Z"/>
                </svg>
                <span>有效期</span>
              </div>
              <div class="expire-tabs">
                <button
                  v-for="opt in expireOptions"
                  :key="opt.value"
                  :class="['et-btn', { active: form.expireOption === opt.value }]"
                  @click="form.expireOption = opt.value"
                >{{ opt.label }}</button>
              </div>
            </div>

            <transition name="expand">
              <div v-if="form.expireOption === 'custom'" class="sd-custom-expire">
                <div class="field-wrap">
                  <svg class="field-icon" width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M4.75 0a.75.75 0 0 1 .75.75V2h5V.75a.75.75 0 0 1 1.5 0V2h1.25c.966 0 1.75.784 1.75 1.75v10.5A1.75 1.75 0 0 1 13.25 16H2.75A1.75 1.75 0 0 1 1 14.25V3.75C1 2.784 1.784 2 2.75 2H4V.75A.75.75 0 0 1 4.75 0Zm-2.25 7.5v6.75c0 .138.112.25.25.25h10.5a.25.25 0 0 0 .25-.25V7.5Z"/></svg>
                  <input v-model="form.customExpiresAtLocal" class="sd-input" type="datetime-local" />
                </div>
                <p class="sd-hint">按本地时区到点失效</p>
              </div>
            </transition>
          </div>

          <button class="sd-create-btn" :disabled="creating" @click="createShare">
            <svg v-if="!creating" width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M7.75 2a.75.75 0 0 1 .75.75V7h4.25a.75.75 0 0 1 0 1.5H8.5v4.25a.75.75 0 0 1-1.5 0V8.5H2.75a.75.75 0 0 1 0-1.5H7V2.75A.75.75 0 0 1 7.75 2Z"/></svg>
            <span v-else class="spin">◌</span>
            {{ creating ? '创建中...' : '生成分享链接' }}
          </button>
        </div>

        <div v-if="shares.length" class="sd-section">
          <div class="sd-section-title">已有链接 <span class="sd-count">{{ shares.length }}</span></div>
          <div class="sd-list">
            <div v-for="share in shares" :key="share.id" class="sd-share-item">
              <div class="ssi-url-row">
                <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" style="color:var(--text3);flex-shrink:0">
                  <path d="m7.775 3.275 1.25-1.25a3.5 3.5 0 1 1 4.95 4.95l-2.5 2.5a3.5 3.5 0 0 1-4.95 0 .751.751 0 0 1 .018-1.042.751.751 0 0 1 1.042-.018 2 2 0 0 0 2.83 0l2.5-2.5a2 2 0 0 0-2.83-2.83l-1.25 1.25a.751.751 0 0 1-1.042-.018.751.751 0 0 1-.018-1.042Zm-4.69 9.64a2 2 0 0 0 2.83 0l1.25-1.25a.751.751 0 0 1 1.042.018.751.751 0 0 1 .018 1.042l-1.25 1.25a3.5 3.5 0 1 1-4.95-4.95l2.5-2.5a3.5 3.5 0 0 1 4.95 0 .751.751 0 0 1-.018 1.042.751.751 0 0 1-1.042.018 2 2 0 0 0-2.83 0l-2.5 2.5a2 2 0 0 0 0 2.83Z"/>
                </svg>
                <span class="ssi-url" @click="copyLink(share.token)">{{ getShareUrl(share.token) }}</span>
              </div>
              <div class="ssi-meta-row">
                <div class="ssi-tags">
                  <span v-if="share.has_password" class="ssi-tag tag-yellow">密码保护</span>
                  <span v-if="share.expires_at" class="ssi-tag tag-blue">{{ fmtExpiry(share.expires_at) }} 到期</span>
                  <span v-if="!share.expires_at" class="ssi-tag tag-green">永久</span>
                </div>
                <div class="ssi-ops">
                  <button class="ssi-btn" title="复制链接" @click="copyLink(share.token)">
                    <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M0 6.75C0 5.784.784 5 1.75 5h1.5a.75.75 0 0 1 0 1.5h-1.5a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-1.5a.75.75 0 0 1 1.5 0v1.5A1.75 1.75 0 0 1 9.25 16h-7.5A1.75 1.75 0 0 1 0 14.25Z"/><path d="M5 1.75C5 .784 5.784 0 6.75 0h7.5C15.216 0 16 .784 16 1.75v7.5A1.75 1.75 0 0 1 14.25 11h-7.5A1.75 1.75 0 0 1 5 9.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z"/></svg>
                  </button>
                  <button
                    v-if="share.has_password"
                    class="ssi-btn ssi-btn-pass"
                    title="复制链接+标题+密码"
                    @click="copyLinkWithPassword(share)"
                  >
                    <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
                      <path d="M4 7.5V6a4 4 0 1 1 8 0v1.5h.25c.966 0 1.75.784 1.75 1.75v5.5A1.75 1.75 0 0 1 12.25 16h-8.5A1.75 1.75 0 0 1 2 14.75v-5.5C2 8.284 2.784 7.5 3.75 7.5Zm1.5-1.5v1.5h5V6a2.5 2.5 0 0 0-5 0Z"/>
                    </svg>
                  </button>
                  <button class="ssi-btn ssi-btn-del" title="删除" @click="deleteShare(share.id)">
                    <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M11 1.75V3h2.25a.75.75 0 0 1 0 1.5H2.75a.75.75 0 0 1 0-1.5H5V1.75C5 .784 5.784 0 6.75 0h2.5C10.216 0 11 .784 11 1.75ZM4.496 6.675l.66 6.6a.25.25 0 0 0 .249.225h5.19a.25.25 0 0 0 .249-.225l.66-6.6a.75.75 0 0 1 1.492.149l-.66 6.6A1.748 1.748 0 0 1 10.595 15h-5.19a1.75 1.75 0 0 1-1.741-1.575l-.66-6.6a.75.75 0 1 1 1.492-.15ZM6.5 1.75V3h3V1.75a.25.25 0 0 0-.25-.25h-2.5a.25.25 0 0 0-.25.25Z"/></svg>
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>

        <div v-else-if="!creating" class="sd-empty">
          <svg width="24" height="24" viewBox="0 0 16 16" fill="currentColor" style="color:var(--text3)"><path d="m7.775 3.275 1.25-1.25a3.5 3.5 0 1 1 4.95 4.95l-2.5 2.5a3.5 3.5 0 0 1-4.95 0 .751.751 0 0 1 .018-1.042.751.751 0 0 1 1.042-.018 2 2 0 0 0 2.83 0l2.5-2.5a2 2 0 0 0-2.83-2.83l-1.25 1.25a.751.751 0 0 1-1.042-.018.751.751 0 0 1-.018-1.042Zm-4.69 9.64a2 2 0 0 0 2.83 0l1.25-1.25a.751.751 0 0 1 1.042.018.751.751 0 0 1 .018 1.042l-1.25 1.25a3.5 3.5 0 1 1-4.95-4.95l2.5-2.5a3.5 3.5 0 0 1 4.95 0 .751.751 0 0 1-.018 1.042.751.751 0 0 1-1.042.018 2 2 0 0 0-2.83 0l-2.5 2.5a2 2 0 0 0 0 2.83Z"/></svg>
          <span>尚未创建分享链接</span>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import request from '@/utils/request'
import type { DocNode } from '@/stores/docs'

type ShareItem = {
  id: string
  token: string
  has_password: boolean
  expires_at?: string | null
}

const SHARE_PASSWORD_CACHE_KEY = 'markflow.share.password.by_token'

const props = defineProps<{ modelValue: boolean; node: DocNode | null }>()
const emit = defineEmits(['update:modelValue'])

function close() {
  emit('update:modelValue', false)
}

const shares = ref<ShareItem[]>([])
const creating = ref(false)
const form = ref({
  hasPassword: false,
  password: '',
  expireOption: 'never',
  customExpiresAtLocal: '',
})

const expireOptions = [
  { label: '永久', value: 'never' },
  { label: '1天', value: '1d' },
  { label: '7天', value: '7d' },
  { label: '30天', value: '30d' },
  { label: '自定义', value: 'custom' },
]

function getShareUrl(token: string) {
  return `${window.location.origin}/s/${token}`
}

function fmtExpiry(dt: string) {
  const d = new Date(dt)
  const yyyy = d.getFullYear()
  const MM = `${d.getMonth() + 1}`.padStart(2, '0')
  const dd = `${d.getDate()}`.padStart(2, '0')
  const hh = `${d.getHours()}`.padStart(2, '0')
  const mm = `${d.getMinutes()}`.padStart(2, '0')
  return `${yyyy}/${MM}/${dd} ${hh}:${mm}`
}

function readPasswordCache(): Record<string, string> {
  try {
    const raw = localStorage.getItem(SHARE_PASSWORD_CACHE_KEY)
    if (!raw) return {}
    const parsed = JSON.parse(raw) as Record<string, string>
    return parsed && typeof parsed === 'object' ? parsed : {}
  } catch {
    return {}
  }
}

function writePasswordCache(next: Record<string, string>) {
  localStorage.setItem(SHARE_PASSWORD_CACHE_KEY, JSON.stringify(next))
}

function getCachedPassword(token: string): string | undefined {
  const cache = readPasswordCache()
  const pw = cache[token]
  return typeof pw === 'string' && pw.length ? pw : undefined
}

function cachePassword(token: string, password: string) {
  const normalized = password.trim()
  if (!token || !normalized) return
  const cache = readPasswordCache()
  cache[token] = normalized
  writePasswordCache(cache)
}

function removeCachedPassword(token: string) {
  const cache = readPasswordCache()
  if (!cache[token]) return
  delete cache[token]
  writePasswordCache(cache)
}

function buildCopyPayload(share: ShareItem, password: string) {
  const title = (props.node?.name || '未命名').trim()
  return `${getShareUrl(share.token)} 《${title}》 密码：${password}`
}

async function loadShares() {
  if (!props.node) return
  try {
    const data = (await request.get(`/shares/doc/${props.node.id}`)) as any
    shares.value = (data.shares || []) as ShareItem[]
  } catch {
    // ignore
  }
}

function resolveExpiresAtISO(): string | undefined {
  const expiryMap: Record<string, number> = { '1d': 1, '7d': 7, '30d': 30 }

  if (form.value.expireOption === 'never') {
    return undefined
  }

  if (form.value.expireOption === 'custom') {
    if (!form.value.customExpiresAtLocal) {
      ElMessage.warning('请设置具体到期时间')
      return 'invalid'
    }
    const custom = new Date(form.value.customExpiresAtLocal)
    if (Number.isNaN(custom.getTime())) {
      ElMessage.warning('到期时间格式无效')
      return 'invalid'
    }
    if (custom.getTime() <= Date.now()) {
      ElMessage.warning('到期时间必须晚于当前时间')
      return 'invalid'
    }
    return custom.toISOString()
  }

  const days = expiryMap[form.value.expireOption]
  if (!days) return undefined
  const d = new Date()
  d.setDate(d.getDate() + days)
  return d.toISOString()
}

async function createShare() {
  if (!props.node) return

  if (form.value.hasPassword && !form.value.password.trim()) {
    ElMessage.warning('请输入访问密码')
    return
  }

  const expiresAt = resolveExpiresAtISO()
  if (expiresAt === 'invalid') return

  const currentPassword = form.value.password.trim()
  creating.value = true
  try {
    const data = (await request.post('/shares', {
      doc_id: props.node.id,
      password: form.value.hasPassword ? currentPassword : undefined,
      expires_at: expiresAt,
    })) as { share?: ShareItem }

    if (form.value.hasPassword && data?.share?.token && currentPassword) {
      cachePassword(data.share.token, currentPassword)
    }

    ElMessage({ message: '链接已创建', type: 'success', duration: 1500 })
    form.value = { hasPassword: false, password: '', expireOption: 'never', customExpiresAtLocal: '' }
    await loadShares()
  } catch {
    ElMessage.error('创建失败')
  } finally {
    creating.value = false
  }
}

async function deleteShare(id: string) {
  const share = shares.value.find((s) => s.id === id)
  try {
    await request.delete(`/shares/${id}`)
    shares.value = shares.value.filter((s) => s.id !== id)
    if (share?.token) removeCachedPassword(share.token)
    ElMessage({ message: '已删除', type: 'success', duration: 1200 })
  } catch {
    ElMessage.error('删除失败')
  }
}

async function copyLink(token: string) {
  try {
    await navigator.clipboard.writeText(getShareUrl(token))
    ElMessage({ message: '链接已复制', type: 'success', duration: 1200 })
  } catch {
    ElMessage.error('复制失败')
  }
}

async function copyLinkWithPassword(share: ShareItem) {
  let password = getCachedPassword(share.token)
  if (!password) {
    const input = window.prompt('请输入该分享链接密码（仅保存在当前浏览器）', '')
    password = input?.trim()
    if (!password) {
      ElMessage.warning('未输入密码，已取消复制')
      return
    }
    cachePassword(share.token, password)
  }

  try {
    await navigator.clipboard.writeText(buildCopyPayload(share, password))
    ElMessage({ message: '链接、标题和密码已复制', type: 'success', duration: 1200 })
  } catch {
    ElMessage.error('复制失败')
  }
}

watch(
  () => props.modelValue,
  (v) => {
    if (v) loadShares()
  },
  { immediate: true }
)

watch(
  () => props.node?.id,
  (id, prev) => {
    if (props.modelValue && id && id !== prev) {
      loadShares()
    }
  }
)
</script>

<style scoped>
.dialog-mask {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  z-index: 2000;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
  backdrop-filter: blur(4px);
  animation: mask-in 0.15s ease;
}

@keyframes mask-in {
  from { opacity: 0; }
  to { opacity: 1; }
}

.share-dialog {
  width: 100%;
  max-width: 500px;
  background: var(--bg2);
  border: 1px solid var(--border);
  border-radius: var(--r-xl);
  box-shadow: var(--shadow-xl);
  overflow: hidden;
  animation: dialog-in 0.18s cubic-bezier(0.4, 0, 0.2, 1);
}

@keyframes dialog-in {
  from { transform: scale(0.96) translateY(8px); opacity: 0; }
  to { transform: scale(1) translateY(0); opacity: 1; }
}

.sd-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border);
}

.sd-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  font-weight: 600;
}

.sd-docname {
  color: var(--text2);
  font-weight: 400;
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  padding-left: 4px;
  border-left: 1px solid var(--border);
}

.sd-close {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--text3);
  border-radius: var(--r-sm);
  cursor: pointer;
  transition: all 0.12s;
}

.sd-close:hover {
  background: color-mix(in srgb, var(--blue) 12%, transparent);
  color: var(--text);
}

.sd-section {
  padding: 16px 20px;
  border-bottom: 1px solid var(--border);
}

.sd-section:last-child {
  border-bottom: none;
}

.sd-section-title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.6px;
  text-transform: uppercase;
  color: var(--text3);
  margin-bottom: 12px;
  display: flex;
  align-items: center;
  gap: 6px;
}

.sd-count {
  font-size: 11px;
  background: var(--bg3);
  border: 1px solid var(--border);
  border-radius: 20px;
  padding: 0 6px;
  color: var(--text2);
  text-transform: none;
  letter-spacing: 0;
  font-weight: 500;
}

.sd-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-bottom: 14px;
  background: var(--bg3);
  border: 1px solid var(--border);
  border-radius: var(--r-md);
  padding: 12px 14px;
}

.sd-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.sd-row-left {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 13px;
  color: var(--text2);
}

.toggle {
  position: relative;
  display: inline-block;
  cursor: pointer;
}

.toggle input {
  opacity: 0;
  width: 0;
  height: 0;
  position: absolute;
}

.toggle-track {
  display: block;
  width: 36px;
  height: 20px;
  background: var(--bg4);
  border: 1px solid var(--border2);
  border-radius: 10px;
  transition: all 0.2s;
  position: relative;
}

.toggle-track::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 14px;
  height: 14px;
  background: white;
  border-radius: 50%;
  transition: transform 0.2s;
  opacity: 0.5;
}

.toggle input:checked + .toggle-track {
  background: var(--green2);
  border-color: var(--green2);
}

.toggle input:checked + .toggle-track::after {
  transform: translateX(16px);
  opacity: 1;
}

.expand-enter-active,
.expand-leave-active {
  transition: all 0.2s ease;
  overflow: hidden;
}

.expand-enter-from,
.expand-leave-to {
  max-height: 0;
  opacity: 0;
}

.expand-enter-to,
.expand-leave-from {
  max-height: 90px;
  opacity: 1;
}

.sd-pw-input,
.sd-custom-expire {
  padding-top: 2px;
}

.field-wrap {
  position: relative;
}

.field-icon {
  position: absolute;
  left: 10px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--text3);
  pointer-events: none;
}

.sd-input {
  width: 100%;
  padding: 8px 12px 8px 30px;
  background: var(--bg2);
  border: 1px solid var(--border);
  border-radius: var(--r-sm);
  color: var(--text);
  font-size: 13px;
  font-family: var(--font);
  outline: none;
  transition: border-color 0.15s;
}

.sd-input:focus {
  border-color: var(--blue);
}

.sd-hint {
  margin-top: 6px;
  font-size: 11px;
  color: var(--text3);
}

.expire-tabs {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}

.et-btn {
  padding: 4px 10px;
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text3);
  border-radius: var(--r-sm);
  font-size: 12px;
  cursor: pointer;
  font-family: var(--font);
  transition: all 0.12s;
}

.et-btn:hover {
  border-color: var(--border2);
  color: var(--text2);
}

.et-btn.active {
  background: var(--green-dim);
  border-color: var(--green2);
  color: var(--green3);
}

.sd-create-btn {
  width: 100%;
  padding: 9px;
  background: var(--green);
  border: none;
  border-radius: var(--r-sm);
  color: #fff;
  font-size: 13px;
  font-weight: 600;
  font-family: var(--font);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  transition: background 0.15s;
}

.sd-create-btn:hover:not(:disabled) {
  background: var(--green2);
}

.sd-create-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.sd-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.sd-share-item {
  background: var(--bg3);
  border: 1px solid var(--border);
  border-radius: var(--r-md);
  padding: 10px 12px;
  transition: border-color 0.12s;
}

.sd-share-item:hover {
  border-color: var(--border2);
}

.ssi-url-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 8px;
}

.ssi-url {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--blue);
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.ssi-url:hover {
  text-decoration: underline;
}

.ssi-meta-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.ssi-tags {
  display: flex;
  gap: 5px;
  flex-wrap: wrap;
}

.ssi-tag {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 7px;
  border-radius: 20px;
  font-size: 11px;
  border: 1px solid;
}

.tag-yellow {
  background: rgba(210, 153, 34, 0.1);
  border-color: rgba(210, 153, 34, 0.3);
  color: var(--yellow);
}

.tag-blue {
  background: rgba(88, 166, 255, 0.1);
  border-color: rgba(88, 166, 255, 0.3);
  color: var(--blue2);
}

.tag-green {
  background: rgba(63, 185, 80, 0.1);
  border-color: rgba(63, 185, 80, 0.3);
  color: var(--green3);
}

.ssi-ops {
  display: flex;
  gap: 4px;
}

.ssi-btn {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text3);
  border-radius: var(--r-sm);
  cursor: pointer;
  transition: all 0.12s;
}

.ssi-btn:hover {
  border-color: var(--border2);
  color: var(--text);
  background: color-mix(in srgb, var(--blue) 10%, transparent);
}

.ssi-btn-del:hover {
  border-color: rgba(248, 81, 73, 0.4);
  color: var(--red);
  background: rgba(248, 81, 73, 0.08);
}

.ssi-btn-pass:hover {
  border-color: rgba(63, 185, 80, 0.45);
  color: var(--green2);
  background: color-mix(in srgb, var(--green2) 14%, transparent);
}

.sd-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 24px;
  color: var(--text3);
  font-size: 12px;
}

.spin {
  animation: spin 0.8s linear infinite;
  display: inline-block;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
