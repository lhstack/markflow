// MCP 服务管理 composable。
//
// 从 AgentPanel 抽出：持有 MCP 全局设置、运行时能力、服务列表与当前草稿状态，
// 承载 draft 工厂、校验、payload 构建与全部 CRUD。请求与类型来自 @/api/agentMcp。
// 弹窗打开时调 refreshState()，其余交互直接调用暴露的动作函数。

import { computed, ref } from 'vue'
import { ElMessage } from 'element-plus'

import {
  createMcpDraft as buildMcpDraft,
  createMcpDraftFromDetail,
  deleteMcpServer as apiDeleteMcpServer,
  fetchMcpServer,
  fetchMcpServers,
  fetchMcpSettings,
  fetchMcpRuntimeCapabilities,
  normalizeMcpRuntimeCapabilities,
  normalizeMcpServerDetail,
  normalizeMcpServerSummary,
  normalizeOptionalTimestamp,
  runMcpDraftAction,
  saveMcpServer,
  updateMcpSettings,
  type McpDraft,
  type McpRuntimeCapabilities,
  type McpServerSummary,
  type McpSettingsState,
  type McpSnapshotTab,
  type McpTransport,
} from '@/api/agentMcp'
import { logAgentPanelError } from '@/components/agent/debug'

export function useMcpServers() {
  const loading = ref(false)
  const saving = ref(false)
  const testing = ref(false)
  const refreshing = ref(false)
  const settingsSaving = ref(false)
  const settings = ref<McpSettingsState>({ enabled: false, updatedAt: null })
  const settingsEnabled = ref(false)
  const runtimeCapabilities = ref<McpRuntimeCapabilities>({
    transports: ['sse', 'streamable-http'],
    stdioEnabled: false,
    stdioAllowedCommands: [],
  })
  const servers = ref<McpServerSummary[]>([])
  const draft = ref<McpDraft>(buildMcpDraft(servers.value.length))
  const activeSnapshotTab = ref<McpSnapshotTab>('tools')

  const isHttpTransport = computed(() => draft.value.transport !== 'stdio')
  const availableTransports = computed<McpTransport[]>(() => {
    const values = [...runtimeCapabilities.value.transports, draft.value.transport]
    const seen = new Set<McpTransport>()
    const result: McpTransport[] = []
    for (const value of values) {
      if (value && !seen.has(value)) {
        seen.add(value)
        result.push(value)
      }
    }
    return result
  })
  const activeSnapshotEntries = computed(() => {
    if (activeSnapshotTab.value === 'resources') return draft.value.resourcesSnapshot
    if (activeSnapshotTab.value === 'prompts') return draft.value.promptsSnapshot
    return draft.value.toolsSnapshot
  })

  function newDraft(seed = ''): McpDraft {
    const next = buildMcpDraft(servers.value.length, seed)
    if (!runtimeCapabilities.value.stdioEnabled && next.transport === 'stdio') {
      next.transport = 'sse'
    }
    return next
  }

  async function refreshState() {
    loading.value = true
    try {
      const [runtimeData, settingsData, listData] = await Promise.all([
        fetchMcpRuntimeCapabilities(),
        fetchMcpSettings(),
        fetchMcpServers(),
      ])

      runtimeCapabilities.value = normalizeMcpRuntimeCapabilities(runtimeData)
      settings.value = {
        enabled: settingsData.settings?.enabled === true,
        updatedAt: normalizeOptionalTimestamp(settingsData.settings?.updated_at ?? null),
      }
      settingsEnabled.value = settings.value.enabled
      servers.value = (listData.servers || [])
        .map((server) => normalizeMcpServerSummary(server))
        .filter((server): server is McpServerSummary => Boolean(server))

      if (draft.value.id && servers.value.some((server) => server.id === draft.value.id)) {
        await editServer(draft.value.id, { silent: true })
        return
      }
      if (servers.value[0]) {
        await editServer(servers.value[0].id, { silent: true })
        return
      }

      draft.value = newDraft()
      activeSnapshotTab.value = 'tools'
    } catch (error: any) {
      logAgentPanelError('refresh_mcp_state', error)
      ElMessage.error(error.response?.data?.error || error.message || '加载 MCP 配置失败')
    } finally {
      loading.value = false
    }
  }

  async function editServer(serverId: string, options: { silent?: boolean } = {}) {
    if (!serverId) return
    try {
      const data = await fetchMcpServer(serverId)
      const detail = normalizeMcpServerDetail(data.server)
      if (!detail) throw new Error('MCP 服务详情缺失')
      draft.value = createMcpDraftFromDetail(detail)
      activeSnapshotTab.value = 'tools'
    } catch (error: any) {
      logAgentPanelError('edit_mcp_server', error, { serverId })
      if (!options.silent) {
        ElMessage.error(error.response?.data?.error || error.message || '读取 MCP 服务详情失败')
      }
    }
  }

  function buildSecretLinesPayload(mode: McpDraft['customHeadersMode'], text: string, options: { allowKeep?: boolean } = {}) {
    if (mode === 'keep' && options.allowKeep !== false) return { mode: 'keep' }
    if (mode === 'keep') {
      return { mode: 'replace', value: text.split('\n').map((line) => line.trim()).filter(Boolean) }
    }
    if (mode === 'clear') return { mode: 'clear' }
    return { mode: 'replace', value: text.split('\n').map((line) => line.trim()).filter(Boolean) }
  }

  function validateDraft(options: { requireCompleteConnection?: boolean } = {}): string {
    const requireCompleteConnection = options.requireCompleteConnection !== false
    const current = draft.value
    if (!current.name.trim()) return '请填写 MCP 名称'

    if (current.transport === 'stdio') {
      if (!runtimeCapabilities.value.stdioEnabled) return '当前后端没有开启 stdio'
      if (requireCompleteConnection && !current.command.trim()) return '请填写 STDIO Command'
      return ''
    }

    if (requireCompleteConnection && !current.url.trim()) return '请填写 HTTP URL'

    if (current.authType === 'bearer' && !current.bearerToken.trim() && !current.authSecretFlags.token) {
      return '请填写 Bearer Token'
    }
    if (current.authType === 'basic') {
      if (!current.authUsername.trim()) return '请填写 Basic Auth 用户名'
      if (!current.authPassword.trim() && !current.authSecretFlags.password) return '请填写 Basic Auth 密码'
    }
    if (current.authType === 'header') {
      if (!current.authHeaderName.trim()) return '请填写 Header 名称'
      if (!current.authHeaderValue.trim() && !current.authSecretFlags.value) return '请填写 Header 值'
    }
    if (current.authType === 'query') {
      if (!current.authQueryName.trim()) return '请填写 Query 参数名'
      if (!current.authQueryValue.trim() && !current.authSecretFlags.value) return '请填写 Query 参数值'
    }
    return ''
  }

  function normalizePersistedDraftId(value: string | null | undefined): string | null {
    if (!value) return null
    const numeric = Number(value)
    if (!Number.isFinite(numeric) || numeric <= 0) return null
    return `${numeric}`
  }

  function buildAuthPayload(source: McpDraft, options: { allowKeep?: boolean } = {}) {
    const allowKeep = options.allowKeep !== false
    if (source.transport === 'stdio' || source.authType === 'none') return { mode: 'clear' }

    if (source.authType === 'bearer') {
      return {
        mode: 'replace',
        value: {
          type: 'bearer',
          scheme: source.bearerScheme.trim() || null,
          token: source.bearerToken.trim()
            ? { mode: 'replace', value: source.bearerToken.trim() }
            : source.authSecretFlags.token && allowKeep ? { mode: 'keep' } : { mode: 'clear' },
        },
      }
    }
    if (source.authType === 'basic') {
      return {
        mode: 'replace',
        value: {
          type: 'basic',
          username: source.authUsername.trim(),
          password: source.authPassword.trim()
            ? { mode: 'replace', value: source.authPassword.trim() }
            : source.authSecretFlags.password && allowKeep ? { mode: 'keep' } : { mode: 'clear' },
        },
      }
    }
    if (source.authType === 'header') {
      return {
        mode: 'replace',
        value: {
          type: 'header',
          name: source.authHeaderName.trim(),
          value: source.authHeaderValue.trim()
            ? { mode: 'replace', value: source.authHeaderValue.trim() }
            : source.authSecretFlags.value && allowKeep ? { mode: 'keep' } : { mode: 'clear' },
        },
      }
    }
    return {
      mode: 'replace',
      value: {
        type: 'query',
        name: source.authQueryName.trim(),
        value: source.authQueryValue.trim()
          ? { mode: 'replace', value: source.authQueryValue.trim() }
          : source.authSecretFlags.value && allowKeep ? { mode: 'keep' } : { mode: 'clear' },
      },
    }
  }

  function buildDraftPayload(source: McpDraft = draft.value, options: { allowKeep?: boolean } = {}) {
    const persistedId = normalizePersistedDraftId(source.id)
    const allowKeep = options.allowKeep !== false && Boolean(persistedId)
    const isHttp = source.transport !== 'stdio'
    const normalizedUrl = source.url.trim()
    const normalizedCommand = source.command.trim()
    return {
      id: persistedId ? Number(persistedId) : null,
      name: source.name.trim(),
      enabled: source.enabled,
      transport: source.transport,
      url: isHttp ? (normalizedUrl || null) : null,
      command: source.transport === 'stdio' ? (normalizedCommand || null) : null,
      args: source.transport === 'stdio'
        ? source.argsText.split('\n').map((line) => line.trim()).filter(Boolean)
        : [],
      auth: buildAuthPayload(source, { allowKeep }),
      custom_headers: isHttp
        ? buildSecretLinesPayload(source.customHeadersMode, source.customHeadersText, { allowKeep })
        : { mode: 'clear' },
      stdio_env: source.transport === 'stdio'
        ? buildSecretLinesPayload(source.stdioEnvMode, source.stdioEnvText, { allowKeep })
        : { mode: 'clear' },
    }
  }

  async function createPersistedServer(source: McpDraft, successMessage: string) {
    saving.value = true
    try {
      const data = await saveMcpServer(buildDraftPayload(source, { allowKeep: false }))
      const detail = normalizeMcpServerDetail(data.server)
      if (!detail) throw new Error('MCP 创建结果缺失')
      draft.value = createMcpDraftFromDetail(detail)
      await refreshState()
      ElMessage.success(successMessage)
    } catch (error: any) {
      logAgentPanelError('create_mcp_server', error, { sourceId: source.id, transport: source.transport })
      ElMessage.error(error.response?.data?.error || error.message || '创建 MCP 服务失败')
    } finally {
      saving.value = false
    }
  }

  async function startCreate() {
    const next = newDraft()
    next.enabled = false
    await createPersistedServer(next, '已新增 MCP 服务')
  }

  async function duplicate() {
    const current = draft.value
    const next = newDraft(current.name ? `${current.name} 副本` : '')
    next.enabled = false
    next.transport = current.transport === 'stdio' && !runtimeCapabilities.value.stdioEnabled ? 'sse' : current.transport
    next.url = current.url
    next.command = current.command
    next.argsText = current.argsText
    next.authType = current.authType
    next.bearerScheme = current.bearerScheme
    next.bearerToken = current.bearerToken
    next.authUsername = current.authUsername
    next.authPassword = current.authPassword
    next.authHeaderName = current.authHeaderName
    next.authHeaderValue = current.authHeaderValue
    next.authQueryName = current.authQueryName
    next.authQueryValue = current.authQueryValue
    next.customHeadersText = current.customHeadersText
    next.stdioEnvText = current.stdioEnvText
    next.authSecretFlags = { token: false, password: false, value: false }
    await createPersistedServer(next, '已复制 MCP 服务')
  }

  async function handleSettingsChange(value: string | number | boolean) {
    const enabled = Boolean(value)
    const previous = settings.value.enabled
    settingsSaving.value = true
    try {
      const data = await updateMcpSettings(enabled)
      settings.value = {
        enabled: data.settings?.enabled === true,
        updatedAt: normalizeOptionalTimestamp(data.settings?.updated_at ?? null),
      }
      settingsEnabled.value = settings.value.enabled
      ElMessage.success(settings.value.enabled ? '已启用 MCP' : '已关闭 MCP')
    } catch (error: any) {
      settingsEnabled.value = previous
      logAgentPanelError('save_mcp_settings', error)
      ElMessage.error(error.response?.data?.error || error.message || '保存 MCP 开关失败')
    } finally {
      settingsSaving.value = false
    }
  }

  async function saveDraft() {
    const validationMessage = validateDraft({ requireCompleteConnection: draft.value.enabled })
    if (validationMessage) {
      ElMessage.warning(validationMessage)
      return
    }
    saving.value = true
    try {
      const data = await saveMcpServer(buildDraftPayload())
      const detail = normalizeMcpServerDetail(data.server)
      if (!detail) throw new Error('MCP 保存结果缺失')
      await refreshState()
      draft.value = createMcpDraftFromDetail(detail)
      ElMessage.success('MCP 服务已保存')
    } catch (error: any) {
      logAgentPanelError('save_mcp_server', error, { serverId: draft.value.id, transport: draft.value.transport })
      ElMessage.error(error.response?.data?.error || error.message || '保存 MCP 服务失败')
    } finally {
      saving.value = false
    }
  }

  async function removeServer(serverId: string | null) {
    if (!serverId) return
    try {
      await apiDeleteMcpServer(serverId)
      await refreshState()
      ElMessage.success('MCP 服务已删除')
    } catch (error: any) {
      logAgentPanelError('remove_mcp_server', error, { serverId })
      ElMessage.error(error.response?.data?.error || error.message || '删除 MCP 服务失败')
    }
  }

  async function runAction(endpoint: 'test' | 'refresh') {
    const validationMessage = validateDraft({ requireCompleteConnection: true })
    if (validationMessage) {
      ElMessage.warning(validationMessage)
      return
    }
    const loadingRef = endpoint === 'test' ? testing : refreshing
    loadingRef.value = true
    try {
      const data = await runMcpDraftAction(endpoint, buildDraftPayload())
      const detail = normalizeMcpServerDetail(data.server)
      if (!detail) throw new Error('MCP 刷新结果缺失')
      draft.value = createMcpDraftFromDetail(detail)
      ElMessage.success(endpoint === 'test' ? 'MCP 测试连接成功' : 'MCP 能力已刷新')
    } catch (error: any) {
      const server = normalizeMcpServerDetail(error?.response?.data?.server)
      if (server) draft.value = createMcpDraftFromDetail(server)
      logAgentPanelError(`mcp_${endpoint}`, error, { serverId: draft.value.id, transport: draft.value.transport })
      ElMessage.error(error.response?.data?.error || error.message || (endpoint === 'test' ? '测试连接失败' : '刷新 MCP 能力失败'))
    } finally {
      loadingRef.value = false
    }
  }

  return {
    loading,
    saving,
    testing,
    refreshing,
    settingsSaving,
    settingsEnabled,
    runtimeCapabilities,
    servers,
    draft,
    activeSnapshotTab,
    isHttpTransport,
    availableTransports,
    activeSnapshotEntries,
    refreshState,
    editServer,
    startCreate,
    duplicate,
    handleSettingsChange,
    saveDraft,
    removeServer,
    testConnection: () => runAction('test'),
    refreshCapabilities: () => runAction('refresh'),
  }
}
