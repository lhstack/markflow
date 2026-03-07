import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import request from '@/utils/request'

export interface SystemSettings {
  registration_enabled: boolean
  upload_max_bytes: number
  upload_max_mb: number
  updated_at?: string
}

const STORAGE_KEY = 'markflow.public_settings'
const DEFAULT_SETTINGS: SystemSettings = {
  registration_enabled: true,
  upload_max_bytes: 20 * 1024 * 1024,
  upload_max_mb: 20,
}

function normalizeSettings(raw: any): SystemSettings {
  const uploadMaxBytes = Number(raw?.upload_max_bytes)
  const uploadMaxMb = Number(raw?.upload_max_mb)

  return {
    registration_enabled: Boolean(raw?.registration_enabled ?? DEFAULT_SETTINGS.registration_enabled),
    upload_max_bytes: Number.isFinite(uploadMaxBytes) && uploadMaxBytes > 0
      ? uploadMaxBytes
      : DEFAULT_SETTINGS.upload_max_bytes,
    upload_max_mb: Number.isFinite(uploadMaxMb) && uploadMaxMb > 0
      ? uploadMaxMb
      : Math.max(1, Math.round((Number.isFinite(uploadMaxBytes) && uploadMaxBytes > 0
        ? uploadMaxBytes
        : DEFAULT_SETTINGS.upload_max_bytes) / 1024 / 1024)),
    updated_at: typeof raw?.updated_at === 'string' ? raw.updated_at : undefined,
  }
}

function loadCachedSettings(): SystemSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    return raw ? normalizeSettings(JSON.parse(raw)) : DEFAULT_SETTINGS
  } catch {
    return DEFAULT_SETTINGS
  }
}

function persistSettings(settings: SystemSettings) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings))
}

export const useSystemStore = defineStore('system', () => {
  const publicSettings = ref<SystemSettings>(loadCachedSettings())
  const adminSettings = ref<SystemSettings | null>(null)
  const loadingPublic = ref(false)
  const loadingAdmin = ref(false)

  const registrationEnabled = computed(() => publicSettings.value.registration_enabled)
  const uploadMaxBytes = computed(() => publicSettings.value.upload_max_bytes)
  const uploadMaxMb = computed(() => publicSettings.value.upload_max_mb)
  const uploadLimitLabel = computed(() => `${uploadMaxMb.value}MB`)

  function applyPublicSettings(raw: any) {
    const settings = normalizeSettings(raw)
    publicSettings.value = settings
    persistSettings(settings)
    return settings
  }

  async function fetchPublicSettings() {
    loadingPublic.value = true
    try {
      const data = await request.get('/auth/public-settings') as any
      return applyPublicSettings(data.settings)
    } finally {
      loadingPublic.value = false
    }
  }

  async function fetchAdminSettings() {
    loadingAdmin.value = true
    try {
      const data = await request.get('/admin/system-settings') as any
      const settings = applyPublicSettings(data.settings)
      adminSettings.value = settings
      return settings
    } finally {
      loadingAdmin.value = false
    }
  }

  async function updateAdminSettings(payload: { registration_enabled: boolean; upload_max_mb: number }) {
    const data = await request.put('/admin/system-settings', payload) as any
    const settings = applyPublicSettings(data.settings)
    adminSettings.value = settings
    return settings
  }

  return {
    publicSettings,
    adminSettings,
    loadingPublic,
    loadingAdmin,
    registrationEnabled,
    uploadMaxBytes,
    uploadMaxMb,
    uploadLimitLabel,
    fetchPublicSettings,
    fetchAdminSettings,
    updateAdminSettings,
  }
})
