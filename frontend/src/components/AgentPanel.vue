<template>
  <div
    v-if="mounted"
    class="agent-panel"
    :class="{ collapsed }"
    :style="panelStyle"
  >
    <button
      v-if="collapsed"
      class="agent-fab"
      @mousedown.stop="startDrag"
      @click="handleFabClick"
    >
      AI 助手
    </button>

    <div v-else class="agent-shell">
      <header class="agent-header" @mousedown="startDrag">
        <div class="agent-header-content">
          <div class="agent-meta-line">
            <span class="agent-meta-label">供应方：</span>
            <span class="agent-meta-value">{{ activeProvider?.name || '未配置供应商' }}</span>
            <span class="agent-meta-divider">|</span>
            <span class="agent-meta-label">Base URL:</span>
            <span class="agent-meta-value agent-meta-url">
              {{ activeProvider ? normalizedProviderBaseUrl(activeProvider.baseUrl) : DEFAULT_BASE_URL }}
            </span>
          </div>
          <div class="agent-toolbar-line" @mousedown.stop>
            <div class="agent-session-line">
              <span class="agent-meta-label">会话：</span>
              <el-select
                v-model="currentSessionId"
                class="agent-session-select"
                size="small"
                placeholder="选择会话"
              >
                <el-option
                  v-for="session in sessions"
                  :key="session.id"
                  :label="session.title"
                  :value="session.id"
                >
                  <div class="session-select-option">
                    <div class="session-select-info">
                      <span class="session-select-title">{{ session.title }}</span>
                      <span class="session-select-meta">{{ session.model || '未选模型' }}</span>
                    </div>
                    <div class="session-select-actions">
                      <span class="session-select-time">{{ formatSessionTime(session.updatedAt) }}</span>
                      <span
                        v-if="sessions.length > 1"
                        class="session-select-delete"
                        @click.stop.prevent="deleteSession(session.id)"
                      >
                        删除
                      </span>
                    </div>
                  </div>
                </el-option>
              </el-select>
            </div>

            <div class="agent-header-actions">
              <el-tooltip content="新建会话" placement="top">
                <el-button class="agent-header-icon" :icon="Plus" circle @click="createSession" />
              </el-tooltip>
              <el-tooltip content="供应商管理" placement="top">
                <el-button class="agent-header-icon" :icon="Setting" circle @click="openProviderDialog" />
              </el-tooltip>
              <el-tooltip content="MCP 管理" placement="top">
                <el-button class="agent-header-icon" :icon="Connection" circle @click="openMcpDialog" />
              </el-tooltip>
              <button class="header-btn" title="清空当前会话" @click="clearCurrentSession">清空</button>
              <button class="header-btn" title="收起" @click="toggleCollapse(true)">收起</button>
            </div>
          </div>
        </div>
      </header>

      <section class="agent-main">
        <div class="agent-workspace">
          <div ref="messagesRef" class="agent-messages">
            <template v-if="visibleMessages.length">
              <article
                v-for="message in visibleMessages"
                :key="message.id"
                class="agent-message"
                :class="message.role"
              >
                <div class="message-role">{{ message.role === 'user' ? '用户' : '助手' }}</div>

                <details
                  v-if="message.role === 'assistant' && displayedMessageReasoning(message)"
                  class="message-reasoning"
                >
                  <summary>推理</summary>
                  <pre class="message-reasoning-content">{{ displayedMessageReasoning(message) }}</pre>
                </details>

                <div v-if="message.attachments?.length" class="message-attachments">
                  <div
                    v-for="attachment in message.attachments"
                    :key="`${message.id}-${attachment.uploadId}`"
                    class="message-attachment-chip"
                    @click="openAttachment(attachment)"
                  >
                    <span class="message-attachment-name">{{ attachment.name }}</span>
                    <span class="message-attachment-meta">{{ attachment.kind === 'image' ? '图片' : '文件' }}</span>
                  </div>
                </div>

                <pre
                  v-if="displayedMessageContent(message) || (streaming && message.role === 'assistant')"
                  class="message-content"
                >{{ displayedMessageContent(message) || '...' }}</pre>
              </article>
            </template>

            <div v-else class="agent-empty">
              <div class="agent-empty-title">可以直接开始对话或写文档</div>
              <div class="agent-empty-desc">直接描述你的目标即可，模型会判断是答复、续写、改写，或在兼容接口上继续流式返回结果。</div>
            </div>
          </div>

          <div class="agent-composer">
            <div class="agent-mode-tip">{{ modeTip }}</div>
            <textarea
              v-model="prompt"
              class="agent-textarea"
              :placeholder="textareaPlaceholder"
              :disabled="streaming"
              @keydown="handleComposerKeydown"
              @paste="handleComposerPaste"
            />

          <input
            ref="attachmentInputRef"
            class="agent-hidden-input"
            type="file"
            :accept="composerAttachmentAccept"
            multiple
            :disabled="streaming || attachmentUploading"
            @change="handleAttachmentInputChange"
          />

          <div v-if="composerAttachmentEnabled || composerAttachments.length" class="agent-attachment-panel">
            <div class="agent-attachment-head">
              <div v-if="composerAttachments.length" class="agent-attachment-list">
                <div
                  v-for="attachment in composerAttachments"
                  :key="attachment.localId"
                  class="agent-attachment-item"
                  @click="openAttachment(attachment)"
                >
                  <div class="agent-attachment-item-main">
                    <span class="agent-attachment-item-name">{{ attachment.name }}</span>
                  </div>
                  <span class="agent-attachment-item-meta">
                    {{ attachment.kind === 'image' ? '图片' : '文件' }} · {{ formatAttachmentSize(attachment.size) }}
                  </span>
                  <button
                    class="agent-attachment-remove"
                    :disabled="streaming || attachmentUploading"
                    @click.stop="removeComposerAttachment(attachment.localId)"
                  >
                    移除
                  </button>
                </div>
              </div>

              <button
                v-if="composerAttachmentEnabled"
                class="agent-attachment-trigger"
                :disabled="streaming || attachmentUploading"
                @click="openAttachmentPicker"
              >
                {{ attachmentUploading ? '上传中...' : '选择附件' }}
              </button>
            </div>
          </div>

            <div class="agent-composer-footer">
            <div class="agent-shortcut-tip">
              Ctrl+Enter / Cmd+Enter 发送，Enter 换行<span v-if="composerAttachmentEnabled">，支持粘贴并上传{{ composerSupportsImage && composerSupportsDocument ? '图片和文件' : composerSupportsImage ? '图片' : '文件' }}</span>
            </div>

            <div class="agent-bottom-bar">
              <div class="agent-controls">
                <div class="agent-inline-selects">
                  <div class="agent-select-row">
                    <span class="agent-control-label">供应方：</span>
                    <el-select
                      v-model="selectedProviderId"
                      class="agent-provider-select"
                      size="small"
                      placeholder="选择供应商"
                    >
                      <el-option
                        v-for="provider in providers"
                        :key="provider.id"
                        :label="provider.name"
                        :value="provider.id"
                      />
                    </el-select>
                  </div>

                  <div class="agent-select-row">
                    <span class="agent-control-label">模型：</span>
                    <el-select
                      v-model="currentSessionModel"
                      class="agent-model-select"
                      size="small"
                      filterable
                      allow-create
                      default-first-option
                      :reserve-keyword="false"
                      :disabled="!currentSession || !activeProvider"
                      placeholder="选择或添加模型"
                    >
                      <el-option
                        v-for="model in activeModelOptions"
                        :key="model"
                        :label="model"
                        :value="model"
                      />
                    </el-select>
                  </div>

                  <div class="agent-select-row">
                    <span class="agent-control-label">模式：</span>
                    <el-select
                      v-model="currentSessionTransportMode"
                      class="agent-model-select"
                      size="small"
                      :disabled="!currentSession"
                    >
                      <el-option label="自动" value="auto" />
                      <el-option label="Responses" value="responses" />
                      <el-option label="Chat" value="chat" />
                    </el-select>
                  </div>
                </div>
              </div>

              <div class="agent-action-row">
                <el-tooltip content="模型管理" placement="top">
                  <el-button
                    class="agent-icon-btn"
                    :icon="Setting"
                    :disabled="!activeProvider"
                    circle
                    @click="showModelDialog = true"
                  />
                </el-tooltip>
                <button class="primary-action" @click="streaming ? stopStreaming() : sendMessage()">
                  {{ streaming ? '停止' : '发送' }}
                </button>
              </div>
            </div>
            </div>
          </div>
        </div>
      </section>
    </div>

    <el-dialog v-model="showProviderDialog" class="agent-dialog provider-dialog" title="供应商配置" width="820px" append-to-body destroy-on-close>
      <div class="provider-manager">
        <aside class="provider-list-pane">
          <div class="provider-list">
            <button
              v-for="provider in providers"
              :key="provider.id"
              class="provider-item"
              :class="{ active: provider.id === providerDraft.id }"
              @click="editProvider(provider.id)"
              @dblclick="activateProvider(provider.id)"
            >
              <span class="provider-item-name">
                {{ provider.name }}
                <span v-if="provider.id === activeProviderId" class="provider-item-tag">已激活</span>
              </span>
              <span class="provider-item-meta">{{ providerKindLabel(provider.providerKind) }} · {{ normalizedProviderBaseUrl(provider.baseUrl) }}</span>
            </button>
          </div>
        </aside>

        <section class="provider-editor">
          <el-input v-model="providerDraft.name" placeholder="供应商名称，例如 OpenAI 官方" />
          <el-select v-model="providerDraft.providerKind" placeholder="供应商类型">
            <el-option label="OpenAI / Compatible" value="openai" />
            <el-option label="Anthropic / Claude" value="anthropic" />
            <el-option label="Gemini" value="gemini" />
          </el-select>
          <el-input v-model="providerDraft.baseUrl" placeholder="Base URL，例如 https://api.openai.com/v1" />
          <el-input v-model="providerDraft.apiKey" type="password" show-password placeholder="API Key" />
          <div class="provider-hint">
            <div>{{ providerDraftPreset.summary }}</div>
            <div>默认 Base URL：{{ providerDraftPreset.baseUrl }}</div>
            <div>认证方式：{{ providerDraftPreset.auth }}</div>
            <div>拉取模型：{{ providerDraftPreset.modelsApi }}</div>
            <div>常见模型：{{ providerDraftPreset.models.join('、') }}</div>
            <div>供应商配置会持久化到后端。保存后请到“模型管理”里配置可选模型。</div>
          </div>
        </section>
      </div>
      <template #footer>
        <el-button @click="showProviderDialog = false">关闭</el-button>
        <el-button @click="startCreateProvider">新增</el-button>
        <el-button :disabled="!providerDraft.id" @click="activateProvider(providerDraft.id)">设为激活</el-button>
        <el-button type="danger" plain :disabled="!providerDraft.id" @click="removeProvider(providerDraft.id)">删除</el-button>
        <el-button type="primary" @click="saveProviderDraft">保存</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showMcpDialog" class="agent-dialog provider-dialog mcp-dialog" title="MCP 配置" width="840px" append-to-body destroy-on-close align-center>
      <div class="provider-manager">
        <aside class="provider-list-pane">
          <div class="mcp-settings-card">
            <div class="mcp-settings-copy">
              <div class="mcp-settings-title">启用 MCP</div>
              <div class="mcp-settings-desc">开启后，聊天会把已启用的 MCP 服务注入到当前工具链。</div>
            </div>
            <el-switch
              v-model="mcpSettingsEnabled"
              :loading="mcpSettingsSaving"
              inline-prompt
              active-text="开"
              inactive-text="关"
              @change="handleMcpSettingsChange"
            />
          </div>

          <div class="provider-list">
            <button
              v-for="server in mcpServers"
              :key="server.id"
              class="provider-item mcp-item"
              :class="{ active: server.id === mcpDraft.id }"
              @click="editMcpServer(server.id)"
            >
              <span class="provider-item-name">
                {{ server.name }}
                <span v-if="server.enabled" class="provider-item-tag">已启用</span>
              </span>
              <span class="provider-item-meta">{{ mcpTransportLabel(server.transport) }} · {{ mcpStatusLabel(server.lastStatus) }}</span>
              <span v-if="server.lastError" class="mcp-item-error">{{ server.lastError }}</span>
            </button>

            <div v-if="!mcpServers.length && !mcpLoading" class="provider-hint">还没有 MCP 服务，点击“新增”开始配置。</div>
            <div v-else-if="mcpLoading" class="provider-hint">正在加载 MCP 配置…</div>
          </div>

          <div class="mcp-runtime-hint">
            <div>可用传输：{{ availableMcpTransports.length ? availableMcpTransports.map((item) => mcpTransportLabel(item)).join(' / ') : '加载中' }}</div>
            <div v-if="mcpRuntimeCapabilities.stdioEnabled">
              允许的 STDIO 命令：{{ mcpRuntimeCapabilities.stdioAllowedCommands.length ? mcpRuntimeCapabilities.stdioAllowedCommands.join('、') : '未限制' }}
            </div>
            <div v-else>当前后端未开启 `stdio`，因此不会显示对应配置项。</div>
          </div>
        </aside>

        <section class="provider-editor mcp-editor">
          <div class="mcp-form-grid">
            <label class="mcp-field">
              <span class="mcp-field-label">名称</span>
              <el-input v-model="mcpDraft.name" placeholder="例如 文档知识库 / 项目检索 / GitHub MCP" />
            </label>

            <label class="mcp-field">
              <span class="mcp-field-label">启用</span>
              <div class="mcp-field-inline">
                <el-switch v-model="mcpDraft.enabled" inline-prompt active-text="开" inactive-text="关" />
                <span class="mcp-field-help">关闭后仍会保留配置，但不会注入到聊天工具链。</span>
              </div>
            </label>

            <label class="mcp-field">
              <span class="mcp-field-label">传输</span>
              <el-select v-model="mcpDraft.transport" placeholder="选择传输方式">
                <el-option
                  v-for="transport in availableMcpTransports"
                  :key="transport"
                  :label="mcpTransportLabel(transport)"
                  :value="transport"
                />
              </el-select>
            </label>

            <template v-if="mcpIsHttpTransport">
              <label class="mcp-field mcp-field-full">
                <span class="mcp-field-label">HTTP URL</span>
                <el-input
                  v-model="mcpDraft.url"
                  :placeholder="mcpDraft.transport === 'sse' ? 'https://example.com/mcp/sse' : 'https://example.com/mcp'"
                />
              </label>

              <label class="mcp-field">
                <span class="mcp-field-label">认证方式</span>
                <el-select v-model="mcpDraft.authType">
                  <el-option label="无认证" value="none" />
                  <el-option label="Bearer Token" value="bearer" />
                  <el-option label="Basic Auth" value="basic" />
                  <el-option label="Header" value="header" />
                  <el-option label="Query" value="query" />
                </el-select>
              </label>

              <template v-if="mcpDraft.authType === 'bearer'">
                <label class="mcp-field">
                  <span class="mcp-field-label">Scheme</span>
                  <el-input v-model="mcpDraft.bearerScheme" placeholder="默认 Bearer，可改成 Token 等" />
                </label>
                <label class="mcp-field">
                  <span class="mcp-field-label">Token</span>
                  <el-input v-model="mcpDraft.bearerToken" type="password" show-password placeholder="Bearer Token" />
                </label>
              </template>

              <template v-else-if="mcpDraft.authType === 'basic'">
                <label class="mcp-field">
                  <span class="mcp-field-label">用户名</span>
                  <el-input v-model="mcpDraft.authUsername" placeholder="basic auth 用户名" />
                </label>
                <label class="mcp-field">
                  <span class="mcp-field-label">密码</span>
                  <el-input v-model="mcpDraft.authPassword" type="password" show-password placeholder="Basic Auth 密码" />
                </label>
              </template>

              <template v-else-if="mcpDraft.authType === 'header'">
                <label class="mcp-field">
                  <span class="mcp-field-label">Header 名称</span>
                  <el-input v-model="mcpDraft.authHeaderName" placeholder="例如 Authorization / X-Api-Key" />
                </label>
                <label class="mcp-field">
                  <span class="mcp-field-label">Header 值</span>
                  <el-input v-model="mcpDraft.authHeaderValue" type="password" show-password placeholder="Header 值" />
                </label>
              </template>

              <template v-else-if="mcpDraft.authType === 'query'">
                <label class="mcp-field">
                  <span class="mcp-field-label">Query 参数名</span>
                  <el-input v-model="mcpDraft.authQueryName" placeholder="例如 api_key / token" />
                </label>
                <label class="mcp-field">
                  <span class="mcp-field-label">Query 参数值</span>
                  <el-input v-model="mcpDraft.authQueryValue" type="password" show-password placeholder="Query 参数值" />
                </label>
              </template>

              <div class="mcp-secret-block mcp-field-full">
                <div class="mcp-secret-head">
                  <div>
                    <div class="mcp-field-label">自定义 Headers</div>
                    <div class="mcp-field-help">按 `KEY: VALUE` 每行一条。保存时会和认证信息一起发给 MCP 服务。</div>
                  </div>
                  <el-select v-model="mcpDraft.customHeadersMode" class="mcp-secret-mode-select">
                    <el-option label="保留当前" value="keep" />
                    <el-option label="整体替换" value="replace" />
                    <el-option label="清空" value="clear" />
                  </el-select>
                </div>

                <el-input
                  v-if="mcpDraft.customHeadersMode === 'replace'"
                  v-model="mcpDraft.customHeadersText"
                  type="textarea"
                  :rows="5"
                  placeholder="例如&#10;X-Workspace: markflow&#10;X-Trace-Id: local-dev"
                />
              </div>
            </template>

            <template v-else>
              <label class="mcp-field">
                <span class="mcp-field-label">STDIO Command</span>
                <el-input v-model="mcpDraft.command" placeholder="例如 npx / uvx / node / python" />
              </label>

              <label class="mcp-field">
                <span class="mcp-field-label">允许范围</span>
                <div class="mcp-field-inline">
                  <span class="mcp-field-help">
                    {{ mcpRuntimeCapabilities.stdioAllowedCommands.length ? `当前允许：${mcpRuntimeCapabilities.stdioAllowedCommands.join('、')}` : '当前未配置命令白名单' }}
                  </span>
                </div>
              </label>

              <label class="mcp-field mcp-field-full">
                <span class="mcp-field-label">STDIO Args</span>
                <el-input
                  v-model="mcpDraft.argsText"
                  type="textarea"
                  :rows="4"
                  placeholder="每行一个参数，例如&#10;-y&#10;@modelcontextprotocol/server-filesystem&#10;/path/to/workspace"
                />
              </label>

              <div class="mcp-secret-block mcp-field-full">
                <div class="mcp-secret-head">
                  <div>
                    <div class="mcp-field-label">STDIO Env</div>
                    <div class="mcp-field-help">按 `KEY=VALUE` 每行一条。敏感值会加密后持久化到后端。</div>
                  </div>
                  <el-select v-model="mcpDraft.stdioEnvMode" class="mcp-secret-mode-select">
                    <el-option label="保留当前" value="keep" />
                    <el-option label="整体替换" value="replace" />
                    <el-option label="清空" value="clear" />
                  </el-select>
                </div>

                <el-input
                  v-if="mcpDraft.stdioEnvMode === 'replace'"
                  v-model="mcpDraft.stdioEnvText"
                  type="textarea"
                  :rows="5"
                  placeholder="例如&#10;OPENAI_API_KEY=xxxx&#10;GITHUB_TOKEN=yyyy"
                />
              </div>
            </template>
          </div>

          <div class="mcp-status-card">
            <div class="mcp-status-row">
              <span class="mcp-status-label">状态</span>
              <span class="mcp-status-value">{{ mcpStatusLabel(mcpDraft.lastStatus) }}</span>
            </div>
            <div class="mcp-status-row">
              <span class="mcp-status-label">配置版本</span>
              <span class="mcp-status-value">{{ mcpDraft.configVersion || '-' }}</span>
            </div>
            <div class="mcp-status-row">
              <span class="mcp-status-label">最近同步</span>
              <span class="mcp-status-value">{{ formatOptionalTimestamp(mcpDraft.lastSyncAt) }}</span>
            </div>
            <div v-if="mcpDraft.lastError" class="mcp-status-error">{{ mcpDraft.lastError }}</div>
          </div>

          <div class="mcp-snapshot-card">
            <el-tabs v-model="activeMcpSnapshotTab" class="mcp-snapshot-tabs">
              <el-tab-pane label="Tools" name="tools" />
              <el-tab-pane label="Resources" name="resources" />
              <el-tab-pane label="Prompts" name="prompts" />
            </el-tabs>

            <div v-if="activeMcpSnapshotEntries.length" class="mcp-snapshot-list">
              <article
                v-for="(entry, index) in activeMcpSnapshotEntries"
                :key="`${activeMcpSnapshotTab}-${index}`"
                class="mcp-snapshot-item"
              >
                <div class="mcp-snapshot-title">{{ mcpSnapshotEntryTitle(entry, index) }}</div>
                <pre class="mcp-snapshot-raw">{{ mcpSnapshotEntryDetail(entry) }}</pre>
              </article>
            </div>
            <div v-else class="provider-hint">还没有能力快照。可以直接测试当前表单参数，确认没问题后再保存。</div>
          </div>
        </section>
      </div>
      <template #footer>
        <el-button @click="startCreateMcpServer">新增</el-button>
        <el-button :disabled="!mcpDraft.id" @click="duplicateMcpServer">复制</el-button>
        <el-button type="danger" plain :disabled="!mcpDraft.id" @click="removeMcpServer(mcpDraft.id)">删除</el-button>
        <el-button :loading="mcpTesting" @click="testMcpServerConnection">测试连接</el-button>
        <el-button :loading="mcpRefreshing" @click="refreshMcpServerCapabilities">刷新能力</el-button>
        <el-button type="primary" :loading="mcpSaving" @click="saveMcpServerDraft">保存</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showModelDialog" class="agent-dialog model-dialog" title="模型管理" width="760px" append-to-body destroy-on-close>
      <div v-if="activeProvider" class="model-manager">
        <div class="model-header">
          <div>
            <div class="model-header-title">{{ activeProvider.name }}</div>
            <div class="model-header-meta">{{ normalizedProviderBaseUrl(activeProvider.baseUrl) }}</div>
          </div>
          <el-button :loading="modelLoading" @click="fetchProviderModels">同步远端模型</el-button>
        </div>

        <div class="provider-hint">左侧是当前可用模型，右侧是 SDK 返回模型。勾选右侧模型后，当前会话选择框就能直接使用；左侧每个模型上的“参数”按钮可打开完整模型参数配置。</div>

        <div class="model-grid">
          <section class="model-pane">
            <div class="model-pane-head">
              <div class="model-section-title">当前模型列表</div>
            </div>

            <div class="model-pane-body model-pane-scroll">
              <div v-if="currentManagedModels.length" class="current-model-list">
                <div
                  v-for="entry in currentManagedModels"
                  :key="entry.id"
                  class="current-model-item"
                >
                  <div class="current-model-main">
                    <span class="current-model-name">{{ entry.id }}</span>
                    <span v-if="entry.isCustom" class="model-source-badge is-custom">自定义</span>
                    <span v-else-if="entry.isRemote" class="model-source-badge">SDK</span>
                    <el-tooltip v-if="entry.isConfigured" content="已配置模型参数" placement="top">
                      <span class="model-configured-dot" aria-label="已配置模型参数" />
                    </el-tooltip>
                  </div>
                  <div class="current-model-actions">
                    <el-button class="model-action-button" text @click="openModelConfigDialog(entry.id)">
                      参数
                    </el-button>
                    <el-button
                      link
                      type="danger"
                      @click="entry.isCustom ? removeCustomModel(entry.id) : toggleCustomModel(entry.id, false)"
                    >
                      移除
                    </el-button>
                  </div>
                </div>
              </div>
              <div v-else class="model-empty">还没有可用模型。</div>
            </div>

            <div class="model-pane-foot">
              <div class="model-custom-row">
                <el-input
                  v-model="customModelInput"
                  placeholder="新增自定义模型，例如 gpt-4.1 或 qwen-plus"
                  @keydown.enter.prevent="addCustomModel"
                />
                <el-button @click="addCustomModel">新增</el-button>
              </div>
            </div>
          </section>

          <section class="model-pane">
            <div class="model-pane-head model-pane-head-row">
              <div class="model-section-title">SDK 模型列表</div>
              <el-input
                v-model="modelSearchQuery"
                class="model-search-input"
                clearable
                placeholder="搜索模型 ID"
              />
            </div>

            <div class="model-pane-body model-pane-scroll">
              <el-checkbox-group v-model="modelDraft.enabledModels" class="model-check-list">
                <label
                  v-for="model in filteredRemoteModels"
                  :key="model"
                  class="model-check-item"
                >
                  <el-checkbox :value="model">{{ model }}</el-checkbox>
                </label>
              </el-checkbox-group>
              <div v-if="!modelDraft.remoteModels.length" class="model-empty">还没有拉取到远端模型。</div>
              <div v-else-if="!filteredRemoteModels.length" class="model-empty">没有匹配的模型。</div>
            </div>
          </section>
        </div>
      </div>

      <div v-else class="model-empty-state">
        <div class="model-empty">请先在供应商配置里新增并激活一个供应商。</div>
        <el-button type="primary" @click="openProviderManagerFromModelDialog">去配置供应商</el-button>
      </div>

      <template #footer>
        <el-button @click="showModelDialog = false">取消</el-button>
        <el-button type="primary" :disabled="!activeProvider" @click="saveModelDraft">保存</el-button>
      </template>
    </el-dialog>

    <el-dialog
      v-model="showModelConfigDialog"
      class="agent-dialog model-config-dialog"
      :title="editingModelConfigName ? `模型参数 · ${editingModelConfigName}` : '模型参数'"
      width="760px"
      align-center
      append-to-body
      destroy-on-close
    >
      <div class="model-config-dialog-body">
        <div class="model-config-toolbar">
          <div class="model-config-reference">
            <div class="model-config-reference-title">参考参数</div>
            <div class="model-config-reference-text">不填写的字段会继续使用 SDK / Provider / 模型默认值，不会强行覆盖。</div>
            <div class="model-config-reference-text">“填充推荐参数”会写入一套适合作为起点的建议值，你之后仍然可以逐项调整。</div>
            <div class="model-config-reference-text">这里保存的是当前编辑状态；关闭子弹窗后，还需要回到“模型管理”点击一次“保存”才会正式持久化。</div>
          </div>
          <div class="model-config-toolbar-actions">
            <el-button @click="resetModelConfigDraftToCurrent">恢复当前配置</el-button>
            <el-button @click="applyRecommendedModelConfig">填充推荐参数</el-button>
            <el-button type="danger" plain @click="removeCurrentModelConfig">清除配置</el-button>
          </div>
        </div>

        <div class="model-config-scroll">
          <div class="model-config-grid">
            <label class="model-config-field model-config-field-full">
              <span class="model-config-label">
                支持模态
                <el-tooltip :content="modelParameterHelp.modalities" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <div class="model-modality-row">
                <label v-for="modality in modalityOptions" :key="modality.value" class="model-modality-item">
                  <input
                    type="checkbox"
                    :checked="modelConfigDraft.modalities.includes(modality.value)"
                    @change="($event.target as HTMLInputElement).checked
                      ? modelConfigDraft.modalities = uniqueStrings([...modelConfigDraft.modalities, modality.value])
                      : modelConfigDraft.modalities = modelConfigDraft.modalities.filter((item) => item !== modality.value)"
                  />
                  <span>{{ modality.label }}</span>
                </label>
              </div>
            </label>

            <label class="model-config-field">
              <span class="model-config-label">
                工具调用
                <el-tooltip :content="modelParameterHelp.tools_enabled" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-select v-model="modelConfigDraft.tools_enabled">
                <el-option v-for="option in booleanOptions" :key="`tools-${option.value}`" :label="option.label" :value="option.value" />
              </el-select>
            </label>

            <label class="model-config-field">
              <span class="model-config-label">
                Thinking
                <el-tooltip :content="modelParameterHelp.thinking" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-select v-model="modelConfigDraft.thinking">
                <el-option v-for="option in booleanOptions" :key="`thinking-${option.value}`" :label="option.label" :value="option.value" />
              </el-select>
            </label>

            <label class="model-config-field">
              <span class="model-config-label">
                Temperature
                <el-tooltip :content="modelParameterHelp.temperature" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-input v-model="modelConfigDraft.temperatureText" placeholder="留空表示默认，例如 0.7" />
            </label>

            <label class="model-config-field">
              <span class="model-config-label">
                Max Output Tokens
                <el-tooltip :content="modelParameterHelp.max_output_tokens" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-input v-model="modelConfigDraft.maxTokensText" placeholder="留空表示默认，例如 4096" />
            </label>

            <label class="model-config-field">
              <span class="model-config-label">
                Top P
                <el-tooltip :content="modelParameterHelp.top_p" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-input v-model="modelConfigDraft.topPText" placeholder="留空表示默认" />
            </label>

            <label class="model-config-field">
              <span class="model-config-label">
                Top K
                <el-tooltip :content="modelParameterHelp.top_k" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-input v-model="modelConfigDraft.topKText" placeholder="留空表示默认" />
            </label>

            <label class="model-config-field">
              <span class="model-config-label">
                Presence Penalty
                <el-tooltip :content="modelParameterHelp.presence_penalty" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-input v-model="modelConfigDraft.presencePenaltyText" placeholder="留空表示默认" />
            </label>

            <label class="model-config-field">
              <span class="model-config-label">
                Frequency Penalty
                <el-tooltip :content="modelParameterHelp.frequency_penalty" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-input v-model="modelConfigDraft.frequencyPenaltyText" placeholder="留空表示默认" />
            </label>

            <label class="model-config-field">
              <span class="model-config-label">
                并行工具调用
                <el-tooltip :content="modelParameterHelp.parallel_tool_calls" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-select v-model="modelConfigDraft.parallelToolCalls">
                <el-option v-for="option in booleanOptions" :key="`parallel-${option.value}`" :label="option.label" :value="option.value" />
              </el-select>
            </label>

            <label class="model-config-field">
              <span class="model-config-label">
                Reasoning Effort
                <el-tooltip :content="modelParameterHelp.reasoning_effort" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-select v-model="modelConfigDraft.reasoningEffort">
                <el-option v-for="option in reasoningEffortOptions" :key="option.value || 'unset'" :label="option.label" :value="option.value" />
              </el-select>
            </label>

            <label class="model-config-field model-config-field-full">
              <span class="model-config-label">
                Stop Sequences
                <el-tooltip :content="modelParameterHelp.stop_sequences" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-input
                v-model="modelConfigDraft.stopSequencesText"
                type="textarea"
                :rows="4"
                placeholder="每行一个停止词，留空表示不设置"
              />
            </label>

            <label class="model-config-field model-config-field-full">
              <span class="model-config-label">
                Response MIME Type
                <el-tooltip :content="modelParameterHelp.response_mime_type" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-input v-model="modelConfigDraft.responseMimeType" placeholder="例如 application/json" />
            </label>

            <label class="model-config-field model-config-field-full">
              <span class="model-config-label">
                高级附加参数
                <el-tooltip :content="modelParameterHelp.additional_params" placement="top">
                  <el-icon class="param-help-icon"><QuestionFilled /></el-icon>
                </el-tooltip>
              </span>
              <el-input
                v-model="modelConfigDraft.additionalParamsText"
                type="textarea"
                :rows="8"
                placeholder='请输入 JSON 对象，例如 { "top_logprobs": 3 }'
              />
            </label>
          </div>
        </div>
      </div>

      <template #footer>
        <div class="model-config-footer">
          <div class="model-config-footer-tip">应用后会回写到当前模型草稿，最后仍需在“模型管理”点一次“保存”。</div>
          <div class="model-config-footer-actions">
            <el-button @click="showModelConfigDialog = false">取消</el-button>
            <el-button type="primary" @click="saveCurrentModelConfig">应用到当前模型</el-button>
          </div>
        </div>
      </template>
    </el-dialog>

    <el-dialog
      v-model="showAttachmentPreview"
      class="agent-dialog attachment-preview-dialog"
      title="图片预览"
      width="min(920px, calc(100vw - 36px))"
      append-to-body
      destroy-on-close
      align-center
    >
      <div class="attachment-preview-body">
        <img v-if="previewAttachmentUrl" :src="previewAttachmentUrl" alt="附件预览" class="attachment-preview-image" />
      </div>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, triggerRef, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Connection, Plus, QuestionFilled, Setting } from '@element-plus/icons-vue'

import request from '@/utils/request'
import {
  executeAgentToolCalls,
  type AgentToolCall,
  type AgentToolOutputPayload,
} from '@/utils/agentTools'
import {
  AGENT_WRITER_RESULT_EVENT,
  dispatchAgentWriterChunk,
  dispatchAgentWriterComplete,
  dispatchAgentWriterStart,
  getAgentEditorSnapshot,
  type AgentWriterMode,
  type AgentWriterResultDetail,
} from '@/utils/agentWriter'
import { getAgentEditorBridge } from '@/utils/agentEditorBridge'
import { hasDocDraft } from '@/utils/docDraftCache'
import {
  AGENT_ACTION_CLOSE_MARKER,
  AGENT_TASK_ANALYSIS_COMPLEXITIES,
  AGENT_TASK_ANALYSIS_INTENTS,
  AGENT_TASK_ANALYSIS_MODES,
  AGENT_TASK_ANALYSIS_WRITE_SCOPES,
  AGENT_WRITE_ACTION_MODES,
  AGENT_WRITE_ACTION_OPEN_MARKERS,
  DEFAULT_AGENT_BASE_URL,
  resolveAgentEditorSnapshotSource,
  toolHasOnlyCapabilities,
  resolveAgentWriteMode,
  type AgentControlBlock,
  type AgentPageScope,
} from '@/agent/protocol'
import { useSystemStore } from '@/stores/system'

type PageScope = AgentPageScope
type DocType = 'doc' | 'dir' | null
type SessionRole = 'user' | 'assistant' | 'system'
type RequestRole = SessionRole
type StreamAction = 'chat' | AgentWriterMode
type RouteKind = 'overview' | 'project' | 'doc'
type ProviderKind = 'openai' | 'anthropic' | 'gemini'

interface AgentRouteTarget {
  kind: RouteKind
  name?: string
}

interface AgentMessage {
  id: string
  role: SessionRole
  content: string
  reasoning?: string
  attachments?: AgentAttachment[]
  internalStatus?: boolean
}

interface AgentRequestMessage {
  role: RequestRole
  content: string
  attachments?: AgentAttachment[]
}

interface AgentAttachment {
  localId: string
  uploadId: number
  kind: 'image' | 'document'
  name: string
  url: string
  contentType: string | null
  size: number
}

interface AgentRuntimePlanStep {
  id: string
  title: string
  kind: string
  description: string
  status: 'pending' | 'running' | 'completed' | 'failed' | 'blocked' | string
  toolHints: string[]
  requiresConfirmation: boolean
  requiresDocumentWrite: boolean
  requiresDocumentSave: boolean
}

interface AgentRuntimePlan {
  id: string
  goal: string
  summary: string | null
  status: 'pending' | 'running' | 'completed' | 'failed' | 'blocked' | string
  steps: AgentRuntimePlanStep[]
  createdAt: string | null
  updatedAt: string | null
}

interface AgentTaskAnalysis {
  intent: string
  complexity: string
  mode: 'chat' | 'plan' | string
  requiresTools: boolean
  requiresUserConfirmation: boolean
  writeScope: 'partial' | 'full' | string | null
  preferredWriteAction: AgentWriterMode | string | null
  deliverable: string | null
}

interface AgentSessionMemory {
  summary: string | null
  activeUserGoals: string[]
  completedFacts: string[]
  openLoops: string[]
  updatedAt: string | null
}

interface AgentArtifact {
  id: string
  type: 'markdown_doc' | 'folder_plan' | 'project_outline' | string
  title: string
  status: 'drafting' | 'ready' | 'applied' | 'failed' | string
  content: string
  relatedDocId: number | null
}

interface AgentToolEvent {
  id: string
  tool: string
  status: 'requested' | 'running' | 'completed' | 'failed' | 'noop' | string
  summary: string
}

interface AgentExecutionToolCallSummary {
  name: string
  arguments: string | null
  output: string | null
  ok: boolean | null
  outcome: 'success' | 'noop' | 'error' | 'unknown'
  stagePolicy?: string | null
  capabilities?: string[]
}

interface AgentExecutionState {
  currentMode: 'normal' | 'plan' | string
  awaiting: string | null
  currentActionKind: string | null
  currentActionStatus: string | null
  currentActionMode: string | null
  currentActionTarget: string | null
  confirmationRequired: boolean
  pendingPlan: string | null
  pendingPlanUserReply: string | null
  planConfirmationDecision: 'approved' | 'rejected' | null
  compositeWriteThenSave: boolean
  semanticContinuation: boolean
  semanticContinuationRound: number
  previousAssistantSummary: string | null
  taskKind: string | null
  editIntent: string | null
  editStage: string | null
  saveRequested: boolean
  writeCompleted: boolean
  planStepIndex: number | null
  planTotalSteps: number | null
  planCurrentStep: string | null
  planCompletedSteps: string[]
  documentWriteObserved: boolean
  saveAttemptWithoutDocumentChange: boolean
  lastInterceptCode: string | null
  lastInterceptMessage: string | null
  lastInterceptGuidance: string | null
  recentToolCalls: AgentExecutionToolCallSummary[]
}

interface AgentExecutionMemory {
  currentMode: 'normal' | 'plan' | string
  awaiting: string | null
  currentActionKind: string | null
  currentActionStatus: string | null
  currentActionMode: string | null
  currentActionTarget: string | null
  confirmationRequired: boolean
  plan: string | null
  assistantSummary: string | null
  controlPhase: string | null
  taskKind: string | null
  editIntent: string | null
  editStage: string | null
  saveRequested: boolean
  writeCompleted: boolean
  planStepIndex: number | null
  planTotalSteps: number | null
  planCurrentStep: string | null
  planCompletedSteps: string[]
  documentWriteObserved: boolean
  saveAttemptWithoutDocumentChange: boolean
  lastInterceptCode: string | null
  lastInterceptMessage: string | null
  lastInterceptGuidance: string | null
  recentToolCalls: AgentExecutionToolCallSummary[]
}

interface AgentStructuredResponse {
  message: string
  state: AgentControlBlock | null
  plan: AgentRuntimePlan | null
}

interface AgentSession {
  id: string
  title: string
  messages: AgentMessage[]
  taskAnalysis: AgentTaskAnalysis | null
  runtimePlan: AgentRuntimePlan | null
  artifacts: AgentArtifact[]
  toolEvents: AgentToolEvent[]
  providerId: string | null
  model: string
  transportMode: 'auto' | 'responses' | 'chat'
  previousResponseId: string | null
  pendingPlan: string | null
  pendingPlanToolOutputs: AgentToolOutputPayload[]
  lastPlan: string | null
  lastExecutionMemory: AgentExecutionMemory | null
  sessionMemory: AgentSessionMemory | null
  lastSyncedMessageCount: number
  createdAt: number
  updatedAt: number
}

interface AgentProvider {
  id: string
  name: string
  providerKind: ProviderKind
  baseUrl: string
  hasApiKey: boolean
  remoteModels: string[]
  enabledModels: string[]
  customModels: string[]
  modelConfigs: Record<string, ProviderModelConfig>
  createdAt: number
  updatedAt: number
}

interface ProviderDraft {
  id: string | null
  name: string
  providerKind: ProviderKind
  baseUrl: string
  apiKey: string
}

interface ProviderModelConfig {
  modalities: string[]
  thinking: boolean | null
  tools_enabled: boolean | null
  temperature: number | null
  max_output_tokens: number | null
  top_p: number | null
  top_k: number | null
  presence_penalty: number | null
  frequency_penalty: number | null
  parallel_tool_calls: boolean | null
  reasoning_effort: string | null
  stop_sequences: string[]
  response_mime_type: string | null
  additional_params: Record<string, unknown> | null
}

interface ModelDraft {
  remoteModels: string[]
  enabledModels: string[]
  customModels: string[]
}

interface ModelConfigDraft {
  modalities: string[]
  thinking: '' | 'true' | 'false'
  tools_enabled: '' | 'true' | 'false'
  temperatureText: string
  maxTokensText: string
  topPText: string
  topKText: string
  presencePenaltyText: string
  frequencyPenaltyText: string
  parallelToolCalls: '' | 'true' | 'false'
  reasoningEffort: string
  stopSequencesText: string
  responseMimeType: string
  additionalParamsText: string
}

interface ModelApiItem {
  id: string
  owned_by?: string
  created?: number
}

interface ProviderApiItem {
  id: number | string
  name: string
  provider_kind?: string
  base_url?: string
  remote_models?: string[]
  enabled_models?: string[]
  custom_models?: string[]
  model_configs?: Record<string, ProviderModelConfig>
  is_active?: boolean
  has_api_key?: boolean
  created_at?: string
  updated_at?: string
}

interface ProviderListResponse {
  providers?: ProviderApiItem[]
  active_provider_id?: number | string | null
}

interface ProviderDetailResponse {
  id: number | string
  name: string
  provider_kind?: string
  base_url?: string
  api_key?: string
  remote_models?: string[]
  enabled_models?: string[]
  custom_models?: string[]
  model_configs?: Record<string, ProviderModelConfig>
  is_active?: boolean
}

type McpTransport = 'sse' | 'streamable-http' | 'stdio'
type McpAuthType = 'none' | 'bearer' | 'basic' | 'header' | 'query'
type McpSecretCollectionMode = 'keep' | 'replace' | 'clear'
type McpSnapshotTab = 'tools' | 'resources' | 'prompts'

interface McpRuntimeCapabilities {
  transports: McpTransport[]
  stdioEnabled: boolean
  stdioAllowedCommands: string[]
}

interface McpRuntimeCapabilitiesApiResponse {
  transports?: string[]
  stdio_enabled?: boolean
  stdio_allowed_commands?: string[]
}

interface McpSettingsState {
  enabled: boolean
  updatedAt: number | null
}

interface McpSettingsApiResponse {
  settings?: {
    enabled?: boolean
    updated_at?: string | null
  }
}

interface McpSecretValueResponse {
  has_value?: boolean
  value?: string | null
}

interface McpSecretEntry {
  name: string
  hasValue: boolean
  value: string
}

interface McpSecretEntryApiResponse {
  name?: string
  has_value?: boolean
  value?: string | null
}

interface McpAuthDetail {
  authType: McpAuthType
  scheme: string
  username: string
  headerName: string
  queryName: string
  tokenHasValue: boolean
  passwordHasValue: boolean
  valueHasValue: boolean
  tokenValue: string
  passwordValue: string
  valueText: string
}

interface McpAuthDetailApiResponse {
  auth_type?: string
  scheme?: string | null
  username?: string | null
  header_name?: string | null
  query_name?: string | null
  token?: McpSecretValueResponse
  password?: McpSecretValueResponse
  value?: McpSecretValueResponse
}

interface McpServerSummary {
  id: string
  name: string
  enabled: boolean
  transport: McpTransport
  url: string
  command: string
  authType: McpAuthType
  lastStatus: string | null
  lastError: string | null
  configVersion: number
  createdAt: number
  updatedAt: number
}

interface McpServerSummaryApiResponse {
  id: number | string
  name?: string
  enabled?: boolean
  transport?: string
  url?: string | null
  command?: string | null
  auth_type?: string
  last_status?: string | null
  last_error?: string | null
  config_version?: number
  created_at?: string
  updated_at?: string
}

interface McpServerDetail extends McpServerSummary {
  args: string[]
  auth: McpAuthDetail | null
  customHeaders: McpSecretEntry[]
  stdioEnv: McpSecretEntry[]
  toolsSnapshot: unknown[]
  resourcesSnapshot: unknown[]
  promptsSnapshot: unknown[]
  lastSyncAt: number | null
}

interface McpServerDetailApiResponse extends McpServerSummaryApiResponse {
  args?: string[]
  auth?: McpAuthDetailApiResponse | null
  custom_headers?: McpSecretEntryApiResponse[]
  stdio_env?: McpSecretEntryApiResponse[]
  tools_snapshot?: unknown
  resources_snapshot?: unknown
  prompts_snapshot?: unknown
  last_sync_at?: string | null
}

interface McpServersResponse {
  servers?: McpServerSummaryApiResponse[]
}

interface McpServerResponse {
  server?: McpServerDetailApiResponse
}

interface McpDraft {
  id: string | null
  name: string
  enabled: boolean
  transport: McpTransport
  url: string
  command: string
  argsText: string
  authType: McpAuthType
  bearerScheme: string
  bearerToken: string
  authUsername: string
  authPassword: string
  authHeaderName: string
  authHeaderValue: string
  authQueryName: string
  authQueryValue: string
  customHeadersMode: McpSecretCollectionMode
  customHeadersText: string
  stdioEnvMode: McpSecretCollectionMode
  stdioEnvText: string
  existingCustomHeaders: McpSecretEntry[]
  existingStdioEnv: McpSecretEntry[]
  authSecretFlags: {
    token: boolean
    password: boolean
    value: boolean
  }
  lastStatus: string | null
  lastError: string | null
  configVersion: number
  toolsSnapshot: unknown[]
  resourcesSnapshot: unknown[]
  promptsSnapshot: unknown[]
  lastSyncAt: number | null
  createdAt: number
  updatedAt: number
}

const REQUEST_RECENT_MESSAGE_COUNT = 8
const REQUEST_SUMMARY_TRIGGER_CHARS = 6000
const REQUEST_SUMMARY_MAX_ITEMS = 6
const REQUEST_SUMMARY_ITEM_CHARS = 240
const MAX_SEMANTIC_CONTINUATION_ROUNDS = 128
const AGENT_DEBUG_STORAGE_KEY = 'markflow:agent-debug'
const ACTION_MODE_PATTERN = AGENT_WRITE_ACTION_MODES.join('|')
const ACTION_BLOCK_REGEX = new RegExp(String.raw`\s*\[\[ACTION:(${ACTION_MODE_PATTERN})\]\][\s\S]*?\[\[/ACTION\]\]\s*`, 'gi')
const ACTION_OPEN_REGEX = new RegExp(String.raw`\[\[ACTION:(${ACTION_MODE_PATTERN})\]\]`, 'i')
const ACTION_WRAPPED_REGEX = new RegExp(String.raw`^\s*\[\[ACTION:(${ACTION_MODE_PATTERN})\]\]\s*([\s\S]*?)\s*\[\[/ACTION\]\]\s*$`, 'i')
const ROUTE_MARKER_REGEX = /\s*\[\[ROUTE:(overview|project|doc)(?::[\s\S]*?)?\]\]\s*/gi
const PLAN_BLOCK_REGEX = /\s*\[\[PLAN\]\][\s\S]*?\[\[\/PLAN\]\]\s*/gi
const props = defineProps<{
  pageScope: PageScope
  pageState: string
  projectId: number | null
  projectName: string
  docId: number | null
  docName: string
  docType: DocType
  docContent: string
  projectCatalog: string
  currentNodeCatalog: string
  editorAvailable: boolean
  editorSnapshotSource: string
  editorUnsavedChanges: boolean
}>()

const emit = defineEmits<{
  navigate: [target: AgentRouteTarget]
}>()

const DEFAULT_BASE_URL = DEFAULT_AGENT_BASE_URL
const providerKindDefaults: Record<ProviderKind, {
  label: string
  baseUrl: string
  auth: string
  modelsApi: string
  models: string[]
  summary: string
}> = {
  openai: {
    label: 'OpenAI / Compatible',
    baseUrl: 'https://api.openai.com/v1',
    auth: 'Bearer API Key',
    modelsApi: 'GET /models',
    models: ['gpt-4.1', 'gpt-4.1-mini', 'gpt-4o', 'gpt-4o-mini'],
    summary: '适用于 OpenAI 兼容协议，以及大多数第三方兼容网关。',
  },
  anthropic: {
    label: 'Anthropic / Claude',
    baseUrl: 'https://api.anthropic.com/v1',
    auth: 'x-api-key + anthropic-version',
    modelsApi: 'GET /v1/models',
    models: ['claude-sonnet-4-0', 'claude-3-7-sonnet-latest', 'claude-3-5-haiku-latest'],
    summary: '适用于 Claude 原生协议，Base URL 通常以 /v1 结尾。',
  },
  gemini: {
    label: 'Gemini',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta',
    auth: 'URL query key=API_KEY',
    modelsApi: 'GET /v1beta/models?key=...',
    models: ['gemini-2.5-pro', 'gemini-2.5-flash', 'gemini-2.0-flash'],
    summary: '适用于 Gemini 原生协议，模型列表接口通常走 Google Generative Language。',
  },
}
const modalityOptions = [
  { value: 'text', label: '文本' },
  { value: 'image', label: '图片' },
  { value: 'audio', label: '音频' },
  { value: 'video', label: '视频' },
  { value: 'document', label: '文件' },
]
const reasoningEffortOptions = [
  { value: '', label: '未设置（默认）' },
  { value: 'minimal', label: 'minimal' },
  { value: 'low', label: 'low' },
  { value: 'medium', label: 'medium' },
  { value: 'high', label: 'high' },
]
const booleanOptions = [
  { value: '', label: '未设置（默认）' },
  { value: 'true', label: '开启' },
  { value: 'false', label: '关闭' },
]
const modelParameterHelp: Record<string, string> = {
  modalities: '声明这个模型在你项目中的可用输入模态。它主要用于前端能力提示和后续多模态开关判断；留空表示按模型实际能力与 SDK 默认处理。',
  tools_enabled: '是否允许这个模型在 Agent 过程中发起工具调用。留空表示按系统默认策略；关闭后会禁止工具调用，只保留纯文本对话生成。',
  thinking: '是否显式开启推理/思考模式。留空表示不额外指定，由 provider 或模型自行决定；开启后会尽量向上游传递 reasoning / thinking 配置。',
  temperature: '采样温度，数值越低越稳定，越高越发散。常见范围是 0 到 2；留空表示沿用 provider / SDK 默认值。',
  max_output_tokens: '单次响应允许生成的最大 token 数。留空表示使用模型默认上限或 SDK 默认值。',
  top_p: '核采样阈值。设置后会限制累计概率最高的一部分 token 参与采样；通常与 temperature 二选一微调即可。',
  top_k: '仅对支持的模型生效，限制每一步只在前 K 个候选 token 中采样。Gemini 等模型更常见。',
  presence_penalty: '出现惩罚。鼓励模型引入新词、新主题，减少重复已出现过的内容。',
  frequency_penalty: '频率惩罚。对已经高频出现的 token 提高惩罚，抑制重复表达。',
  parallel_tool_calls: '是否允许模型并行发起多个工具调用。只对支持该能力的 provider 生效；留空表示使用默认行为。',
  reasoning_effort: '推理强度。当前主要用于支持 reasoning effort 的模型；数值越高，通常思考更深但成本和时延也更高。',
  stop_sequences: '命中这些停止词后立刻结束生成。每行一个，留空表示不设置。',
  response_mime_type: '期望的响应 MIME 类型，主要用于 Gemini 等支持结构化 MIME 的模型，例如 application/json。',
  additional_params: '高级附加参数，会原样透传给对应 provider。用于补充当前表单未覆盖的协议字段，必须填写合法 JSON 对象。',
}
const PANEL_STATE_KEY = 'markflow.agent.panel.state'
const SESSIONS_KEY = 'markflow.agent.sessions'
const VIEWPORT_MARGIN = 24
const FAB_WIDTH = 96
const FAB_HEIGHT = 48
const PANEL_WIDTH = 520
const PANEL_HEIGHT = 720
const MAX_TOOL_CALL_ROUNDS = 128
const MAX_REPEAT_TOOL_SIGNATURE_HITS = 4

const mounted = ref(false)
const collapsed = ref(false)
const showProviderDialog = ref(false)
const showMcpDialog = ref(false)
const showModelDialog = ref(false)
const showModelConfigDialog = ref(false)
const showAttachmentPreview = ref(false)
const showRuntimeDock = ref(true)
const streaming = ref(false)
const modelLoading = ref(false)
const prompt = ref('')
const attachmentInputRef = ref<HTMLInputElement | null>(null)
const composerAttachments = ref<AgentAttachment[]>([])
const attachmentUploadingCount = ref(0)
const previewAttachmentUrl = ref('')
const panelX = ref(0)
const panelY = ref(88)
const expandedPanelX = ref(0)
const expandedPanelY = ref(88)
const collapsedPanelX = ref(0)
const collapsedPanelY = ref(88)
const currentSessionId = ref<string>('')
const activeProviderId = ref<string>('')
const sessions = ref<AgentSession[]>([])
const providers = ref<AgentProvider[]>([])
const streamingAssistantId = ref('')
const liveAssistantContent = ref('')
const liveAssistantReasoning = ref('')
const agentTransportMode = ref<'responses' | 'chat_fallback' | 'chat' | ''>('')
const messagesRef = ref<HTMLElement | null>(null)
const customModelInput = ref('')
const modelSearchQuery = ref('')
const editingModelConfigName = ref('')
const providerDraft = ref<ProviderDraft>({
  id: null,
  name: '',
  providerKind: 'openai',
  baseUrl: providerKindDefaults.openai.baseUrl,
  apiKey: '',
})
const mcpLoading = ref(false)
const mcpSaving = ref(false)
const mcpTesting = ref(false)
const mcpRefreshing = ref(false)
const mcpSettingsSaving = ref(false)
const mcpSettings = ref<McpSettingsState>({
  enabled: false,
  updatedAt: null,
})
const mcpSettingsEnabled = ref(false)
const mcpRuntimeCapabilities = ref<McpRuntimeCapabilities>({
  transports: ['sse', 'streamable-http'],
  stdioEnabled: false,
  stdioAllowedCommands: [],
})
const mcpServers = ref<McpServerSummary[]>([])
const mcpDraft = ref<McpDraft>(createMcpDraft())
const activeMcpSnapshotTab = ref<McpSnapshotTab>('tools')
const modelDraft = ref<ModelDraft>({
  remoteModels: [],
  enabledModels: [],
  customModels: [],
})
const modelConfigDraft = ref<ModelConfigDraft>({
  modalities: [],
  thinking: '',
  tools_enabled: '',
  temperatureText: '',
  maxTokensText: '',
  topPText: '',
  topKText: '',
  presencePenaltyText: '',
  frequencyPenaltyText: '',
  parallelToolCalls: '',
  reasoningEffort: '',
  stopSequencesText: '',
  responseMimeType: '',
  additionalParamsText: '',
})
const providerDraftPreset = computed(() => providerKindDefaults[normalizeProviderKind(providerDraft.value.providerKind)])
const editingModelConfig = computed(() => {
  const provider = activeProvider.value
  const model = editingModelConfigName.value.trim()
  if (!provider || !model) return null
  return provider.modelConfigs[model] || null
})

let dragOffsetX = 0
let dragOffsetY = 0
let dragging = false
let didDrag = false
let activeStreamController: AbortController | null = null
const systemStore = useSystemStore()

const panelStyle = computed(() => ({
  transform: `translate(${panelX.value}px, ${panelY.value}px)`,
}))

const currentSession = computed(() => sessions.value.find((session) => session.id === currentSessionId.value) || null)
const visibleMessages = computed(() =>
  (currentSession.value?.messages || []).filter((message) =>
    message.role === 'user' || message.role === 'assistant',
  ),
)
const currentSessionLastPlan = computed(() => currentSession.value?.lastPlan?.trim() || '')
const currentSessionTaskAnalysis = computed(() => currentSession.value?.taskAnalysis || null)
const currentSessionRuntimePlan = computed(() => currentSession.value?.runtimePlan || null)
const currentSessionArtifacts = computed(() => currentSession.value?.artifacts || [])
const currentSessionToolEvents = computed(() => currentSession.value?.toolEvents || [])
const currentRuntimeStep = computed(() => {
  const runtimePlan = currentSessionRuntimePlan.value
  if (!runtimePlan?.steps?.length) return null
  return runtimePlan.steps.find((step) => step.status === 'running')
    || runtimePlan.steps.find((step) => step.status === 'pending')
    || runtimePlan.steps[runtimePlan.steps.length - 1]
    || null
})
const runtimeDockSummary = computed(() => {
  if (currentPlanNeedsConfirmation.value) {
    return '待确认计划'
  }
  if (streaming.value) {
    return currentRuntimeStep.value?.title || '正在执行'
  }
  if (currentSessionRuntimePlan.value?.status === 'completed') {
    return '执行完成'
  }
  return currentRuntimeStep.value?.title || '暂无运行状态'
})
const activeProvider = computed(() => providers.value.find((provider) => provider.id === activeProviderId.value) || null)
const mcpIsHttpTransport = computed(() => mcpDraft.value.transport !== 'stdio')
const availableMcpTransports = computed(() => {
  const values = uniqueStrings([
    ...mcpRuntimeCapabilities.value.transports,
    mcpDraft.value.transport,
  ])
  return values
    .map((value) => normalizeMcpTransport(value))
    .filter((value, index, list): value is McpTransport => Boolean(value) && list.indexOf(value) === index)
})
const activeMcpSnapshotEntries = computed(() => {
  if (activeMcpSnapshotTab.value === 'resources') {
    return mcpDraft.value.resourcesSnapshot
  }
  if (activeMcpSnapshotTab.value === 'prompts') {
    return mcpDraft.value.promptsSnapshot
  }
  return mcpDraft.value.toolsSnapshot
})
const selectedProviderId = computed({
  get: () => activeProviderId.value,
  set: (value: string) => {
    activateProvider(value)
  },
})
const activeModelOptions = computed(() => enabledModelsForProvider(activeProvider.value))
const currentSessionModel = computed({
  get: () => currentSession.value?.model || '',
  set: (value: string) => {
    const session = currentSession.value
    if (!session) return
    const normalized = value.trim()
    const modelChanged = session.model !== normalized
    ensureModelAvailableForActiveProvider(normalized)
    session.providerId = activeProvider.value?.id || null
    session.model = normalized
    if (modelChanged) {
      session.previousResponseId = null
      session.lastSyncedMessageCount = 0
    }
    sessions.value = [...sessions.value]
    persistSessions()
  },
})
const currentSessionTransportMode = computed({
  get: () => currentSession.value?.transportMode || 'auto',
  set: (value: 'auto' | 'responses' | 'chat') => {
    const session = currentSession.value
    if (!session) return
    const normalized = value === 'responses' || value === 'chat' ? value : 'auto'
    if (session.transportMode === normalized) return
    session.transportMode = normalized
    session.previousResponseId = null
    session.lastSyncedMessageCount = 0
    sessions.value = [...sessions.value]
    persistSessions()
  },
})
const activeModelKey = computed(() => activeModelOptions.value.join('|'))
const filteredRemoteModels = computed(() => {
  const keyword = modelSearchQuery.value.trim().toLowerCase()
  if (!keyword) return modelDraft.value.remoteModels
  return modelDraft.value.remoteModels.filter((model) => model.toLowerCase().includes(keyword))
})
const activeModelConfig = computed(() => {
  const provider = activeProvider.value
  const model = currentSessionModel.value.trim()
  if (!provider || !model) return null
  return provider.modelConfigs[model] || null
})
const activeModelModalities = computed(() => uniqueStrings(activeModelConfig.value?.modalities || []))
const composerSupportsImage = computed(() => activeModelModalities.value.includes('image'))
const composerSupportsDocument = computed(() => activeModelModalities.value.includes('document'))
const composerAttachmentEnabled = computed(() => composerSupportsImage.value || composerSupportsDocument.value)
const composerAttachmentAccept = computed(() => {
  const parts: string[] = []
  if (composerSupportsImage.value) {
    parts.push('image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/heic', 'image/heif', 'image/svg+xml')
  }
  if (composerSupportsDocument.value) {
    parts.push('text/*', ...SUPPORTED_DOCUMENT_EXTENSIONS.map((ext) => `.${ext}`))
  }
  return parts.join(',')
})
const attachmentUploading = computed(() => attachmentUploadingCount.value > 0)
const currentManagedModels = computed(() =>
  uniqueStrings([...modelDraft.value.enabledModels]).map((model) => ({
    id: model,
    isCustom: modelDraft.value.customModels.includes(model),
    isRemote: modelDraft.value.remoteModels.includes(model),
    isConfigured: modelConfigIsConfigured(activeProvider.value?.modelConfigs?.[model]),
  })),
)
const currentConfirmationPlan = computed(() => {
  const session = currentSession.value
  if (!session) return ''
  const awaitingPlanConfirmation = session.lastExecutionMemory?.currentMode === 'plan'
    && session.lastExecutionMemory?.awaiting === 'user_input'
  if (!awaitingPlanConfirmation && session.runtimePlan?.status !== 'pending') {
    return ''
  }
  return runtimePlanToPlanText(session.runtimePlan).trim() || session.pendingPlan?.trim() || ''
})
const currentPlanNeedsConfirmation = computed(() => Boolean(currentConfirmationPlan.value))

const modeTip = computed(() => {
  if (props.docType === 'doc') return '直接说你的目标即可，模型会自己决定是答复、续写还是重写当前文档。'
  return '当前不在具体文档页时，模型只进行对话回答，不会直接写入文档。'
})

const textareaPlaceholder = computed(() => {
  if (props.docType === 'doc') return '例如：继续完善这篇文档的部署说明，并补充安装、验证和常见问题'
  return '输入你的问题，按 Enter 发送'
})

function genId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`
}

function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw ? JSON.parse(raw) as T : fallback
  } catch {
    return fallback
  }
}

function uniqueStrings(values: unknown[]): string[] {
  const normalized = values
    .map((value) => typeof value === 'string' ? value.trim() : '')
    .filter(Boolean)
  return Array.from(new Set(normalized))
}

function normalizeAttachment(raw: any): AgentAttachment | null {
  if (!raw || typeof raw !== 'object') return null
  const uploadId = Number(raw.uploadId ?? raw.upload_id)
  if (!Number.isFinite(uploadId) || uploadId <= 0) return null
  const kind = raw.kind === 'image' ? 'image' : raw.kind === 'document' ? 'document' : null
  if (!kind) return null
  const name = typeof raw.name === 'string' && raw.name.trim() ? raw.name.trim() : `附件-${uploadId}`
  const url = typeof raw.url === 'string' ? raw.url : ''
  const contentType = typeof raw.contentType === 'string'
    ? raw.contentType
    : typeof raw.content_type === 'string'
      ? raw.content_type
      : null
  const size = Number(raw.size)

  return {
    localId: typeof raw.localId === 'string' && raw.localId.trim() ? raw.localId.trim() : `upload-${uploadId}`,
    uploadId,
    kind,
    name,
    url,
    contentType,
    size: Number.isFinite(size) && size > 0 ? size : 0,
  }
}

function fileExtension(name: string) {
  const normalized = name.trim().toLowerCase()
  const index = normalized.lastIndexOf('.')
  return index >= 0 ? normalized.slice(index + 1) : ''
}

const SUPPORTED_DOCUMENT_MIME_TYPES = [
  'application/pdf',
  'text/plain',
  'text/markdown',
  'text/md',
  'text/rtf',
  'text/html',
  'text/css',
  'text/csv',
  'text/xml',
  'application/x-javascript',
  'text/x-javascript',
  'application/javascript',
  'text/javascript',
  'application/x-python',
  'text/x-python',
] as const

const SUPPORTED_DOCUMENT_EXTENSIONS = [
  'pdf',
  'txt',
  'md',
  'markdown',
  'rtf',
  'html',
  'htm',
  'css',
  'csv',
  'xml',
  'js',
  'mjs',
  'cjs',
  'py',
  'json',
  'toml',
  'yaml',
  'yml',
  'ini',
  'cfg',
  'conf',
  'env',
  'properties',
  'gradle',
  'gitignore',
  'gitattributes',
  'npmrc',
  'yarnrc',
  'editorconfig',
  'vmoptions',
  'log',
  'sh',
  'bash',
  'zsh',
  'fish',
  'sql',
  'ts',
  'tsx',
  'jsx',
  'java',
  'kt',
  'kts',
  'go',
  'rs',
  'c',
  'cc',
  'cpp',
  'cxx',
  'h',
  'hpp',
  'cs',
  'php',
  'rb',
  'swift',
] as const

function isSupportedImageFile(file: File) {
  const type = file.type.toLowerCase()
  const ext = fileExtension(file.name)
  return [
    'image/png',
    'image/jpeg',
    'image/gif',
    'image/webp',
    'image/heic',
    'image/heif',
    'image/svg+xml',
  ].includes(type) || ['png', 'jpg', 'jpeg', 'gif', 'webp', 'heic', 'heif', 'svg'].includes(ext)
}

function isSupportedDocumentFile(file: File) {
  const type = file.type.toLowerCase()
  const ext = fileExtension(file.name)
  return SUPPORTED_DOCUMENT_MIME_TYPES.includes(type as typeof SUPPORTED_DOCUMENT_MIME_TYPES[number])
    || type.startsWith('text/')
    || SUPPORTED_DOCUMENT_EXTENSIONS.includes(ext as typeof SUPPORTED_DOCUMENT_EXTENSIONS[number])
}

async function isProbablyUtf8TextFile(file: File) {
  const sampleSize = Math.min(file.size, 8192)
  if (sampleSize <= 0) return true

  const bytes = new Uint8Array(await file.slice(0, sampleSize).arrayBuffer())
  if (!bytes.length) return true
  if (bytes.includes(0)) return false

  try {
    new TextDecoder('utf-8', { fatal: true }).decode(bytes)
  } catch {
    return false
  }

  let controlCharCount = 0
  for (const byte of bytes) {
    if (byte < 0x09 || (byte > 0x0d && byte < 0x20)) {
      controlCharCount += 1
    }
  }

  return controlCharCount / bytes.length < 0.02
}

function formatAttachmentSize(size: number) {
  if (!Number.isFinite(size) || size <= 0) return '未知大小'
  if (size < 1024) return `${size}B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(size >= 10 * 1024 ? 0 : 1)}KB`
  return `${(size / 1024 / 1024).toFixed(size >= 10 * 1024 * 1024 ? 0 : 1)}MB`
}

function absoluteAttachmentUrl(attachment: AgentAttachment) {
  if (!attachment.url) return ''
  if (/^https?:\/\//i.test(attachment.url)) return attachment.url
  return `${window.location.origin}${attachment.url}`
}

function isImageAttachment(attachment: AgentAttachment) {
  return attachment.kind === 'image' || attachment.contentType?.startsWith('image/') === true
}

function isPdfAttachment(attachment: AgentAttachment) {
  return attachment.contentType === 'application/pdf' || fileExtension(attachment.name) === 'pdf'
}

function triggerAttachmentDownload(attachment: AgentAttachment) {
  const url = absoluteAttachmentUrl(attachment)
  if (!url) return
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = attachment.name || ''
  anchor.rel = 'noopener noreferrer'
  document.body.appendChild(anchor)
  anchor.click()
  document.body.removeChild(anchor)
}

function openAttachment(attachment: AgentAttachment) {
  const url = absoluteAttachmentUrl(attachment)
  if (!url) {
    ElMessage.warning('附件链接不可用')
    return
  }

  if (isImageAttachment(attachment)) {
    previewAttachmentUrl.value = url
    showAttachmentPreview.value = true
    return
  }

  if (isPdfAttachment(attachment)) {
    window.open(url, '_blank', 'noopener,noreferrer')
    return
  }

  triggerAttachmentDownload(attachment)
}

function clearComposerAttachments() {
  composerAttachments.value = []
  if (attachmentInputRef.value) {
    attachmentInputRef.value.value = ''
  }
}

function removeComposerAttachment(localId: string) {
  composerAttachments.value = composerAttachments.value.filter((attachment) => attachment.localId !== localId)
  if (attachmentInputRef.value) {
    attachmentInputRef.value.value = ''
  }
}

function parseNumberText(value: string): number | null {
  const trimmed = value.trim()
  if (!trimmed) return null
  const parsed = Number(trimmed)
  return Number.isFinite(parsed) ? parsed : null
}

function parseIntegerText(value: string): number | null {
  const parsed = parseNumberText(value)
  return parsed === null ? null : Math.trunc(parsed)
}

function parseBooleanChoice(value: '' | 'true' | 'false'): boolean | null {
  if (value === 'true') return true
  if (value === 'false') return false
  return null
}

function normalizeModelConfig(input?: Partial<ProviderModelConfig> | null): ProviderModelConfig {
  return {
    modalities: Array.isArray(input?.modalities) ? uniqueStrings(input.modalities) : [],
    thinking: typeof input?.thinking === 'boolean' ? input.thinking : null,
    tools_enabled: typeof input?.tools_enabled === 'boolean' ? input.tools_enabled : null,
    temperature: typeof input?.temperature === 'number' ? input.temperature : null,
    max_output_tokens: typeof input?.max_output_tokens === 'number' ? input.max_output_tokens : null,
    top_p: typeof input?.top_p === 'number' ? input.top_p : null,
    top_k: typeof input?.top_k === 'number' ? input.top_k : null,
    presence_penalty: typeof input?.presence_penalty === 'number' ? input.presence_penalty : null,
    frequency_penalty: typeof input?.frequency_penalty === 'number' ? input.frequency_penalty : null,
    parallel_tool_calls: typeof input?.parallel_tool_calls === 'boolean' ? input.parallel_tool_calls : null,
    reasoning_effort: typeof input?.reasoning_effort === 'string' && input.reasoning_effort.trim()
      ? input.reasoning_effort.trim()
      : null,
    stop_sequences: Array.isArray(input?.stop_sequences) ? uniqueStrings(input.stop_sequences) : [],
    response_mime_type: typeof input?.response_mime_type === 'string' && input.response_mime_type.trim()
      ? input.response_mime_type.trim()
      : null,
    additional_params: input?.additional_params && typeof input.additional_params === 'object'
      ? { ...input.additional_params }
      : null,
  }
}

function createModelConfigDraft(config?: Partial<ProviderModelConfig> | null): ModelConfigDraft {
  const normalized = normalizeModelConfig(config)
  return {
    modalities: [...normalized.modalities],
    thinking: normalized.thinking === null ? '' : normalized.thinking ? 'true' : 'false',
    tools_enabled: normalized.tools_enabled === null ? '' : normalized.tools_enabled ? 'true' : 'false',
    temperatureText: normalized.temperature === null ? '' : String(normalized.temperature),
    maxTokensText: normalized.max_output_tokens === null ? '' : String(normalized.max_output_tokens),
    topPText: normalized.top_p === null ? '' : String(normalized.top_p),
    topKText: normalized.top_k === null ? '' : String(normalized.top_k),
    presencePenaltyText: normalized.presence_penalty === null ? '' : String(normalized.presence_penalty),
    frequencyPenaltyText: normalized.frequency_penalty === null ? '' : String(normalized.frequency_penalty),
    parallelToolCalls: normalized.parallel_tool_calls === null ? '' : normalized.parallel_tool_calls ? 'true' : 'false',
    reasoningEffort: normalized.reasoning_effort || '',
    stopSequencesText: normalized.stop_sequences.join('\n'),
    responseMimeType: normalized.response_mime_type || '',
    additionalParamsText: normalized.additional_params ? JSON.stringify(normalized.additional_params, null, 2) : '',
  }
}

function recommendedModelConfig(kind: ProviderKind): ProviderModelConfig {
  const base: ProviderModelConfig = {
    modalities: ['text'],
    thinking: false,
    tools_enabled: true,
    temperature: 0.7,
    max_output_tokens: 4096,
    top_p: null,
    top_k: null,
    presence_penalty: null,
    frequency_penalty: null,
    parallel_tool_calls: true,
    reasoning_effort: null,
    stop_sequences: [],
    response_mime_type: null,
    additional_params: null,
  }

  if (kind === 'gemini') {
    return {
      ...base,
      top_p: 0.95,
      parallel_tool_calls: null,
    }
  }

  return base
}

function modelConfigIsConfigured(config?: Partial<ProviderModelConfig> | null) {
  const normalized = normalizeModelConfig(config)
  return Boolean(
    normalized.modalities.length
    || normalized.thinking !== null
    || normalized.tools_enabled !== null
    || normalized.temperature !== null
    || normalized.max_output_tokens !== null
    || normalized.top_p !== null
    || normalized.top_k !== null
    || normalized.presence_penalty !== null
    || normalized.frequency_penalty !== null
    || normalized.parallel_tool_calls !== null
    || normalized.reasoning_effort
    || normalized.stop_sequences.length
    || normalized.response_mime_type
    || normalized.additional_params,
  )
}

function normalizedProviderBaseUrl(value?: string | null) {
  const trimmed = typeof value === 'string' ? value.trim() : ''
  return (trimmed || DEFAULT_BASE_URL).replace(/\/+$/, '')
}

function normalizeProviderKind(value?: string | null): ProviderKind {
  const normalized = typeof value === 'string' ? value.trim().toLowerCase() : ''
  if (normalized === 'anthropic' || normalized === 'claude') return 'anthropic'
  if (normalized === 'gemini' || normalized === 'google') return 'gemini'
  return 'openai'
}

function providerKindLabel(kind: string) {
  return providerKindDefaults[normalizeProviderKind(kind)].label
}

function applyProviderKindDefaults(kind: string, options: { force?: boolean } = {}) {
  const preset = providerKindDefaults[normalizeProviderKind(kind)]
  if (options.force || !providerDraft.value.baseUrl.trim()) {
    providerDraft.value.baseUrl = preset.baseUrl
  }
}

watch(
  () => providerDraft.value.providerKind,
  (next, previous) => {
    const previousPreset = previous ? providerKindDefaults[normalizeProviderKind(previous)] : null
    const normalizedBaseUrl = normalizedProviderBaseUrl(providerDraft.value.baseUrl)

    if (!providerDraft.value.baseUrl.trim()) {
      applyProviderKindDefaults(next, { force: true })
      return
    }

    if (previousPreset && normalizedBaseUrl === normalizedProviderBaseUrl(previousPreset.baseUrl)) {
      applyProviderKindDefaults(next, { force: true })
    }
  },
)

function enabledModelsForProvider(provider: AgentProvider | null) {
  return provider ? uniqueStrings(provider.enabledModels) : []
}

function fallbackModelForProvider(provider: AgentProvider | null) {
  if (!provider) return ''
  const enabled = enabledModelsForProvider(provider)
  if (enabled.length) return enabled[0]
  return uniqueStrings([...provider.customModels, ...provider.remoteModels])[0] || ''
}

function ensureModelAvailableForActiveProvider(model: string) {
  if (!model) return
  const provider = activeProvider.value
  if (!provider) return

  let changed = false

  if (!provider.customModels.includes(model) && !provider.remoteModels.includes(model)) {
    provider.customModels = uniqueStrings([...provider.customModels, model])
    changed = true
  }
  if (!provider.enabledModels.includes(model)) {
    provider.enabledModels = uniqueStrings([...provider.enabledModels, model])
    changed = true
  }

  if (changed) {
    provider.updatedAt = Date.now()
    providers.value = [...providers.value]
    void saveProviderConfig(provider)
  }
}

async function saveProviderConfig(provider: AgentProvider) {
  const data = await request.post('/agent/providers', {
    id: Number(provider.id),
    name: provider.name,
    provider_kind: provider.providerKind,
    base_url: provider.baseUrl,
    api_key: '',
    remote_models: provider.remoteModels,
    enabled_models: provider.enabledModels,
    custom_models: provider.customModels,
    model_configs: provider.modelConfigs,
  }) as ProviderListResponse

  const normalizedProviders = (data.providers || [])
    .map((item) => normalizeProvider(item))
    .filter((item): item is AgentProvider => Boolean(item))

  providers.value = normalizedProviders
  activeProviderId.value = `${data.active_provider_id ?? normalizedProviders.find((item) => item.id === activeProviderId.value)?.id ?? normalizedProviders[0]?.id ?? ''}`
}

function maxPanelXFor(nextCollapsed: boolean) {
  const width = nextCollapsed ? FAB_WIDTH : PANEL_WIDTH
  return Math.max(VIEWPORT_MARGIN, window.innerWidth - width - VIEWPORT_MARGIN)
}

function maxPanelYFor(nextCollapsed: boolean) {
  const estimatedHeight = nextCollapsed
    ? FAB_HEIGHT
    : Math.min(PANEL_HEIGHT, Math.max(420, window.innerHeight - 112))
  return Math.max(VIEWPORT_MARGIN, window.innerHeight - estimatedHeight - VIEWPORT_MARGIN)
}

function clampPanelPosition(nextX: number, nextY: number, nextCollapsed: boolean) {
  return {
    x: Math.max(VIEWPORT_MARGIN, Math.min(maxPanelXFor(nextCollapsed), nextX)),
    y: Math.max(VIEWPORT_MARGIN, Math.min(maxPanelYFor(nextCollapsed), nextY)),
  }
}

function defaultPanelState(nextCollapsed = true) {
  const expandedPosition = clampPanelPosition(
    window.innerWidth - PANEL_WIDTH - VIEWPORT_MARGIN,
    window.innerHeight - PANEL_HEIGHT - VIEWPORT_MARGIN,
    false,
  )
  const collapsedPosition = clampPanelPosition(
    window.innerWidth - FAB_WIDTH - VIEWPORT_MARGIN,
    window.innerHeight - FAB_HEIGHT - VIEWPORT_MARGIN,
    true,
  )
  return {
    collapsed: nextCollapsed,
    panelX: nextCollapsed ? collapsedPosition.x : expandedPosition.x,
    panelY: nextCollapsed ? collapsedPosition.y : expandedPosition.y,
    expandedPanelX: expandedPosition.x,
    expandedPanelY: expandedPosition.y,
    collapsedPanelX: collapsedPosition.x,
    collapsedPanelY: collapsedPosition.y,
    currentSessionId: '',
  }
}

function panelSize(nextCollapsed: boolean) {
  return {
    width: nextCollapsed ? FAB_WIDTH : PANEL_WIDTH,
    height: nextCollapsed ? FAB_HEIGHT : Math.min(PANEL_HEIGHT, Math.max(420, window.innerHeight - 112)),
  }
}

function toggleCollapse(nextCollapsed: boolean) {
  if (collapsed.value === nextCollapsed) return
  if (collapsed.value) {
    collapsedPanelX.value = panelX.value
    collapsedPanelY.value = panelY.value
  } else {
    expandedPanelX.value = panelX.value
    expandedPanelY.value = panelY.value
  }

  const nextPosition = clampPanelPosition(
    nextCollapsed ? collapsedPanelX.value : expandedPanelX.value,
    nextCollapsed ? collapsedPanelY.value : expandedPanelY.value,
    nextCollapsed,
  )
  panelX.value = nextPosition.x
  panelY.value = nextPosition.y
  collapsed.value = nextCollapsed
}

function persistPanelState() {
  localStorage.setItem(
    PANEL_STATE_KEY,
    JSON.stringify({
      collapsed: collapsed.value,
      panelX: panelX.value,
      panelY: panelY.value,
      expandedPanelX: expandedPanelX.value,
      expandedPanelY: expandedPanelY.value,
      collapsedPanelX: collapsedPanelX.value,
      collapsedPanelY: collapsedPanelY.value,
      currentSessionId: currentSessionId.value,
    }),
  )
}

function persistSessions() {
  localStorage.setItem(SESSIONS_KEY, JSON.stringify(sessions.value))
}

function createProviderDraft(seed = ''): ProviderDraft {
  return {
    id: null,
    name: seed || `供应商 ${providers.value.length + 1}`,
    providerKind: 'openai',
    baseUrl: providerKindDefaults.openai.baseUrl,
    apiKey: '',
  }
}

function normalizeOptionalTimestamp(value: unknown) {
  if (typeof value === 'string' && value.trim()) {
    const timestamp = new Date(value).getTime()
    return Number.isFinite(timestamp) ? timestamp : null
  }
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value
  }
  return null
}

function formatOptionalTimestamp(value: number | null) {
  if (!value) return '未同步'
  return formatSessionTime(value)
}

function normalizeMcpTransport(value: unknown): McpTransport | null {
  if (typeof value !== 'string') return null
  const normalized = value.trim().toLowerCase().replace(/_/g, '-')
  if (normalized === 'streamable-http') return 'streamable-http'
  if (normalized === 'stdio') return 'stdio'
  if (normalized === 'sse') return 'sse'
  return null
}

function normalizeMcpAuthType(value: unknown): McpAuthType {
  if (typeof value !== 'string') return 'none'
  const normalized = value.trim().toLowerCase().replace(/_/g, '-')
  if (normalized === 'bearer' || normalized === 'basic' || normalized === 'header' || normalized === 'query') {
    return normalized
  }
  return 'none'
}

function mcpTransportLabel(transport: string) {
  if (transport === 'streamable-http') return 'Streamable HTTP'
  if (transport === 'stdio') return 'STDIO'
  return 'SSE'
}

function mcpStatusLabel(status: string | null | undefined) {
  if (!status) return '未检测'
  if (status === 'ready') return '已就绪'
  if (status === 'error') return '异常'
  if (status === 'disabled') return '已禁用'
  return status
}

function normalizeMcpSecretEntry(raw: unknown): McpSecretEntry | null {
  if (!raw || typeof raw !== 'object') return null
  const name = typeof (raw as any).name === 'string' ? (raw as any).name.trim() : ''
  if (!name) return null
  return {
    name,
    hasValue: (raw as any).has_value === true || (raw as any).hasValue === true,
    value: typeof (raw as any).value === 'string' ? (raw as any).value : '',
  }
}

function normalizeMcpAuthDetail(raw: unknown): McpAuthDetail | null {
  if (!raw || typeof raw !== 'object') return null
  return {
    authType: normalizeMcpAuthType((raw as any).auth_type ?? (raw as any).authType),
    scheme: typeof (raw as any).scheme === 'string' ? (raw as any).scheme : '',
    username: typeof (raw as any).username === 'string' ? (raw as any).username : '',
    headerName: typeof (raw as any).header_name === 'string' ? (raw as any).header_name : '',
    queryName: typeof (raw as any).query_name === 'string' ? (raw as any).query_name : '',
    tokenHasValue: (raw as any).token?.has_value === true || (raw as any).token?.hasValue === true,
    passwordHasValue: (raw as any).password?.has_value === true || (raw as any).password?.hasValue === true,
    valueHasValue: (raw as any).value?.has_value === true || (raw as any).value?.hasValue === true,
    tokenValue: typeof (raw as any).token?.value === 'string' ? (raw as any).token.value : '',
    passwordValue: typeof (raw as any).password?.value === 'string' ? (raw as any).password.value : '',
    valueText: typeof (raw as any).value?.value === 'string' ? (raw as any).value.value : '',
  }
}

function normalizeMcpServerSummary(raw: unknown): McpServerSummary | null {
  if (!raw || typeof raw !== 'object') return null
  const transport = normalizeMcpTransport((raw as any).transport)
  if (!transport) return null
  return {
    id: `${(raw as any).id ?? ''}`.trim() || genId(),
    name: typeof (raw as any).name === 'string' && (raw as any).name.trim() ? (raw as any).name.trim() : '未命名 MCP',
    enabled: (raw as any).enabled === true,
    transport,
    url: typeof (raw as any).url === 'string' ? (raw as any).url.trim() : '',
    command: typeof (raw as any).command === 'string' ? (raw as any).command.trim() : '',
    authType: normalizeMcpAuthType((raw as any).auth_type ?? (raw as any).authType),
    lastStatus: typeof (raw as any).last_status === 'string'
      ? (raw as any).last_status
      : typeof (raw as any).lastStatus === 'string'
        ? (raw as any).lastStatus
        : null,
    lastError: typeof (raw as any).last_error === 'string'
      ? (raw as any).last_error
      : typeof (raw as any).lastError === 'string'
        ? (raw as any).lastError
        : null,
    configVersion: Number.isFinite((raw as any).config_version) ? Number((raw as any).config_version) : Number((raw as any).configVersion) || 1,
    createdAt: normalizeOptionalTimestamp((raw as any).created_at ?? (raw as any).createdAt) || Date.now(),
    updatedAt: normalizeOptionalTimestamp((raw as any).updated_at ?? (raw as any).updatedAt) || Date.now(),
  }
}

function normalizeMcpServerDetail(raw: unknown): McpServerDetail | null {
  const summary = normalizeMcpServerSummary(raw)
  if (!summary || !raw || typeof raw !== 'object') return null
  return {
    ...summary,
    args: Array.isArray((raw as any).args)
      ? (raw as any).args
        .filter((value: unknown): value is string => typeof value === 'string')
        .map((value: string) => value.trim())
        .filter(Boolean)
      : [],
    auth: normalizeMcpAuthDetail((raw as any).auth),
    customHeaders: Array.isArray((raw as any).custom_headers ?? (raw as any).customHeaders)
      ? ((raw as any).custom_headers ?? (raw as any).customHeaders)
        .map((item: unknown) => normalizeMcpSecretEntry(item))
        .filter((item: McpSecretEntry | null): item is McpSecretEntry => Boolean(item))
      : [],
    stdioEnv: Array.isArray((raw as any).stdio_env ?? (raw as any).stdioEnv)
      ? ((raw as any).stdio_env ?? (raw as any).stdioEnv)
        .map((item: unknown) => normalizeMcpSecretEntry(item))
        .filter((item: McpSecretEntry | null): item is McpSecretEntry => Boolean(item))
      : [],
    toolsSnapshot: Array.isArray((raw as any).tools_snapshot ?? (raw as any).toolsSnapshot) ? ((raw as any).tools_snapshot ?? (raw as any).toolsSnapshot) : [],
    resourcesSnapshot: Array.isArray((raw as any).resources_snapshot ?? (raw as any).resourcesSnapshot) ? ((raw as any).resources_snapshot ?? (raw as any).resourcesSnapshot) : [],
    promptsSnapshot: Array.isArray((raw as any).prompts_snapshot ?? (raw as any).promptsSnapshot) ? ((raw as any).prompts_snapshot ?? (raw as any).promptsSnapshot) : [],
    lastSyncAt: normalizeOptionalTimestamp((raw as any).last_sync_at ?? (raw as any).lastSyncAt),
  }
}

function createMcpDraft(seed = ''): McpDraft {
  return {
    id: null,
    name: seed || `MCP 服务 ${mcpServers.value.length + 1}`,
    enabled: true,
    transport: 'sse',
    url: '',
    command: '',
    argsText: '',
    authType: 'none',
    bearerScheme: 'Bearer',
    bearerToken: '',
    authUsername: '',
    authPassword: '',
    authHeaderName: 'Authorization',
    authHeaderValue: '',
    authQueryName: 'token',
    authQueryValue: '',
    customHeadersMode: 'replace',
    customHeadersText: '',
    stdioEnvMode: 'replace',
    stdioEnvText: '',
    existingCustomHeaders: [],
    existingStdioEnv: [],
    authSecretFlags: {
      token: false,
      password: false,
      value: false,
    },
    lastStatus: null,
    lastError: null,
    configVersion: 1,
    toolsSnapshot: [],
    resourcesSnapshot: [],
    promptsSnapshot: [],
    lastSyncAt: null,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }
}

function normalizePersistedMcpDraftId(value: string | null | undefined) {
  if (!value) return null
  const numeric = Number(value)
  if (!Number.isFinite(numeric) || numeric <= 0) return null
  return `${numeric}`
}

function createMcpDraftFromDetail(detail: McpServerDetail): McpDraft {
  const customHeadersText = detail.customHeaders
    .map((entry) => `${entry.name}: ${entry.value}`)
    .join('\n')
  const stdioEnvText = detail.stdioEnv
    .map((entry) => `${entry.name}=${entry.value}`)
    .join('\n')
  return {
    id: normalizePersistedMcpDraftId(detail.id),
    name: detail.name,
    enabled: detail.enabled,
    transport: detail.transport,
    url: detail.url,
    command: detail.command,
    argsText: detail.args.join('\n'),
    authType: detail.auth?.authType || 'none',
    bearerScheme: detail.auth?.scheme || 'Bearer',
    bearerToken: detail.auth?.tokenValue || '',
    authUsername: detail.auth?.username || '',
    authPassword: detail.auth?.passwordValue || '',
    authHeaderName: detail.auth?.headerName || 'Authorization',
    authHeaderValue: detail.auth?.authType === 'header' ? detail.auth.valueText : '',
    authQueryName: detail.auth?.queryName || 'token',
    authQueryValue: detail.auth?.authType === 'query' ? detail.auth.valueText : '',
    customHeadersMode: 'replace',
    customHeadersText,
    stdioEnvMode: 'replace',
    stdioEnvText,
    existingCustomHeaders: [...detail.customHeaders],
    existingStdioEnv: [...detail.stdioEnv],
    authSecretFlags: {
      token: detail.auth?.tokenHasValue === true,
      password: detail.auth?.passwordHasValue === true,
      value: detail.auth?.valueHasValue === true,
    },
    lastStatus: detail.lastStatus,
    lastError: detail.lastError,
    configVersion: detail.configVersion,
    toolsSnapshot: [...detail.toolsSnapshot],
    resourcesSnapshot: [...detail.resourcesSnapshot],
    promptsSnapshot: [...detail.promptsSnapshot],
    lastSyncAt: detail.lastSyncAt,
    createdAt: detail.createdAt,
    updatedAt: detail.updatedAt,
  }
}

function normalizeMcpRuntimeCapabilities(raw: unknown): McpRuntimeCapabilities {
  const transports = Array.isArray((raw as any)?.transports)
    ? (raw as any).transports
      .map((value: unknown) => normalizeMcpTransport(value))
      .filter((value: McpTransport | null, index: number, list: Array<McpTransport | null>): value is McpTransport => Boolean(value) && list.indexOf(value) === index)
    : ['sse', 'streamable-http']
  return {
    transports: transports.length ? transports : ['sse', 'streamable-http'],
    stdioEnabled: (raw as any)?.stdio_enabled === true || (raw as any)?.stdioEnabled === true,
    stdioAllowedCommands: Array.isArray((raw as any)?.stdio_allowed_commands ?? (raw as any)?.stdioAllowedCommands)
      ? ((raw as any).stdio_allowed_commands ?? (raw as any).stdioAllowedCommands)
        .filter((value: unknown): value is string => typeof value === 'string')
        .map((value: string) => value.trim())
        .filter(Boolean)
      : [],
  }
}

function mcpSnapshotEntryTitle(entry: unknown, index: number) {
  if (typeof entry === 'string' && entry.trim()) return entry.trim()
  if (entry && typeof entry === 'object') {
    const anyEntry = entry as Record<string, unknown>
    const candidate = anyEntry.title || anyEntry.name || anyEntry.uri || anyEntry.path || anyEntry.id
    if (typeof candidate === 'string' && candidate.trim()) return candidate.trim()
  }
  return `项目 ${index + 1}`
}

function mcpSnapshotEntryDetail(entry: unknown) {
  try {
    return JSON.stringify(entry, null, 2)
  } catch {
    return String(entry)
  }
}

function normalizeProvider(raw: any): AgentProvider | null {
  if (!raw || typeof raw !== 'object') return null

  const id = `${raw.id ?? ''}`.trim() || genId()
  const name = typeof raw.name === 'string' && raw.name.trim() ? raw.name.trim() : '未命名供应商'
  const remoteModels = uniqueStrings(Array.isArray(raw.remote_models ?? raw.remoteModels) ? (raw.remote_models ?? raw.remoteModels) : [])
  const customModels = uniqueStrings(Array.isArray(raw.custom_models ?? raw.customModels) ? (raw.custom_models ?? raw.customModels) : [])
  const enabledModels = uniqueStrings(Array.isArray(raw.enabled_models ?? raw.enabledModels) ? (raw.enabled_models ?? raw.enabledModels) : [])

  return {
    id,
    name,
    providerKind: normalizeProviderKind(raw.provider_kind ?? raw.providerKind),
    baseUrl: normalizedProviderBaseUrl(raw.base_url ?? raw.baseUrl),
    hasApiKey: Boolean(raw.has_api_key ?? raw.hasApiKey),
    remoteModels,
    enabledModels,
    customModels,
    modelConfigs: Object.fromEntries(
      Object.entries(raw.model_configs ?? raw.modelConfigs ?? {})
        .map(([model, config]) => [model, normalizeModelConfig(config as Partial<ProviderModelConfig>)])
        .filter(([model]) => typeof model === 'string' && Boolean(model.trim())),
    ),
    createdAt: raw.created_at ? new Date(raw.created_at).getTime() : Number.isFinite(raw.createdAt) ? raw.createdAt : Date.now(),
    updatedAt: raw.updated_at ? new Date(raw.updated_at).getTime() : Number.isFinite(raw.updatedAt) ? raw.updatedAt : Date.now(),
  }
}

async function refreshProvidersState() {
  const data = await request.get('/agent/providers') as ProviderListResponse
  const normalizedProviders = (data.providers || [])
    .map((provider) => normalizeProvider(provider))
    .filter((provider): provider is AgentProvider => Boolean(provider))

  providers.value = normalizedProviders
  activeProviderId.value = normalizedProviders.some((provider) => provider.id === `${data.active_provider_id ?? ''}`)
    ? `${data.active_provider_id ?? ''}`
    : normalizedProviders.find((provider) => provider.id === activeProviderId.value)?.id || normalizedProviders[0]?.id || ''
}

function normalizeMessage(raw: any): AgentMessage | null {
  if (!raw || typeof raw !== 'object') return null
  const role = raw.role === 'assistant'
    ? 'assistant'
    : raw.role === 'user'
      ? 'user'
      : raw.role === 'system'
        ? 'system'
        : null
  if (!role) return null

  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : genId(),
    role,
    content: typeof raw.content === 'string' ? raw.content : '',
    reasoning: typeof raw.reasoning === 'string' ? raw.reasoning : '',
    internalStatus: raw.internalStatus === true || raw.internal_status === true,
    attachments: Array.isArray(raw.attachments)
      ? raw.attachments
        .map((attachment: any) => normalizeAttachment(attachment))
        .filter((attachment: AgentAttachment | null): attachment is AgentAttachment => Boolean(attachment))
      : [],
  }
}

function normalizeTaskAnalysis(raw: any): AgentTaskAnalysis | null {
  if (!raw || typeof raw !== 'object') return null
  const intent = typeof raw.intent === 'string' && raw.intent.trim()
    ? raw.intent.trim()
    : AGENT_TASK_ANALYSIS_INTENTS[0] || ''
  const complexity = typeof raw.complexity === 'string' && AGENT_TASK_ANALYSIS_COMPLEXITIES.includes(raw.complexity.trim() as any)
    ? raw.complexity.trim()
    : AGENT_TASK_ANALYSIS_COMPLEXITIES[0] || ''
  const mode = typeof raw.mode === 'string' && AGENT_TASK_ANALYSIS_MODES.includes(raw.mode.trim() as any)
    ? raw.mode.trim()
    : AGENT_TASK_ANALYSIS_MODES[0] || 'chat'
  const writeScope = typeof raw.writeScope === 'string' && AGENT_TASK_ANALYSIS_WRITE_SCOPES.includes(raw.writeScope.trim() as any)
    ? raw.writeScope.trim()
    : typeof raw.write_scope === 'string' && AGENT_TASK_ANALYSIS_WRITE_SCOPES.includes(raw.write_scope.trim() as any)
      ? raw.write_scope.trim()
      : null
  return {
    intent,
    complexity,
    mode,
    requiresTools: raw.requiresTools === true || raw.requires_tools === true,
    requiresUserConfirmation: raw.requiresUserConfirmation === true || raw.requires_user_confirmation === true,
    writeScope,
    preferredWriteAction: typeof raw.preferredWriteAction === 'string' && raw.preferredWriteAction.trim()
      ? raw.preferredWriteAction.trim()
      : typeof raw.preferred_write_action === 'string' && raw.preferred_write_action.trim()
        ? raw.preferred_write_action.trim()
        : null,
    deliverable: typeof raw.deliverable === 'string' && raw.deliverable.trim() ? raw.deliverable.trim() : null,
  }
}

function normalizeSessionMemory(raw: any): AgentSessionMemory | null {
  if (!raw || typeof raw !== 'object') return null
  const normalizeItems = (value: unknown) => Array.isArray(value)
    ? value
      .filter((item: unknown): item is string => typeof item === 'string' && Boolean(item.trim()))
      .map((item: string) => item.trim())
    : []
  const summary = typeof raw.summary === 'string' && raw.summary.trim() ? raw.summary.trim() : null
  const activeUserGoals = normalizeItems(raw.activeUserGoals ?? raw.active_user_goals)
  const completedFacts = normalizeItems(raw.completedFacts ?? raw.completed_facts)
  const openLoops = normalizeItems(raw.openLoops ?? raw.open_loops)
  if (!summary && !activeUserGoals.length && !completedFacts.length && !openLoops.length) return null
  return {
    summary,
    activeUserGoals,
    completedFacts,
    openLoops,
    updatedAt: typeof raw.updatedAt === 'string'
      ? raw.updatedAt
      : typeof raw.updated_at === 'string'
        ? raw.updated_at
        : null,
  }
}

function normalizeRuntimePlanStep(raw: any, fallbackIndex = 0): AgentRuntimePlanStep | null {
  if (!raw || typeof raw !== 'object') return null
  const title = typeof raw.title === 'string' && raw.title.trim() ? raw.title.trim() : ''
  if (!title) return null
  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : `step_${fallbackIndex + 1}`,
    title,
    kind: typeof raw.kind === 'string' && raw.kind.trim() ? raw.kind.trim() : 'edit',
    description: typeof raw.description === 'string' ? raw.description.trim() : title,
    status: typeof raw.status === 'string' && raw.status.trim() ? raw.status.trim() : 'pending',
    toolHints: Array.isArray(raw.toolHints ?? raw.tool_hints)
      ? (raw.toolHints ?? raw.tool_hints)
        .filter((item: unknown): item is string => typeof item === 'string' && Boolean(item.trim()))
        .map((item: string) => item.trim())
      : [],
    requiresConfirmation: raw.requiresConfirmation === true || raw.requires_confirmation === true,
    requiresDocumentWrite: raw.requiresDocumentWrite === true || raw.requires_document_write === true,
    requiresDocumentSave: raw.requiresDocumentSave === true || raw.requires_document_save === true,
  }
}

function normalizeRuntimePlan(raw: any): AgentRuntimePlan | null {
  if (!raw || typeof raw !== 'object') return null
  const goal = typeof raw.goal === 'string' && raw.goal.trim() ? raw.goal.trim() : ''
  const steps = Array.isArray(raw.steps)
    ? raw.steps
      .map((step: any, index: number) => normalizeRuntimePlanStep(step, index))
      .filter((step: AgentRuntimePlanStep | null): step is AgentRuntimePlanStep => Boolean(step))
    : []
  if (!goal && !steps.length) return null
  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : genId(),
    goal: goal || '执行计划',
    summary: typeof raw.summary === 'string' && raw.summary.trim() ? raw.summary.trim() : null,
    status: typeof raw.status === 'string' && raw.status.trim() ? raw.status.trim() : 'pending',
    steps,
    createdAt: typeof raw.createdAt === 'string' ? raw.createdAt : typeof raw.created_at === 'string' ? raw.created_at : null,
    updatedAt: typeof raw.updatedAt === 'string' ? raw.updatedAt : typeof raw.updated_at === 'string' ? raw.updated_at : null,
  }
}

function normalizeStructuredResponse(raw: any): AgentStructuredResponse | null {
  if (!raw || typeof raw !== 'object') return null
  const message = typeof raw.message === 'string' ? raw.message : ''
  const state = raw.state && typeof raw.state === 'object'
    ? normalizeControlPayload(raw.state)
    : null
  const plan = normalizeRuntimePlan(raw.plan)
  if (!message.trim() && !state && !plan) return null
  return {
    message,
    state,
    plan,
  }
}

function normalizeArtifact(raw: any): AgentArtifact | null {
  if (!raw || typeof raw !== 'object') return null
  const title = typeof raw.title === 'string' && raw.title.trim() ? raw.title.trim() : ''
  if (!title) return null
  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : genId(),
    type: typeof raw.type === 'string' && raw.type.trim() ? raw.type.trim() : 'markdown_doc',
    title,
    status: typeof raw.status === 'string' && raw.status.trim() ? raw.status.trim() : 'drafting',
    content: typeof raw.content === 'string' ? raw.content : '',
    relatedDocId: Number.isFinite(raw.relatedDocId) ? Number(raw.relatedDocId) : Number.isFinite(raw.related_doc_id) ? Number(raw.related_doc_id) : null,
  }
}

function normalizeToolEvent(raw: any): AgentToolEvent | null {
  if (!raw || typeof raw !== 'object') return null
  const tool = typeof raw.tool === 'string' && raw.tool.trim() ? raw.tool.trim() : ''
  const summary = typeof raw.summary === 'string' && raw.summary.trim() ? raw.summary.trim() : ''
  if (!tool && !summary) return null
  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : genId(),
    tool: tool || 'tool',
    status: typeof raw.status === 'string' && raw.status.trim() ? raw.status.trim() : 'completed',
    summary: summary || `已执行工具 ${tool}`,
  }
}

function normalizePlanStepTitle(value: string) {
  return value.trim()
}

function resolvePlanStepIndicesByTitles(
  steps: AgentRuntimePlanStep[],
  titles: string[],
  options: { skipIndices?: Set<number> } = {},
) {
  const skipIndices = options.skipIndices || new Set<number>()
  const usedIndices = new Set<number>()
  const resolved: number[] = []

  for (const title of titles.map(normalizePlanStepTitle).filter(Boolean)) {
    const index = steps.findIndex((step, stepIndex) => (
      !skipIndices.has(stepIndex)
      && !usedIndices.has(stepIndex)
      && normalizePlanStepTitle(step.title) === title
    ))
    if (index === -1) continue
    usedIndices.add(index)
    resolved.push(index)
  }

  return resolved
}

function resolveCurrentPlanStepIndexByTitle(
  steps: AgentRuntimePlanStep[],
  title: string,
  completedIndices: Set<number>,
) {
  const normalized = normalizePlanStepTitle(title)
  if (!normalized) return -1
  const nextIncompleteMatch = steps.findIndex((step, index) => (
    !completedIndices.has(index) && normalizePlanStepTitle(step.title) === normalized
  ))
  if (nextIncompleteMatch >= 0) return nextIncompleteMatch
  return steps.findIndex((step) => normalizePlanStepTitle(step.title) === normalized)
}

function buildRuntimePlanFromText(planText: string, goal: string, templatePlan: AgentRuntimePlan | null = null) {
  const parsedTitles = parsePlanSteps(planText)
  const reusesTemplateShape = Boolean(
    templatePlan?.steps?.length === parsedTitles.length
    && parsedTitles.every((title, index) => normalizePlanStepTitle(templatePlan?.steps?.[index]?.title || '') === normalizePlanStepTitle(title)),
  )
  const steps = parsedTitles.map((title, index) => {
    const templateStep = templatePlan?.steps?.[index] || null
    const isSameStep = normalizePlanStepTitle(templateStep?.title || '') === normalizePlanStepTitle(title)
    return {
      id: isSameStep ? (templateStep?.id || `step_${index + 1}`) : `step_${index + 1}`,
      title,
      kind: isSameStep ? (templateStep?.kind || 'edit') : 'edit',
      description: title,
      status: isSameStep ? (templateStep?.status || 'pending') : 'pending',
      toolHints: isSameStep ? (templateStep?.toolHints || []) : [],
      requiresConfirmation: isSameStep && templateStep?.requiresConfirmation === true,
      requiresDocumentWrite: isSameStep && templateStep?.requiresDocumentWrite === true,
      requiresDocumentSave: isSameStep && templateStep?.requiresDocumentSave === true,
    }
  })

  if (!steps.length) return null
  const now = new Date().toISOString()
  return {
    id: reusesTemplateShape ? (templatePlan?.id || genId()) : genId(),
    goal: goal.trim() || '执行计划',
    summary: '由模型输出的正式执行计划',
    status: reusesTemplateShape ? (templatePlan?.status || 'pending') : 'pending',
    steps,
    createdAt: reusesTemplateShape ? (templatePlan?.createdAt || now) : now,
    updatedAt: now,
  } satisfies AgentRuntimePlan
}

function runtimePlanToPlanText(runtimePlan: AgentRuntimePlan | null) {
  if (!runtimePlan?.steps?.length) return ''
  return runtimePlan.steps
    .map((step, index) => `${index + 1}. ${step.title.trim()}`)
    .join('\n')
    .trim()
}

function normalizeSession(raw: any, provider: AgentProvider | null): AgentSession | null {
  if (!raw || typeof raw !== 'object') return null
  const messages = Array.isArray(raw.messages)
    ? raw.messages
      .map((message: any) => normalizeMessage(message))
      .filter((message: AgentMessage | null): message is AgentMessage => {
        if (!message) return false
        return !(message.role === 'system' && isInternalExecutionLedgerMessage(message.content))
      })
    : []

  return {
    id: typeof raw.id === 'string' && raw.id.trim() ? raw.id.trim() : genId(),
    title: typeof raw.title === 'string' && raw.title.trim() ? raw.title.trim() : '新会话',
    messages,
    taskAnalysis: normalizeTaskAnalysis(raw.taskAnalysis ?? raw.task_analysis),
    runtimePlan: normalizeRuntimePlan(raw.runtimePlan ?? raw.runtime_plan),
    artifacts: Array.isArray(raw.artifacts)
      ? raw.artifacts
        .map((artifact: any) => normalizeArtifact(artifact))
        .filter((artifact: AgentArtifact | null): artifact is AgentArtifact => Boolean(artifact))
      : [],
    toolEvents: Array.isArray(raw.toolEvents ?? raw.tool_events)
      ? (raw.toolEvents ?? raw.tool_events)
        .map((event: any) => normalizeToolEvent(event))
        .filter((event: AgentToolEvent | null): event is AgentToolEvent => Boolean(event))
      : [],
    providerId: typeof raw.providerId === 'string' && raw.providerId.trim() ? raw.providerId.trim() : provider?.id || null,
    model: typeof raw.model === 'string' ? raw.model.trim() : fallbackModelForProvider(provider),
    transportMode: raw.transportMode === 'responses' || raw.transportMode === 'chat' ? raw.transportMode : 'auto',
    previousResponseId: typeof raw.previousResponseId === 'string' && raw.previousResponseId.trim() ? raw.previousResponseId.trim() : null,
    pendingPlan: typeof raw.pendingPlan === 'string' && raw.pendingPlan.trim() ? raw.pendingPlan.trim() : null,
    pendingPlanToolOutputs: Array.isArray(raw.pendingPlanToolOutputs)
      ? raw.pendingPlanToolOutputs
        .map((output: any) => ({
          call_id: typeof output?.call_id === 'string' && output.call_id.trim() ? output.call_id.trim() : genId(),
          name: typeof output?.name === 'string' && output.name.trim() ? output.name.trim() : undefined,
          arguments: typeof output?.arguments === 'string' && output.arguments.trim() ? output.arguments.trim() : undefined,
          output: output?.output ?? null,
        }))
        .filter((output: AgentToolOutputPayload) => Boolean(output.call_id))
      : [],
    lastPlan: typeof raw.lastPlan === 'string' && raw.lastPlan.trim() ? raw.lastPlan.trim() : null,
    lastExecutionMemory: raw.lastExecutionMemory && typeof raw.lastExecutionMemory === 'object'
        ? {
            currentMode: typeof raw.lastExecutionMemory.currentMode === 'string' && raw.lastExecutionMemory.currentMode.trim()
              ? raw.lastExecutionMemory.currentMode.trim()
              : typeof raw.lastExecutionMemory.current_mode === 'string' && raw.lastExecutionMemory.current_mode.trim()
                ? raw.lastExecutionMemory.current_mode.trim()
                : 'normal',
            awaiting: typeof raw.lastExecutionMemory.awaiting === 'string' && raw.lastExecutionMemory.awaiting.trim()
              ? raw.lastExecutionMemory.awaiting.trim()
              : null,
            currentActionKind: typeof raw.lastExecutionMemory.currentActionKind === 'string' && raw.lastExecutionMemory.currentActionKind.trim()
              ? raw.lastExecutionMemory.currentActionKind.trim()
              : typeof raw.lastExecutionMemory.current_action_kind === 'string' && raw.lastExecutionMemory.current_action_kind.trim()
                ? raw.lastExecutionMemory.current_action_kind.trim()
                : null,
            currentActionStatus: typeof raw.lastExecutionMemory.currentActionStatus === 'string' && raw.lastExecutionMemory.currentActionStatus.trim()
              ? raw.lastExecutionMemory.currentActionStatus.trim()
              : typeof raw.lastExecutionMemory.current_action_status === 'string' && raw.lastExecutionMemory.current_action_status.trim()
                ? raw.lastExecutionMemory.current_action_status.trim()
                : null,
            currentActionMode: typeof raw.lastExecutionMemory.currentActionMode === 'string' && raw.lastExecutionMemory.currentActionMode.trim()
              ? raw.lastExecutionMemory.currentActionMode.trim()
              : typeof raw.lastExecutionMemory.current_action_mode === 'string' && raw.lastExecutionMemory.current_action_mode.trim()
                ? raw.lastExecutionMemory.current_action_mode.trim()
                : null,
            currentActionTarget: typeof raw.lastExecutionMemory.currentActionTarget === 'string' && raw.lastExecutionMemory.currentActionTarget.trim()
              ? raw.lastExecutionMemory.currentActionTarget.trim()
              : typeof raw.lastExecutionMemory.current_action_target === 'string' && raw.lastExecutionMemory.current_action_target.trim()
                ? raw.lastExecutionMemory.current_action_target.trim()
                : null,
            confirmationRequired: raw.lastExecutionMemory.confirmationRequired === true || raw.lastExecutionMemory.confirmation_required === true,
            plan: typeof raw.lastExecutionMemory.plan === 'string' && raw.lastExecutionMemory.plan.trim() ? raw.lastExecutionMemory.plan.trim() : null,
            assistantSummary: typeof raw.lastExecutionMemory.assistantSummary === 'string' && raw.lastExecutionMemory.assistantSummary.trim() ? raw.lastExecutionMemory.assistantSummary.trim() : null,
            controlPhase: typeof raw.lastExecutionMemory.controlPhase === 'string' && raw.lastExecutionMemory.controlPhase.trim() ? raw.lastExecutionMemory.controlPhase.trim() : null,
            taskKind: typeof raw.lastExecutionMemory.taskKind === 'string' && raw.lastExecutionMemory.taskKind.trim() ? raw.lastExecutionMemory.taskKind.trim() : null,
            editIntent: typeof raw.lastExecutionMemory.editIntent === 'string' && raw.lastExecutionMemory.editIntent.trim() ? raw.lastExecutionMemory.editIntent.trim() : null,
            editStage: typeof raw.lastExecutionMemory.editStage === 'string' && raw.lastExecutionMemory.editStage.trim() ? raw.lastExecutionMemory.editStage.trim() : null,
            saveRequested: raw.lastExecutionMemory.saveRequested === true,
            writeCompleted: raw.lastExecutionMemory.writeCompleted === true,
            planStepIndex: Number.isFinite(raw.lastExecutionMemory.planStepIndex) ? Number(raw.lastExecutionMemory.planStepIndex) : null,
            planTotalSteps: Number.isFinite(raw.lastExecutionMemory.planTotalSteps) ? Number(raw.lastExecutionMemory.planTotalSteps) : null,
            planCurrentStep: typeof raw.lastExecutionMemory.planCurrentStep === 'string' && raw.lastExecutionMemory.planCurrentStep.trim() ? raw.lastExecutionMemory.planCurrentStep.trim() : null,
            planCompletedSteps: Array.isArray(raw.lastExecutionMemory.planCompletedSteps)
              ? raw.lastExecutionMemory.planCompletedSteps
                .filter((item: unknown): item is string => typeof item === 'string' && Boolean(item.trim()))
                .map((item: string) => item.trim())
              : [],
            documentWriteObserved: raw.lastExecutionMemory.documentWriteObserved === true,
            saveAttemptWithoutDocumentChange: raw.lastExecutionMemory.saveAttemptWithoutDocumentChange === true,
            lastInterceptCode: typeof raw.lastExecutionMemory.lastInterceptCode === 'string' && raw.lastExecutionMemory.lastInterceptCode.trim()
              ? raw.lastExecutionMemory.lastInterceptCode.trim()
              : null,
            lastInterceptMessage: typeof raw.lastExecutionMemory.lastInterceptMessage === 'string' && raw.lastExecutionMemory.lastInterceptMessage.trim()
              ? raw.lastExecutionMemory.lastInterceptMessage.trim()
              : null,
            lastInterceptGuidance: typeof raw.lastExecutionMemory.lastInterceptGuidance === 'string' && raw.lastExecutionMemory.lastInterceptGuidance.trim()
              ? raw.lastExecutionMemory.lastInterceptGuidance.trim()
              : null,
            recentToolCalls: Array.isArray(raw.lastExecutionMemory.recentToolCalls)
            ? raw.lastExecutionMemory.recentToolCalls
              .map((call: any) => ({
                name: typeof call?.name === 'string' ? call.name : '',
                arguments: typeof call?.arguments === 'string' && call.arguments.trim() ? call.arguments.trim() : null,
                output: typeof call?.output === 'string' && call.output.trim() ? call.output.trim() : null,
                ok: typeof call?.ok === 'boolean' ? call.ok : null,
                outcome: call?.outcome === 'success' || call?.outcome === 'noop' || call?.outcome === 'error'
                  ? call.outcome
                  : 'unknown',
              }))
              .filter((call: AgentExecutionToolCallSummary) => Boolean(call.name.trim()))
            : [],
        }
      : null,
    sessionMemory: normalizeSessionMemory(raw.sessionMemory ?? raw.session_memory),
    lastSyncedMessageCount: Number.isInteger(raw.lastSyncedMessageCount) ? Math.max(0, raw.lastSyncedMessageCount) : 0,
    createdAt: Number.isFinite(raw.createdAt) ? raw.createdAt : Date.now(),
    updatedAt: Number.isFinite(raw.updatedAt) ? raw.updatedAt : Date.now(),
  }
}

function loadSessions(provider: AgentProvider | null) {
  return loadJson<any[]>(SESSIONS_KEY, [])
    .map((session) => normalizeSession(session, provider))
    .filter((session): session is AgentSession => Boolean(session))
}

function ensureProviderDraftLoaded() {
  if (providerDraft.value.id && providers.value.some((provider) => provider.id === providerDraft.value.id)) return
  if (activeProvider.value) {
    editProvider(activeProvider.value.id)
    return
  }
  if (providers.value[0]) {
    editProvider(providers.value[0].id)
    return
  }
  providerDraft.value = createProviderDraft()
}

async function refreshMcpManagementState() {
  mcpLoading.value = true
  try {
    const [runtimeData, settingsData, listData] = await Promise.all([
      request.get('/agent/mcps/runtime-capabilities') as Promise<McpRuntimeCapabilitiesApiResponse>,
      request.get('/agent/mcps/settings') as Promise<McpSettingsApiResponse>,
      request.get('/agent/mcps') as Promise<McpServersResponse>,
    ])

    mcpRuntimeCapabilities.value = normalizeMcpRuntimeCapabilities(runtimeData)
    mcpSettings.value = {
      enabled: settingsData.settings?.enabled === true,
      updatedAt: normalizeOptionalTimestamp(settingsData.settings?.updated_at ?? null),
    }
    mcpSettingsEnabled.value = mcpSettings.value.enabled
    mcpServers.value = (listData.servers || [])
      .map((server) => normalizeMcpServerSummary(server))
      .filter((server): server is McpServerSummary => Boolean(server))

    if (mcpDraft.value.id && mcpServers.value.some((server) => server.id === mcpDraft.value.id)) {
      await editMcpServer(mcpDraft.value.id, { silent: true })
      return
    }
    if (mcpServers.value[0]) {
      await editMcpServer(mcpServers.value[0].id, { silent: true })
      return
    }

    const nextDraft = createMcpDraft()
    if (!mcpRuntimeCapabilities.value.stdioEnabled && nextDraft.transport === 'stdio') {
      nextDraft.transport = 'sse'
    }
    mcpDraft.value = nextDraft
    activeMcpSnapshotTab.value = 'tools'
  } catch (error: any) {
    logAgentPanelError('refresh_mcp_state', error)
    ElMessage.error(error.response?.data?.error || error.message || '加载 MCP 配置失败')
  } finally {
    mcpLoading.value = false
  }
}

async function editMcpServer(serverId: string, options: { silent?: boolean } = {}) {
  if (!serverId) return
  try {
    const data = await request.get(`/agent/mcps/${serverId}`) as McpServerResponse
    const detail = normalizeMcpServerDetail(data.server)
    if (!detail) {
      throw new Error('MCP 服务详情缺失')
    }
    mcpDraft.value = createMcpDraftFromDetail(detail)
    activeMcpSnapshotTab.value = 'tools'
  } catch (error: any) {
    logAgentPanelError('edit_mcp_server', error, { serverId })
    if (!options.silent) {
      ElMessage.error(error.response?.data?.error || error.message || '读取 MCP 服务详情失败')
    }
  }
}

async function startCreateMcpServer() {
  const draft = createMcpDraft()
  draft.enabled = false
  if (!mcpRuntimeCapabilities.value.stdioEnabled && draft.transport === 'stdio') {
    draft.transport = 'sse'
  }
  await createPersistedMcpServer(draft, '已新增 MCP 服务')
}

async function duplicateMcpServer() {
  const current = mcpDraft.value
  const next = createMcpDraft(current.name ? `${current.name} 副本` : '')
  next.enabled = false
  next.transport = current.transport === 'stdio' && !mcpRuntimeCapabilities.value.stdioEnabled ? 'sse' : current.transport
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
  next.authSecretFlags = {
    token: false,
    password: false,
    value: false,
  }
  await createPersistedMcpServer(next, '已复制 MCP 服务')
}

async function handleMcpSettingsChange(value: string | number | boolean) {
  const enabled = Boolean(value)
  const previous = mcpSettings.value.enabled
  mcpSettingsSaving.value = true
  try {
    const data = await request.post('/agent/mcps/settings', {
      enabled,
    }) as McpSettingsApiResponse
    mcpSettings.value = {
      enabled: data.settings?.enabled === true,
      updatedAt: normalizeOptionalTimestamp(data.settings?.updated_at ?? null),
    }
    mcpSettingsEnabled.value = mcpSettings.value.enabled
    ElMessage.success(mcpSettings.value.enabled ? '已启用 MCP' : '已关闭 MCP')
  } catch (error: any) {
    mcpSettingsEnabled.value = previous
    logAgentPanelError('save_mcp_settings', error)
    ElMessage.error(error.response?.data?.error || error.message || '保存 MCP 开关失败')
  } finally {
    mcpSettingsSaving.value = false
  }
}

function buildMcpSecretLinesPayload(
  mode: McpSecretCollectionMode,
  text: string,
  options: { allowKeep?: boolean } = {},
) {
  if (mode === 'keep' && options.allowKeep !== false) return { mode: 'keep' }
  if (mode === 'keep') {
    return {
      mode: 'replace',
      value: text.split('\n').map((line) => line.trim()).filter(Boolean),
    }
  }
  if (mode === 'clear') return { mode: 'clear' }
  return {
    mode: 'replace',
    value: text.split('\n').map((line) => line.trim()).filter(Boolean),
  }
}

function validateMcpDraft(options: { requireCompleteConnection?: boolean } = {}) {
  const requireCompleteConnection = options.requireCompleteConnection !== false
  const name = mcpDraft.value.name.trim()
  if (!name) return '请填写 MCP 名称'

  if (mcpDraft.value.transport === 'stdio') {
    if (!mcpRuntimeCapabilities.value.stdioEnabled) {
      return '当前后端没有开启 stdio'
    }
    if (requireCompleteConnection && !mcpDraft.value.command.trim()) {
      return '请填写 STDIO Command'
    }
    return ''
  }

  if (requireCompleteConnection && !mcpDraft.value.url.trim()) {
    return '请填写 HTTP URL'
  }

  if (mcpDraft.value.authType === 'bearer' && !mcpDraft.value.bearerToken.trim() && !mcpDraft.value.authSecretFlags.token) {
    return '请填写 Bearer Token'
  }
  if (mcpDraft.value.authType === 'basic') {
    if (!mcpDraft.value.authUsername.trim()) return '请填写 Basic Auth 用户名'
    if (!mcpDraft.value.authPassword.trim() && !mcpDraft.value.authSecretFlags.password) {
      return '请填写 Basic Auth 密码'
    }
  }
  if (mcpDraft.value.authType === 'header') {
    if (!mcpDraft.value.authHeaderName.trim()) return '请填写 Header 名称'
    if (!mcpDraft.value.authHeaderValue.trim() && !mcpDraft.value.authSecretFlags.value) {
      return '请填写 Header 值'
    }
  }
  if (mcpDraft.value.authType === 'query') {
    if (!mcpDraft.value.authQueryName.trim()) return '请填写 Query 参数名'
    if (!mcpDraft.value.authQueryValue.trim() && !mcpDraft.value.authSecretFlags.value) {
      return '请填写 Query 参数值'
    }
  }
  return ''
}

function buildMcpAuthPayload(
  draft: McpDraft = mcpDraft.value,
  options: { allowKeep?: boolean } = {},
) {
  const allowKeep = options.allowKeep !== false
  if (draft.transport === 'stdio' || draft.authType === 'none') {
    return { mode: 'clear' }
  }

  if (draft.authType === 'bearer') {
    return {
      mode: 'replace',
      value: {
        type: 'bearer',
        scheme: draft.bearerScheme.trim() || null,
        token: draft.bearerToken.trim()
          ? { mode: 'replace', value: draft.bearerToken.trim() }
          : draft.authSecretFlags.token && allowKeep
            ? { mode: 'keep' }
            : { mode: 'clear' },
      },
    }
  }

  if (draft.authType === 'basic') {
    return {
      mode: 'replace',
      value: {
        type: 'basic',
        username: draft.authUsername.trim(),
        password: draft.authPassword.trim()
          ? { mode: 'replace', value: draft.authPassword.trim() }
          : draft.authSecretFlags.password && allowKeep
            ? { mode: 'keep' }
            : { mode: 'clear' },
      },
    }
  }

  if (draft.authType === 'header') {
    return {
      mode: 'replace',
      value: {
        type: 'header',
        name: draft.authHeaderName.trim(),
        value: draft.authHeaderValue.trim()
          ? { mode: 'replace', value: draft.authHeaderValue.trim() }
          : draft.authSecretFlags.value && allowKeep
            ? { mode: 'keep' }
            : { mode: 'clear' },
      },
    }
  }

  return {
    mode: 'replace',
    value: {
      type: 'query',
      name: draft.authQueryName.trim(),
      value: draft.authQueryValue.trim()
        ? { mode: 'replace', value: draft.authQueryValue.trim() }
        : draft.authSecretFlags.value && allowKeep
          ? { mode: 'keep' }
          : { mode: 'clear' },
    },
  }
}

function buildMcpDraftPayload(
  draft: McpDraft = mcpDraft.value,
  options: { allowKeep?: boolean } = {},
) {
  const persistedId = normalizePersistedMcpDraftId(draft.id)
  const allowKeep = options.allowKeep !== false && Boolean(persistedId)
  const isHttpTransport = draft.transport !== 'stdio'
  const normalizedUrl = draft.url.trim()
  const normalizedCommand = draft.command.trim()
  return {
    id: persistedId ? Number(persistedId) : null,
    name: draft.name.trim(),
    enabled: draft.enabled,
    transport: draft.transport,
    url: isHttpTransport ? (normalizedUrl || null) : null,
    command: draft.transport === 'stdio' ? (normalizedCommand || null) : null,
    args: draft.transport === 'stdio'
      ? draft.argsText.split('\n').map((line) => line.trim()).filter(Boolean)
      : [],
    auth: buildMcpAuthPayload(draft, { allowKeep }),
    custom_headers: isHttpTransport
      ? buildMcpSecretLinesPayload(draft.customHeadersMode, draft.customHeadersText, { allowKeep })
      : { mode: 'clear' },
    stdio_env: draft.transport === 'stdio'
      ? buildMcpSecretLinesPayload(draft.stdioEnvMode, draft.stdioEnvText, { allowKeep })
      : { mode: 'clear' },
  }
}

async function saveMcpServerDraft() {
  const validationMessage = validateMcpDraft({ requireCompleteConnection: mcpDraft.value.enabled })
  if (validationMessage) {
    ElMessage.warning(validationMessage)
    return
  }

  mcpSaving.value = true
  try {
    const payload = buildMcpDraftPayload()
    const data = await request.post('/agent/mcps', payload) as McpServerResponse
    const detail = normalizeMcpServerDetail(data.server)
    if (!detail) {
      throw new Error('MCP 保存结果缺失')
    }
    await refreshMcpManagementState()
    mcpDraft.value = createMcpDraftFromDetail(detail)
    ElMessage.success('MCP 服务已保存')
  } catch (error: any) {
    logAgentPanelError('save_mcp_server', error, { serverId: mcpDraft.value.id, transport: mcpDraft.value.transport })
    ElMessage.error(error.response?.data?.error || error.message || '保存 MCP 服务失败')
  } finally {
    mcpSaving.value = false
  }
}

async function removeMcpServer(serverId: string | null) {
  if (!serverId) return
  try {
    await request.delete(`/agent/mcps/${serverId}`)
    await refreshMcpManagementState()
    ElMessage.success('MCP 服务已删除')
  } catch (error: any) {
    logAgentPanelError('remove_mcp_server', error, { serverId })
    ElMessage.error(error.response?.data?.error || error.message || '删除 MCP 服务失败')
  }
}

async function runMcpServerAction(
  endpoint: 'test' | 'refresh',
) {
  const validationMessage = validateMcpDraft({ requireCompleteConnection: true })
  if (validationMessage) {
    ElMessage.warning(validationMessage)
    return
  }
  const loadingRef = endpoint === 'test' ? mcpTesting : mcpRefreshing
  loadingRef.value = true
  try {
    const data = await request.post(`/agent/mcps/draft/${endpoint}`, buildMcpDraftPayload()) as McpServerResponse
    const detail = normalizeMcpServerDetail(data.server)
    if (!detail) {
      throw new Error('MCP 刷新结果缺失')
    }
    mcpDraft.value = createMcpDraftFromDetail(detail)
    ElMessage.success(endpoint === 'test' ? 'MCP 测试连接成功' : 'MCP 能力已刷新')
  } catch (error: any) {
    const server = normalizeMcpServerDetail(error?.response?.data?.server)
    if (server) {
      mcpDraft.value = createMcpDraftFromDetail(server)
    }
    logAgentPanelError(`mcp_${endpoint}`, error, { serverId: mcpDraft.value.id, transport: mcpDraft.value.transport })
    ElMessage.error(error.response?.data?.error || error.message || (endpoint === 'test' ? '测试连接失败' : '刷新 MCP 能力失败'))
  } finally {
    loadingRef.value = false
  }
}

async function createPersistedMcpServer(
  draft: McpDraft,
  successMessage: string,
) {
  mcpSaving.value = true
  try {
    const payload = buildMcpDraftPayload(draft, { allowKeep: false })
    const data = await request.post('/agent/mcps', payload) as McpServerResponse
    const detail = normalizeMcpServerDetail(data.server)
    if (!detail) {
      throw new Error('MCP 创建结果缺失')
    }
    mcpDraft.value = createMcpDraftFromDetail(detail)
    await refreshMcpManagementState()
    ElMessage.success(successMessage)
  } catch (error: any) {
    logAgentPanelError('create_mcp_server', error, { sourceId: draft.id, transport: draft.transport })
    ElMessage.error(error.response?.data?.error || error.message || '创建 MCP 服务失败')
  } finally {
    mcpSaving.value = false
  }
}

async function testMcpServerConnection() {
  await runMcpServerAction('test')
}

async function refreshMcpServerCapabilities() {
  await runMcpServerAction('refresh')
}

function resetModelDraft() {
  const provider = activeProvider.value
  if (!provider) {
    modelDraft.value = { remoteModels: [], enabledModels: [], customModels: [] }
    return
  }

  modelDraft.value = {
    remoteModels: [...provider.remoteModels],
    enabledModels: [...provider.enabledModels],
    customModels: [...provider.customModels],
  }
}

function ensureSession(): AgentSession {
  let session = currentSession.value
  if (session) return session

  session = {
    id: genId(),
    title: '新会话',
    messages: [],
    taskAnalysis: null,
    runtimePlan: null,
    artifacts: [],
    toolEvents: [],
    providerId: activeProvider.value?.id || null,
    model: fallbackModelForProvider(activeProvider.value),
    transportMode: 'auto',
    previousResponseId: null,
    pendingPlan: null,
    pendingPlanToolOutputs: [],
    lastPlan: null,
    lastExecutionMemory: null,
    sessionMemory: null,
    lastSyncedMessageCount: 0,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }
  sessions.value.unshift(session)
  currentSessionId.value = session.id
  persistSessions()
  persistPanelState()
  return session
}

function createSession() {
  const session: AgentSession = {
    id: genId(),
    title: '新会话',
    messages: [],
    taskAnalysis: null,
    runtimePlan: null,
    artifacts: [],
    toolEvents: [],
    providerId: activeProvider.value?.id || null,
    model: fallbackModelForProvider(activeProvider.value),
    transportMode: 'auto',
    previousResponseId: null,
    pendingPlan: null,
    pendingPlanToolOutputs: [],
    lastPlan: null,
    lastExecutionMemory: null,
    sessionMemory: null,
    lastSyncedMessageCount: 0,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }
  sessions.value.unshift(session)
  currentSessionId.value = session.id
  prompt.value = ''
  persistSessions()
  persistPanelState()
  scrollMessagesToBottom()
}

function selectSession(sessionId: string) {
  currentSessionId.value = sessionId
  persistPanelState()
}

function deleteSession(sessionId: string) {
  sessions.value = sessions.value.filter((session) => session.id !== sessionId)
  if (currentSessionId.value === sessionId) {
    currentSessionId.value = sessions.value[0]?.id || ''
  }
  if (!sessions.value.length) {
    createSession()
    return
  }
  persistSessions()
  persistPanelState()
}

function clearCurrentSession() {
  const session = ensureSession()
  session.messages = []
  session.title = '新会话'
  session.taskAnalysis = null
  session.runtimePlan = null
  session.artifacts = []
  session.toolEvents = []
  session.previousResponseId = null
  session.pendingPlan = null
  session.pendingPlanToolOutputs = []
  session.lastPlan = null
  session.lastExecutionMemory = null
  session.sessionMemory = null
  session.lastSyncedMessageCount = 0
  session.updatedAt = Date.now()
  sessions.value = [...sessions.value]
  persistSessions()
}

function updateSessionMessage(
  sessionId: string,
  messageId: string,
  patch: Partial<Pick<AgentMessage, 'content' | 'reasoning'>>,
  options: { persist?: boolean } = {},
) {
  const session = sessions.value.find((item) => item.id === sessionId)
  if (!session) return
  const message = session.messages.find((item) => item.id === messageId)
  if (!message) return
  if (typeof patch.content === 'string') {
    message.content = patch.content
  }
  if (typeof patch.reasoning === 'string') {
    message.reasoning = patch.reasoning
  }
  session.updatedAt = Date.now()
  if (options.persist !== false) {
    sessions.value = [...sessions.value]
    persistSessions()
  } else {
    triggerRef(sessions)
  }
  scrollMessagesToBottom()
}

function isStreamingAssistantMessage(message: AgentMessage) {
  return streaming.value && message.role === 'assistant' && message.id === streamingAssistantId.value
}

function displayedMessageContent(message: AgentMessage) {
  const content = isStreamingAssistantMessage(message) ? liveAssistantContent.value : message.content
  return stripProtocolContent(content)
}

function displayedMessageReasoning(message: AgentMessage) {
  return isStreamingAssistantMessage(message) ? liveAssistantReasoning.value : (message.reasoning || '')
}

function stripProtocolContent(content: string) {
  if (!content) return ''
  return content
    .replace(ACTION_BLOCK_REGEX, '\n')
    .replace(ROUTE_MARKER_REGEX, '\n')
    .replace(/[ \t]+\n/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

function syncSessionWithActiveProvider(session: AgentSession | null) {
  if (!session) return

  const provider = activeProvider.value
  const providerId = provider?.id || null
  const allowedModels = enabledModelsForProvider(provider)
  let changed = false

  if (session.providerId !== providerId) {
    session.providerId = providerId
    session.previousResponseId = null
    session.pendingPlan = null
    session.pendingPlanToolOutputs = []
    session.lastPlan = null
    session.lastExecutionMemory = null
    session.lastSyncedMessageCount = 0
    changed = true
  }

  if (!allowedModels.length) {
    if (session.model) {
      session.model = ''
      changed = true
    }
  } else if (!allowedModels.includes(session.model)) {
    session.model = allowedModels[0]
    changed = true
  }

  if (changed) {
    sessions.value = [...sessions.value]
    persistSessions()
  }
}

function formatSessionTime(timestamp: number) {
  const date = new Date(timestamp)
  const MM = `${date.getMonth() + 1}`.padStart(2, '0')
  const dd = `${date.getDate()}`.padStart(2, '0')
  const hh = `${date.getHours()}`.padStart(2, '0')
  const mm = `${date.getMinutes()}`.padStart(2, '0')
  return `${MM}/${dd} ${hh}:${mm}`
}

function logAgentPanelError(scope: string, error: unknown, extra?: Record<string, unknown>) {
  console.error(`agent panel error: ${scope}`, {
    error,
    ...extra,
  })
}

function agentDebugEnabled() {
  try {
    return localStorage.getItem(AGENT_DEBUG_STORAGE_KEY) !== '0'
  } catch {
    return true
  }
}

function logAgentDebugGroup(title: string, payload: Record<string, unknown>) {
  if (!agentDebugEnabled()) return
  console.groupCollapsed(`[agent-debug] ${title}`)
  for (const [key, value] of Object.entries(payload)) {
    console.debug(key, value)
  }
  console.groupEnd()
}

function logAgentModelIo(title: string, payload: Record<string, unknown>) {
  if (!agentDebugEnabled()) return
  console.group(`[agent-debug] ${title}`)
  const preferredOrder = [
    'provider',
    'model',
    'transport_mode',
    'response_id',
    'error',
    'system_prompt',
    'preamble',
    'history',
    'prompt',
    'tools',
    'additional_params',
    'text',
    'partial_text',
    'tool_calls',
  ]
  const seen = new Set<string>()
  for (const key of preferredOrder) {
    if (key in payload) {
      console.log(key, payload[key])
      seen.add(key)
    }
  }
  for (const [key, value] of Object.entries(payload)) {
    if (seen.has(key)) continue
    console.log(key, value)
  }
  console.groupEnd()
}

function buildDebugRuntimePlanSnapshot(runtimePlan: AgentRuntimePlan | null) {
  if (!runtimePlan) return null
  return {
    id: runtimePlan.id,
    status: runtimePlan.status,
    goal: runtimePlan.goal,
    steps: runtimePlan.steps.map((step, index) => ({
      index: index + 1,
      id: step.id,
      title: step.title,
      status: step.status,
      requiresConfirmation: step.requiresConfirmation,
      requiresDocumentWrite: step.requiresDocumentWrite,
      requiresDocumentSave: step.requiresDocumentSave,
      toolHints: step.toolHints,
    })),
  }
}

function buildDebugExecutionSnapshot(
  session: AgentSession,
  executionState: AgentExecutionState,
  control: AgentControlBlock | null = null,
) {
  return {
    confirmationRequired: executionState.confirmationRequired,
    pendingPlan: session.pendingPlan,
    pendingPlanToolOutputs: session.pendingPlanToolOutputs.map((output) => ({
      call_id: output.call_id,
      name: output.name || null,
      arguments: output.arguments || null,
      output: output.output,
    })),
    lastPlan: session.lastPlan,
    currentPlanNeedsConfirmation: Boolean(session.pendingPlan?.trim()),
    semanticContinuation: executionState.semanticContinuation,
    semanticContinuationRound: executionState.semanticContinuationRound,
    planStepIndex: executionState.planStepIndex,
    planTotalSteps: executionState.planTotalSteps,
    planCurrentStep: executionState.planCurrentStep,
    planCompletedSteps: [...executionState.planCompletedSteps],
    writeCompleted: executionState.writeCompleted,
    saveRequested: executionState.saveRequested,
    documentWriteObserved: executionState.documentWriteObserved,
    saveAttemptWithoutDocumentChange: executionState.saveAttemptWithoutDocumentChange,
    lastInterceptCode: executionState.lastInterceptCode,
    lastInterceptMessage: executionState.lastInterceptMessage,
    control: control ? {
      phase: control.phase || null,
      pendingPlan: control.pendingPlan === true,
      autoContinue: control.autoContinue === true,
      needsSave: control.needsSave === true,
      writeScope: control.writeScope || null,
      preferredWriteAction: control.preferredWriteAction || null,
      taskKind: control.taskKind || null,
      editIntent: control.editIntent || null,
      editStage: control.editStage || null,
      saveRequested: control.saveRequested === true,
      writeCompleted: control.writeCompleted === true,
      planStepIndex: control.planStepIndex ?? null,
      planTotalSteps: control.planTotalSteps ?? null,
      planCurrentStep: control.planCurrentStep || null,
      planCompletedSteps: control.planCompletedSteps || [],
    } : null,
    runtimePlan: buildDebugRuntimePlanSnapshot(session.runtimePlan),
    lastExecutionMemory: session.lastExecutionMemory ? {
      confirmationRequired: session.lastExecutionMemory.confirmationRequired,
      plan: session.lastExecutionMemory.plan,
      assistantSummary: session.lastExecutionMemory.assistantSummary,
      controlPhase: session.lastExecutionMemory.controlPhase,
      planStepIndex: session.lastExecutionMemory.planStepIndex,
      planTotalSteps: session.lastExecutionMemory.planTotalSteps,
      planCurrentStep: session.lastExecutionMemory.planCurrentStep,
      planCompletedSteps: [...session.lastExecutionMemory.planCompletedSteps],
      writeCompleted: session.lastExecutionMemory.writeCompleted,
      saveRequested: session.lastExecutionMemory.saveRequested,
      documentWriteObserved: session.lastExecutionMemory.documentWriteObserved,
      saveAttemptWithoutDocumentChange: session.lastExecutionMemory.saveAttemptWithoutDocumentChange,
      lastInterceptCode: session.lastExecutionMemory.lastInterceptCode,
    } : null,
  }
}

function summarizeAgentEventForDebug(event: string, data: any) {
  if (event === 'agent.debug.model_request') {
    return {
      event,
      model: data?.model || null,
      transportMode: data?.transport_mode || null,
      protocol: data?.provider?.protocol || null,
      historyCount: Array.isArray(data?.history) ? data.history.length : 0,
      toolCount: Array.isArray(data?.tools) ? data.tools.length : 0,
      preamblePreview: compactMessageText(typeof data?.preamble === 'string' ? data.preamble : '', 220) || '',
      promptPreview: compactMessageText(JSON.stringify(data?.prompt || {}), 220) || '',
    }
  }

  if (event === 'agent.debug.model_response') {
    return {
      event,
      model: data?.model || null,
      transportMode: data?.transport_mode || null,
      protocol: data?.protocol || null,
      responseId: data?.response_id || null,
      toolCallCount: Array.isArray(data?.tool_calls) ? data.tool_calls.length : 0,
      textPreview: compactMessageText(typeof data?.text === 'string' ? data.text : '', 220) || '',
    }
  }

  if (event === 'agent.debug.model_error') {
    return {
      event,
      model: data?.model || null,
      transportMode: data?.transport_mode || null,
      protocol: data?.protocol || null,
      error: data?.error || null,
      partialTextPreview: compactMessageText(typeof data?.partial_text === 'string' ? data.partial_text : '', 220) || '',
      toolCallCount: Array.isArray(data?.tool_calls) ? data.tool_calls.length : 0,
    }
  }

  if (event === 'message.delta') {
    const content = typeof data?.content === 'string' ? data.content : ''
    return {
      event,
      contentLength: content.length,
      contentPreview: compactMessageText(content, 120) || '',
    }
  }

  if (event === 'reasoning.delta') {
    const delta = typeof data?.delta === 'string'
      ? data.delta
      : typeof data?.content === 'string'
        ? data.content
        : ''
    return {
      event,
      contentLength: delta.length,
      contentPreview: compactMessageText(delta, 120) || '',
    }
  }

  if (event === 'tool.calls.required') {
    return {
      event,
      responseId: data?.response_id || null,
      calls: Array.isArray(data?.calls)
        ? data.calls.map((call: any) => ({
          callId: call?.call_id || null,
          name: call?.name || null,
          arguments: call?.arguments || null,
        }))
        : [],
      contentPreview: compactMessageText(typeof data?.content === 'string' ? data.content : '', 220) || '',
    }
  }

  if (event === 'message.completed') {
    return {
      event,
      responseId: data?.response_id || null,
      contentPreview: compactMessageText(typeof data?.content === 'string' ? data.content : '', 220) || '',
    }
  }

  return {
    event,
    data,
  }
}

function isAgentInternalInterceptError(message: string) {
  const normalized = message.trim()
  if (!normalized) return false
  return [
    '当前步骤要求执行正文修改，但本轮没有输出任何有效的正文协议写入。',
    '模型在正文动作未完整闭合时请求了后续工具',
    '计划执行停滞：连续多轮只读取或保存当前文档，但没有真正写入正文。',
    '计划执行空转：连续多轮没有新的工具结果、正文写入或步骤推进。',
    '检测到重复工具调用循环，已停止本次生成，请调整指令后重试',
    '工具调用轮次过多，已停止本次生成，请补充更明确的目标或范围',
  ].some((prefix) => normalized.startsWith(prefix))
}

function resolveAgentInterceptDetails(message: string) {
  const normalized = message.trim()
  if (!normalized) return null

  if (normalized.startsWith('当前步骤要求执行正文修改，但本轮没有输出任何有效的正文协议写入。')) {
    return {
      code: 'document_write_protocol_missing',
      userMessage: '本轮执行已被拦截：当前步骤要求真正写正文，但模型既没有调用局部编辑工具，也没有输出有效的 ACTION 正文协议。下一轮它需要先完成正文修改，再继续后续动作。',
      modelGuidance: 'The current step requires a document write. Use partial-edit tools for scoped edits. For streamed writes, use ACTION:append for empty-document drafting and end-of-document continuation, and use ACTION:replace only for true full-document replacement.',
    }
  }

  if (normalized.startsWith('模型在正文动作未完整闭合时请求了后续工具')) {
    return {
      code: 'action_block_not_closed',
      userMessage: '本轮执行已被拦截：模型在正文写入动作还没完整闭合时就请求了后续工具。下一轮需要先把 ACTION 写完整，再继续。',
      modelGuidance: 'Finish and close the current ACTION write block before requesting any follow-up tools or claiming completion.',
    }
  }

  if (normalized.startsWith('计划执行停滞：连续多轮只读取或保存当前文档，但没有真正写入正文。')) {
    return {
      code: 'plan_stalled_without_write',
      userMessage: '本轮执行已被拦截：模型连续多轮只做读取或保存，没有真正写入正文。下一轮需要重新规划当前步骤，并明确产出正文修改。',
      modelGuidance: 'The current execution is stalled. Re-plan the current step and produce an actual document write instead of repeating read-only or save-only rounds.',
    }
  }

  if (normalized.startsWith('计划执行空转：连续多轮没有新的工具结果、正文写入或步骤推进。')) {
    return {
      code: 'plan_idle_without_progress',
      userMessage: '本轮执行已被拦截：模型连续多轮没有新的工具结果、正文写入或步骤推进，已经进入空转。下一轮需要重规划当前步骤，基于已有结果直接推进，不要继续重复无进展回复。',
      modelGuidance: 'The execution entered an idle loop with no new tool results, document writes, or step advancement. Re-plan the current step from existing state and move execution forward instead of repeating empty continuation rounds.',
    }
  }

  if (normalized.startsWith('检测到重复工具调用循环，已停止本次生成，请调整指令后重试')) {
    return {
      code: 'tool_loop_detected',
      userMessage: '本轮执行已被拦截：模型进入了重复工具调用循环。下一轮需要改变策略，不要再重复同一批工具调用。',
      modelGuidance: 'A repeated tool loop was detected. Change strategy, use prior tool results, and do not repeat the same tool batch unchanged.',
    }
  }

  if (normalized.startsWith('工具调用轮次过多，已停止本次生成，请补充更明确的目标或范围')) {
    return {
      code: 'tool_round_limit_reached',
      userMessage: '本轮执行已被拦截：工具调用轮次过多。下一轮需要收缩目标范围，基于已有结果直接推进，不要继续无界扩张步骤。',
      modelGuidance: 'The tool-call round limit was reached. Narrow the scope, reuse existing results, and continue with fewer steps instead of expanding the plan.',
    }
  }

  return null
}

function startDrag(event: MouseEvent) {
  dragging = true
  didDrag = false
  dragOffsetX = event.clientX - panelX.value
  dragOffsetY = event.clientY - panelY.value
  window.addEventListener('mousemove', onDrag)
  window.addEventListener('mouseup', stopDrag)
}

function onDrag(event: MouseEvent) {
  if (!dragging) return
  didDrag = true
  const nextX = event.clientX - dragOffsetX
  const nextY = event.clientY - dragOffsetY
  const clamped = clampPanelPosition(nextX, nextY, collapsed.value)
  panelX.value = clamped.x
  panelY.value = clamped.y
  if (collapsed.value) {
    collapsedPanelX.value = clamped.x
    collapsedPanelY.value = clamped.y
  } else {
    expandedPanelX.value = clamped.x
    expandedPanelY.value = clamped.y
  }
}

function stopDrag() {
  dragging = false
  window.removeEventListener('mousemove', onDrag)
  window.removeEventListener('mouseup', stopDrag)
  persistPanelState()
}

function handleFabClick() {
  if (didDrag) {
    didDrag = false
    return
  }
  toggleCollapse(false)
}

async function editProvider(providerId: string) {
  const provider = providers.value.find((item) => item.id === providerId)
  if (!provider) return
  try {
    const detail = await request.get(`/agent/providers/${providerId}`) as ProviderDetailResponse
    providerDraft.value = {
      id: `${detail.id}`,
      name: detail.name || provider.name,
      providerKind: normalizeProviderKind(detail.provider_kind || provider.providerKind),
      baseUrl: normalizedProviderBaseUrl(detail.base_url || provider.baseUrl),
      apiKey: detail.api_key || '',
    }
  } catch (error: any) {
    logAgentPanelError('edit_provider', error, { providerId })
    providerDraft.value = {
      id: provider.id,
      name: provider.name,
      providerKind: provider.providerKind,
      baseUrl: provider.baseUrl,
      apiKey: '',
    }
    ElMessage.error(error.response?.data?.error || error.message || '读取供应商详情失败')
  }
}

function startCreateProvider() {
  providerDraft.value = createProviderDraft()
}

async function saveProviderDraft() {
  const name = providerDraft.value.name.trim()
  const apiKey = providerDraft.value.apiKey.trim()
  const editingProvider = providerDraft.value.id
    ? providers.value.find((item) => item.id === providerDraft.value.id)
    : null

  if (!name) {
    ElMessage.warning('请填写供应商名称')
    return
  }
  if (!apiKey && !editingProvider?.hasApiKey) {
    ElMessage.warning('请填写 API Key')
    return
  }

  try {
    const data = await request.post('/agent/providers', {
      id: providerDraft.value.id ? Number(providerDraft.value.id) : null,
      name,
      provider_kind: providerDraft.value.providerKind,
      base_url: normalizedProviderBaseUrl(providerDraft.value.baseUrl),
      api_key: apiKey,
      remote_models: editingProvider?.remoteModels || [],
      enabled_models: editingProvider?.enabledModels || [],
      custom_models: editingProvider?.customModels || [],
      model_configs: editingProvider?.modelConfigs || {},
    }) as ProviderListResponse

    providers.value = (data.providers || [])
      .map((provider) => normalizeProvider(provider))
      .filter((provider): provider is AgentProvider => Boolean(provider))
    activeProviderId.value = `${data.active_provider_id ?? providers.value[0]?.id ?? ''}`
    ensureProviderDraftLoaded()
    syncSessionWithActiveProvider(currentSession.value)
    ElMessage.success('供应商配置已保存')
  } catch (error: any) {
    logAgentPanelError('save_provider', error, {
      providerId: providerDraft.value.id,
      providerName: name,
    })
    ElMessage.error(error.response?.data?.error || error.message || '保存供应商失败')
  }
}

async function activateProvider(providerId: string | null) {
  if (!providerId) return
  if (!providers.value.some((provider) => provider.id === providerId)) return
  try {
    const data = await request.post(`/agent/providers/${providerId}/activate`) as ProviderListResponse
    providers.value = (data.providers || [])
      .map((provider) => normalizeProvider(provider))
      .filter((provider): provider is AgentProvider => Boolean(provider))
    activeProviderId.value = `${data.active_provider_id ?? providerId}`
    syncSessionWithActiveProvider(currentSession.value)
    ElMessage.success('已切换激活供应商')
  } catch (error: any) {
    logAgentPanelError('activate_provider', error, { providerId })
    ElMessage.error(error.response?.data?.error || error.message || '切换供应商失败')
  }
}

async function removeProvider(providerId: string | null) {
  if (!providerId) return
  try {
    const data = await request.delete(`/agent/providers/${providerId}`) as ProviderListResponse
    providers.value = (data.providers || [])
      .map((provider) => normalizeProvider(provider))
      .filter((provider): provider is AgentProvider => Boolean(provider))
    activeProviderId.value = `${data.active_provider_id ?? providers.value[0]?.id ?? ''}`
    ensureProviderDraftLoaded()
    if (!providers.value.length) {
      providerDraft.value = createProviderDraft()
      showModelDialog.value = false
    }
    syncSessionWithActiveProvider(currentSession.value)
    ElMessage.success('供应商已删除')
  } catch (error: any) {
    logAgentPanelError('remove_provider', error, { providerId })
    ElMessage.error(error.response?.data?.error || error.message || '删除供应商失败')
  }
}

async function fetchProviderModels() {
  const provider = activeProvider.value
  if (!provider) {
    ElMessage.warning('请先激活一个供应商')
    return
  }
  if (!provider.hasApiKey) {
    ElMessage.warning('当前供应商缺少 API Key')
    return
  }

  modelLoading.value = true
  try {
    const data = await request.post('/agent/models', {
      provider_id: Number(provider.id),
    }) as { models?: ModelApiItem[] }

    const ids = uniqueStrings((data.models || []).map((item) => item.id))
    modelDraft.value.remoteModels = ids
    ElMessage.success(`已同步 ${ids.length} 个模型`)
  } catch (error: any) {
    logAgentPanelError('fetch_provider_models', error, { providerId: provider.id })
    ElMessage.error(error.response?.data?.error || error.message || '获取模型列表失败')
  } finally {
    modelLoading.value = false
  }
}

function addCustomModel() {
  const name = customModelInput.value.trim()
  if (!name) return

  modelDraft.value.customModels = uniqueStrings([...modelDraft.value.customModels, name])
  modelDraft.value.enabledModels = uniqueStrings([...modelDraft.value.enabledModels, name])
  customModelInput.value = ''
}

function removeCustomModel(model: string) {
  modelDraft.value.customModels = modelDraft.value.customModels.filter((item) => item !== model)
  modelDraft.value.enabledModels = modelDraft.value.enabledModels.filter((item) => item !== model)
}

function toggleCustomModel(model: string, checked: string | number | boolean) {
  if (checked) {
    modelDraft.value.enabledModels = uniqueStrings([...modelDraft.value.enabledModels, model])
  } else {
    modelDraft.value.enabledModels = modelDraft.value.enabledModels.filter((item) => item !== model)
  }
}

async function saveModelDraft() {
  const provider = activeProvider.value
  if (!provider) {
    ElMessage.warning('请先激活一个供应商')
    return
  }

  provider.remoteModels = uniqueStrings(modelDraft.value.remoteModels)
  provider.customModels = uniqueStrings(modelDraft.value.customModels)
  provider.enabledModels = uniqueStrings([
    ...modelDraft.value.enabledModels.filter((model) =>
      provider.remoteModels.includes(model) || provider.customModels.includes(model),
    ),
  ])
  provider.updatedAt = Date.now()

  try {
    await saveProviderConfig(provider)
    syncSessionWithActiveProvider(currentSession.value)
    showModelDialog.value = false
    ElMessage.success('模型配置已保存')
  } catch (error: any) {
    logAgentPanelError('save_model_draft', error, { providerId: provider.id })
    ElMessage.error(error.response?.data?.error || error.message || '保存模型配置失败')
  }
}

function openModelConfigDialog(model: string) {
  const provider = activeProvider.value
  if (!provider) return
  editingModelConfigName.value = model
  modelConfigDraft.value = createModelConfigDraft(provider.modelConfigs[model] || null)
  showModelConfigDialog.value = true
}

function applyRecommendedModelConfig() {
  const provider = activeProvider.value
  if (!provider) return
  modelConfigDraft.value = createModelConfigDraft(recommendedModelConfig(provider.providerKind))
}

function resetModelConfigDraftToCurrent() {
  const provider = activeProvider.value
  const model = editingModelConfigName.value.trim()
  if (!provider || !model) return
  modelConfigDraft.value = createModelConfigDraft(provider.modelConfigs[model] || null)
}

function removeCurrentModelConfig() {
  const provider = activeProvider.value
  const model = editingModelConfigName.value.trim()
  if (!provider || !model) return
  const nextConfigs = { ...provider.modelConfigs }
  delete nextConfigs[model]
  provider.modelConfigs = nextConfigs
  providers.value = [...providers.value]
  modelConfigDraft.value = createModelConfigDraft()
  ElMessage.success(`已清除模型 ${model} 的参数配置，请回到“模型管理”点击“保存”完成持久化`)
}

function saveCurrentModelConfig() {
  const provider = activeProvider.value
  const model = editingModelConfigName.value.trim()
  if (!provider || !model) return

  let additionalParams: Record<string, unknown> | null = null
  const additionalParamsText = modelConfigDraft.value.additionalParamsText.trim()
  if (additionalParamsText) {
    try {
      const parsed = JSON.parse(additionalParamsText)
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
        ElMessage.warning('高级附加参数必须是 JSON 对象')
        return
      }
      additionalParams = parsed as Record<string, unknown>
    } catch {
      ElMessage.warning('高级附加参数不是合法 JSON')
      return
    }
  }

  const config = normalizeModelConfig({
    modalities: modelConfigDraft.value.modalities,
    thinking: parseBooleanChoice(modelConfigDraft.value.thinking),
    tools_enabled: parseBooleanChoice(modelConfigDraft.value.tools_enabled),
    temperature: parseNumberText(modelConfigDraft.value.temperatureText),
    max_output_tokens: parseIntegerText(modelConfigDraft.value.maxTokensText),
    top_p: parseNumberText(modelConfigDraft.value.topPText),
    top_k: parseIntegerText(modelConfigDraft.value.topKText),
    presence_penalty: parseNumberText(modelConfigDraft.value.presencePenaltyText),
    frequency_penalty: parseNumberText(modelConfigDraft.value.frequencyPenaltyText),
    parallel_tool_calls: parseBooleanChoice(modelConfigDraft.value.parallelToolCalls),
    reasoning_effort: modelConfigDraft.value.reasoningEffort.trim() || null,
    stop_sequences: modelConfigDraft.value.stopSequencesText.split('\n'),
    response_mime_type: modelConfigDraft.value.responseMimeType.trim() || null,
    additional_params: additionalParams,
  })

  const nextConfigs = { ...provider.modelConfigs }
  if (modelConfigIsConfigured(config)) {
    nextConfigs[model] = config
  } else {
    delete nextConfigs[model]
  }
  provider.modelConfigs = nextConfigs
  providers.value = [...providers.value]
  showModelConfigDialog.value = false
  ElMessage.success(`已更新模型 ${model} 的参数配置，请回到“模型管理”点击“保存”完成持久化`)
}

function openProviderManagerFromModelDialog() {
  showModelDialog.value = false
  openProviderDialog()
}

function openProviderDialog() {
  showProviderDialog.value = true
}

function openMcpDialog() {
  showMcpDialog.value = true
}

function openAttachmentPicker() {
  if (!composerAttachmentEnabled.value || streaming.value || attachmentUploading.value) return
  attachmentInputRef.value?.click()
}

async function uploadComposerAttachment(file: File) {
  if (file.size > systemStore.uploadMaxBytes) {
    ElMessage.warning(`文件 ${file.name} 超过 ${systemStore.uploadLimitLabel} 限制`)
    return
  }

  const allowImage = composerSupportsImage.value
  const allowDocument = composerSupportsDocument.value
  const isImage = isSupportedImageFile(file)
  const knownDocument = isSupportedDocumentFile(file)
  const inferredTextDocument = !isImage && !knownDocument && allowDocument
    ? await isProbablyUtf8TextFile(file)
    : false
  const isDocument = knownDocument || inferredTextDocument

  if (isImage && !allowImage) {
    ElMessage.warning(`当前模型没有开启图片输入，无法添加 ${file.name}`)
    return
  }
  if (!isImage && !isDocument) {
    ElMessage.warning(`文件 ${file.name} 暂不支持作为聊天附件`)
    return
  }
  if (!isImage && isDocument && !allowDocument) {
    ElMessage.warning(`当前模型没有开启文件输入，无法添加 ${file.name}`)
    return
  }

  const kind = isImage ? 'doc-image' : 'doc-file'
  const attachmentKind = isImage ? 'image' : 'document'
  const formData = new FormData()
  formData.append('kind', kind)
  formData.append('file', file)

  attachmentUploadingCount.value += 1
  try {
    const data = (await request.post('/uploads', formData)) as {
      upload?: {
        id: number
        url: string
        original_name: string
        content_type?: string | null
        size?: number
      }
    }
    const upload = data?.upload
    if (!upload?.id) {
      throw new Error('上传结果缺少附件 ID')
    }
    composerAttachments.value = [
      ...composerAttachments.value,
      {
        localId: genId(),
        uploadId: Number(upload.id),
        kind: attachmentKind,
        name: upload.original_name || file.name,
        url: upload.url || '',
        contentType: upload.content_type || file.type || null,
        size: Number(upload.size) || file.size || 0,
      },
    ]
  } finally {
    attachmentUploadingCount.value = Math.max(0, attachmentUploadingCount.value - 1)
    if (attachmentInputRef.value) {
      attachmentInputRef.value.value = ''
    }
  }
}

async function addComposerFiles(fileList: FileList | File[]) {
  const files = Array.from(fileList || [])
  if (!files.length) return

  for (const file of files) {
    try {
      await uploadComposerAttachment(file)
    } catch (error: any) {
      ElMessage.error(error.response?.data?.error || error.message || `上传 ${file.name} 失败`)
    }
  }
}

async function handleAttachmentInputChange(event: Event) {
  const target = event.target as HTMLInputElement | null
  if (!target?.files?.length) return
  await addComposerFiles(target.files)
}

async function handleComposerPaste(event: ClipboardEvent) {
  if (!composerAttachmentEnabled.value || streaming.value) return
  const files = Array.from(event.clipboardData?.files || [])
  if (!files.length) return
  await addComposerFiles(files)
}

function currentDocumentHasUnsavedChanges() {
  if (!props.docId) return false
  const liveBridge = getAgentEditorBridge()
  if (liveBridge?.docId === props.docId) {
    return liveBridge.getValue() !== (props.docContent ?? '')
  }
  const snapshot = getAgentEditorSnapshot(props.docId)
  if (snapshot !== null) {
    return snapshot !== (props.docContent ?? '')
  }
  if (hasDocDraft(props.docId)) return true
  return false
}

function currentEditorAvailable() {
  const liveBridge = getAgentEditorBridge()
  return Boolean(liveBridge && (!props.docId || liveBridge.docId === props.docId))
}

function currentEditorSnapshotSource() {
  const liveBridge = getAgentEditorBridge()
  return resolveAgentEditorSnapshotSource({
    hasLiveEditor: Boolean(liveBridge?.docId === props.docId),
    hasAgentSnapshot: Boolean(props.docId && getAgentEditorSnapshot(props.docId) !== null),
    hasDraftCache: Boolean(props.docId && hasDocDraft(props.docId)),
  })
}

function compactMessageText(content: string, maxChars = REQUEST_SUMMARY_ITEM_CHARS) {
  const compact = content.replace(/\s+/g, ' ').trim()
  if (!compact) return ''
  if (compact.length <= maxChars) return compact
  return `${compact.slice(0, maxChars)}...`
}

function summarizeMessageAttachments(attachments: AgentAttachment[]) {
  if (!attachments.length) return ''
  const imageCount = attachments.filter((attachment) => attachment.kind === 'image').length
  const documentCount = attachments.length - imageCount
  const parts: string[] = []
  if (imageCount) parts.push(`${imageCount} 张图片`)
  if (documentCount) parts.push(`${documentCount} 个文件`)
  return parts.join('、')
}

function compactMessageSummary(message: AgentMessage, maxChars = REQUEST_SUMMARY_ITEM_CHARS) {
  const compact = compactMessageText(message.content, maxChars)
  const attachmentSummary = summarizeMessageAttachments(message.attachments || [])
  if (compact && attachmentSummary) {
    return compactMessageText(`${compact}（附件：${attachmentSummary}）`, maxChars)
  }
  if (compact) return compact
  if (attachmentSummary) return `附件：${attachmentSummary}`
  return ''
}

function compactJsonLike(value: unknown, maxChars = REQUEST_SUMMARY_ITEM_CHARS) {
  if (value === null || value === undefined) return ''
  const raw = typeof value === 'string' ? value : JSON.stringify(value)
  return compactMessageText(raw || '', maxChars)
}

function isInternalExecutionLedgerMessage(content: string) {
  const normalized = content.trim()
  if (!normalized) return false
  return [
    '以下是本轮当前请求的执行账本更新。',
    '执行账本更新：',
    '执行账本提示：',
    '执行账本纠偏：',
  ].some((prefix) => normalized.startsWith(prefix))
}

function summarizeToolCallBatch(calls: AgentToolCall[], outputs: AgentToolOutputPayload[]) {
  if (!calls.length) return []

  const outputByCallId = new Map(outputs.map((output) => [output.call_id, output]))
  return calls.map((call) => {
    const output = outputByCallId.get(call.call_id)
    const payload = output?.output && typeof output.output === 'object'
      ? output.output as Record<string, any>
      : null
    const ok = typeof payload?.ok === 'boolean' ? payload.ok : null
    const result = payload?.result && typeof payload.result === 'object'
      ? payload.result as Record<string, any>
      : null
    let outcome: AgentExecutionToolCallSummary['outcome'] = ok === false ? 'error' : 'unknown'

    if (call.name === 'save_current_document') {
      const savePerformed = result?.saved === true
      const saveNoop =
        result?.save_action === 'noop'
        || result?.already_saved === true
        || result?.alreadySaved === true
        || result?.unsaved_changes_before_save === false
      if (savePerformed) outcome = 'success'
      else if (saveNoop) outcome = 'noop'
      else if (ok === false) outcome = 'error'
    } else if (ok === true) {
      outcome = 'success'
    }

    return {
      name: call.name,
      arguments: compactMessageText(call.arguments || '', 320) || null,
      output: output ? compactJsonLike(output.output, 360) || null : null,
      ok,
      outcome,
      stagePolicy: typeof call.stage_policy === 'string' ? call.stage_policy : null,
      capabilities: Array.isArray(call.capabilities)
        ? call.capabilities.filter((item): item is string => typeof item === 'string' && Boolean(item.trim()))
        : [],
    }
  })
}

function buildRoundToolCallSummaryFromToolEvent(raw: any): AgentExecutionToolCallSummary | null {
  if (!raw || typeof raw !== 'object') return null
  const status = typeof raw.status === 'string' && raw.status.trim() ? raw.status.trim() : ''
  if (!['completed', 'failed', 'noop'].includes(status)) return null
  const toolName = typeof raw.tool === 'string' && raw.tool.trim() ? raw.tool.trim() : ''
  if (!toolName) return null

  const payload = raw.output && typeof raw.output === 'object'
    ? raw.output as Record<string, any>
    : null
  const ok = typeof payload?.ok === 'boolean' ? payload.ok : null
  const result = payload?.result && typeof payload.result === 'object'
    ? payload.result as Record<string, any>
    : null
  const rawArguments = typeof raw.arguments === 'string'
    ? raw.arguments
    : raw.arguments && typeof raw.arguments === 'object'
      ? JSON.stringify(raw.arguments)
      : ''

  let outcome: AgentExecutionToolCallSummary['outcome'] = ok === false ? 'error' : 'unknown'
  if (toolName === 'save_current_document') {
    const savePerformed = result?.saved === true
    const saveNoop =
      result?.save_action === 'noop'
      || result?.already_saved === true
      || result?.alreadySaved === true
      || result?.unsaved_changes_before_save === false
    if (savePerformed) outcome = 'success'
    else if (saveNoop) outcome = 'noop'
    else if (ok === false) outcome = 'error'
  } else if (ok === true) {
    outcome = 'success'
  }

  return {
    name: toolName,
    arguments: compactMessageText(rawArguments || '', 320) || null,
    output: compactJsonLike(payload, 360) || null,
    ok,
    outcome,
    stagePolicy: 'mutation',
    capabilities: ['update'],
  }
}

function parseToolArguments(argumentsText: string | null) {
  if (!argumentsText) return null
  try {
    const parsed = JSON.parse(argumentsText)
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? parsed as Record<string, any>
      : null
  } catch {
    return null
  }
}

function coerceBooleanLike(value: unknown): boolean | null {
  if (typeof value === 'boolean') return value
  if (typeof value === 'string') {
    const normalized = value.trim().toLowerCase()
    if (normalized === 'true') return true
    if (normalized === 'false') return false
  }
  return null
}

function coerceNumberLike(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string' && value.trim()) {
    const parsed = Number(value.trim())
    if (Number.isFinite(parsed)) return parsed
  }
  return null
}

function parseJsonLikeValue(value: unknown) {
  if (typeof value !== 'string') return value
  const trimmed = value.trim()
  if (!trimmed) return value
  if (
    (trimmed.startsWith('{') && trimmed.endsWith('}'))
    || (trimmed.startsWith('[') && trimmed.endsWith(']'))
  ) {
    try {
      return JSON.parse(trimmed)
    } catch {
      return value
    }
  }
  return value
}

function coerceStringListLike(value: unknown): string[] {
  const normalized = parseJsonLikeValue(value)
  if (Array.isArray(normalized)) {
    return normalized
      .filter((item): item is string => typeof item === 'string' && Boolean(item.trim()))
      .map((item) => item.trim())
  }
  return []
}

function normalizeControlPayload(raw: any): AgentControlBlock | null {
  if (!raw || typeof raw !== 'object') return null
  const normalized = {
    current_mode: typeof raw.current_mode === 'string' && raw.current_mode.trim()
      ? raw.current_mode.trim()
      : typeof raw.currentMode === 'string' && raw.currentMode.trim()
        ? raw.currentMode.trim()
        : null,
    awaiting: typeof raw.awaiting === 'string' && raw.awaiting.trim() ? raw.awaiting.trim() : null,
    current_action_kind: typeof raw.current_action_kind === 'string' && raw.current_action_kind.trim()
      ? raw.current_action_kind.trim()
      : typeof raw.currentActionKind === 'string' && raw.currentActionKind.trim()
        ? raw.currentActionKind.trim()
        : null,
    current_action_status: typeof raw.current_action_status === 'string' && raw.current_action_status.trim()
      ? raw.current_action_status.trim()
      : typeof raw.currentActionStatus === 'string' && raw.currentActionStatus.trim()
        ? raw.currentActionStatus.trim()
        : null,
    current_action_mode: typeof raw.current_action_mode === 'string' && raw.current_action_mode.trim()
      ? raw.current_action_mode.trim()
      : typeof raw.currentActionMode === 'string' && raw.currentActionMode.trim()
        ? raw.currentActionMode.trim()
        : null,
    current_action_target: typeof raw.current_action_target === 'string' && raw.current_action_target.trim()
      ? raw.current_action_target.trim()
      : typeof raw.currentActionTarget === 'string' && raw.currentActionTarget.trim()
        ? raw.currentActionTarget.trim()
        : null,
    confirmation_required: coerceBooleanLike(raw.confirmation_required ?? raw.confirmationRequired),
    phase: typeof raw.phase === 'string' && raw.phase.trim() ? raw.phase.trim() : null,
    pending_plan: coerceBooleanLike(raw.pending_plan ?? raw.pendingPlan),
    auto_continue: coerceBooleanLike(raw.auto_continue ?? raw.autoContinue),
    needs_save: coerceBooleanLike(raw.needs_save ?? raw.needsSave),
    write_scope: typeof raw.write_scope === 'string' && raw.write_scope.trim()
      ? raw.write_scope.trim()
      : typeof raw.writeScope === 'string' && raw.writeScope.trim()
        ? raw.writeScope.trim()
        : null,
    preferred_write_action: typeof raw.preferred_write_action === 'string' && raw.preferred_write_action.trim()
      ? raw.preferred_write_action.trim()
      : typeof raw.preferredWriteAction === 'string' && raw.preferredWriteAction.trim()
        ? raw.preferredWriteAction.trim()
        : null,
    task_kind: typeof raw.task_kind === 'string' && raw.task_kind.trim()
      ? raw.task_kind.trim()
      : typeof raw.taskKind === 'string' && raw.taskKind.trim()
        ? raw.taskKind.trim()
        : null,
    edit_intent: typeof raw.edit_intent === 'string' && raw.edit_intent.trim()
      ? raw.edit_intent.trim()
      : typeof raw.editIntent === 'string' && raw.editIntent.trim()
        ? raw.editIntent.trim()
        : null,
    edit_stage: typeof raw.edit_stage === 'string' && raw.edit_stage.trim()
      ? raw.edit_stage.trim()
      : typeof raw.editStage === 'string' && raw.editStage.trim()
        ? raw.editStage.trim()
        : null,
    save_requested: coerceBooleanLike(raw.save_requested ?? raw.saveRequested),
    write_completed: coerceBooleanLike(raw.write_completed ?? raw.writeCompleted),
    plan_step_index: coerceNumberLike(raw.plan_step_index ?? raw.planStepIndex),
    plan_total_steps: coerceNumberLike(raw.plan_total_steps ?? raw.planTotalSteps),
    plan_current_step: typeof raw.plan_current_step === 'string' && raw.plan_current_step.trim()
      ? raw.plan_current_step.trim()
      : typeof raw.planCurrentStep === 'string' && raw.planCurrentStep.trim()
        ? raw.planCurrentStep.trim()
        : null,
    plan_completed_steps: coerceStringListLike(raw.plan_completed_steps ?? raw.planCompletedSteps),
  }
  return {
    currentMode: normalized.current_mode,
    awaiting: normalized.awaiting,
    currentActionKind: normalized.current_action_kind,
    currentActionStatus: normalized.current_action_status,
    currentActionMode: normalized.current_action_mode,
    currentActionTarget: normalized.current_action_target,
    confirmationRequired: normalized.confirmation_required === true,
    phase: normalized.phase,
    pendingPlan: normalized.pending_plan === true,
    autoContinue: normalized.auto_continue === true,
    needsSave: normalized.needs_save === true,
    writeScope: normalized.write_scope,
    preferredWriteAction: normalized.preferred_write_action,
    taskKind: normalized.task_kind,
    editIntent: normalized.edit_intent,
    editStage: normalized.edit_stage,
    saveRequested: normalized.save_requested === true,
    writeCompleted: normalized.write_completed === true,
    planStepIndex: normalized.plan_step_index,
    planTotalSteps: normalized.plan_total_steps,
    planCurrentStep: normalized.plan_current_step,
    planCompletedSteps: normalized.plan_completed_steps,
  }
}

function buildControlFromExecutionState(
  executionState: AgentExecutionState,
  taskAnalysis: AgentTaskAnalysis | null,
): AgentControlBlock {
  return {
    currentMode: executionState.currentMode,
    awaiting: executionState.awaiting,
    currentActionKind: executionState.currentActionKind,
    currentActionStatus: executionState.currentActionStatus,
    currentActionMode: executionState.currentActionMode,
    currentActionTarget: executionState.currentActionTarget,
    confirmationRequired: executionState.confirmationRequired,
    phase: null,
    pendingPlan: Boolean(executionState.confirmationRequired && executionState.pendingPlan),
    autoContinue: executionState.semanticContinuation,
    needsSave: executionState.saveRequested,
    writeScope: taskAnalysis?.writeScope || null,
    preferredWriteAction: taskAnalysis?.preferredWriteAction || null,
    taskKind: executionState.taskKind,
    editIntent: executionState.editIntent,
    editStage: executionState.editStage,
    saveRequested: executionState.saveRequested,
    writeCompleted: executionState.writeCompleted,
    planStepIndex: executionState.planStepIndex,
    planTotalSteps: executionState.planTotalSteps,
    planCurrentStep: executionState.planCurrentStep,
    planCompletedSteps: [...executionState.planCompletedSteps],
  }
}

function isHostStateTool(name: string) {
  return (
    name === 'plan_start'
    || name === 'plan_step_update'
    || name === 'plan_complete'
    || name === 'plan_cancel'
    || name === 'write_start'
    || name === 'write_end'
  )
}

function summarizeRoundActions(roundToolCalls: AgentExecutionToolCallSummary[]) {
  if (!roundToolCalls.length) return []

  const createdDirs: string[] = []
  const createdDocs: string[] = []
  const writtenDocs: string[] = []
  let genericWriteCount = 0
  let actionWriteFailed = false
  let createdProject: string | null = null
  const otherActions: string[] = []

  for (const call of roundToolCalls) {
    const args = parseToolArguments(call.arguments)
    const outputPayload = parseToolArguments(call.output)
    const outputResult = outputPayload?.result && typeof outputPayload.result === 'object'
      ? outputPayload.result as Record<string, any>
      : null
    switch (call.name) {
      case 'create_project': {
        const name = typeof args?.name === 'string' && args.name.trim() ? args.name.trim() : ''
        createdProject = name || '新项目'
        break
      }
      case 'create_tree_node': {
        const name = typeof args?.name === 'string' && args.name.trim() ? args.name.trim() : ''
        const nodeType = typeof args?.node_type === 'string' ? args.node_type.trim() : ''
        if (name) {
          if (nodeType === 'dir') createdDirs.push(name)
          else if (nodeType === 'doc') createdDocs.push(name)
        }
        break
      }
      case 'open_tree_node': {
        break
      }
      case 'action_protocol_write': {
        const docName = typeof outputResult?.doc_name === 'string' && outputResult.doc_name.trim()
          ? outputResult.doc_name.trim()
          : ''
        if (call.outcome === 'success') {
          if (docName) {
            writtenDocs.push(docName)
          } else {
            genericWriteCount += 1
          }
        } else if (call.outcome === 'error') {
          actionWriteFailed = true
        }
        break
      }
      case 'read_document':
      case 'read_editor_snapshot':
      case 'get_project_tree':
      case 'list_projects':
      case 'get_current_page_state':
        break
      default:
        otherActions.push(call.name)
        break
    }
  }

  const parts: string[] = []
  if (createdProject) {
    parts.push(`已创建项目《${createdProject}》。`)
  }
  if (createdDirs.length) {
    parts.push(
      createdDirs.length <= 3
        ? `已创建目录：${createdDirs.map((name) => `《${name}》`).join('、')}。`
        : `已创建 ${createdDirs.length} 个目录。`,
    )
  }
  if (createdDocs.length) {
    parts.push(
      createdDocs.length <= 3
        ? `已创建文档：${createdDocs.map((name) => `《${name}》`).join('、')}。`
        : `已创建 ${createdDocs.length} 篇文档。`,
    )
  }
  const uniqueWrittenDocs = [...new Set(writtenDocs)]
  if (uniqueWrittenDocs.length) {
    parts.push(
      uniqueWrittenDocs.length <= 4
        ? `已写入正文：${uniqueWrittenDocs.map((name) => `《${name}》`).join('、')}。`
        : `已写入 ${uniqueWrittenDocs.length} 篇文档正文。`,
    )
  } else if (genericWriteCount > 0) {
    parts.push(genericWriteCount === 1 ? '已完成正文写入。' : `已完成 ${genericWriteCount} 次正文写入。`)
  }
  if (actionWriteFailed) {
    parts.push('部分正文写入失败。')
  }
  return parts
}

function appendRecentToolCalls(
  existing: AgentExecutionToolCallSummary[],
  nextBatch: AgentExecutionToolCallSummary[],
) {
  if (!nextBatch.length) return existing
  return [...existing, ...nextBatch].slice(-8)
}

function buildAgentExecutionContext(state: AgentExecutionState) {
  return {
    current_mode: state.currentMode,
    awaiting: state.awaiting,
    current_action_kind: state.currentActionKind,
    current_action_status: state.currentActionStatus,
    current_action_mode: state.currentActionMode,
    current_action_target: state.currentActionTarget,
    confirmation_required: state.confirmationRequired,
    pending_plan: state.pendingPlan,
    pending_plan_user_reply: state.pendingPlanUserReply,
    plan_confirmation_decision: state.planConfirmationDecision,
    composite_write_then_save: state.compositeWriteThenSave,
    semantic_continuation: state.semanticContinuation,
    semantic_continuation_round: state.semanticContinuationRound,
    previous_assistant_summary: state.previousAssistantSummary,
    task_kind: state.taskKind,
    edit_intent: state.editIntent,
    edit_stage: state.editStage,
    save_requested: state.saveRequested,
    write_completed: state.writeCompleted,
    plan_step_index: state.planStepIndex,
    plan_total_steps: state.planTotalSteps,
    plan_current_step: state.planCurrentStep,
    plan_completed_steps: state.planCompletedSteps,
    document_write_observed: state.documentWriteObserved,
    save_attempt_without_document_change: state.saveAttemptWithoutDocumentChange,
    last_intercept_code: state.lastInterceptCode,
    last_intercept_message: state.lastInterceptMessage,
    last_intercept_guidance: state.lastInterceptGuidance,
    recent_tool_calls: state.recentToolCalls,
  }
}

function buildAgentExecutionMemory(memory: AgentExecutionMemory) {
  return {
    current_mode: memory.currentMode,
    awaiting: memory.awaiting,
    current_action_kind: memory.currentActionKind,
    current_action_status: memory.currentActionStatus,
    current_action_mode: memory.currentActionMode,
    current_action_target: memory.currentActionTarget,
    confirmation_required: memory.confirmationRequired,
    plan: memory.plan,
    assistant_summary: memory.assistantSummary,
    control_phase: memory.controlPhase,
    task_kind: memory.taskKind,
    edit_intent: memory.editIntent,
    edit_stage: memory.editStage,
    save_requested: memory.saveRequested,
    write_completed: memory.writeCompleted,
    plan_step_index: memory.planStepIndex,
    plan_total_steps: memory.planTotalSteps,
    plan_current_step: memory.planCurrentStep,
    plan_completed_steps: memory.planCompletedSteps,
    document_write_observed: memory.documentWriteObserved,
    save_attempt_without_document_change: memory.saveAttemptWithoutDocumentChange,
    last_intercept_code: memory.lastInterceptCode,
    last_intercept_message: memory.lastInterceptMessage,
    last_intercept_guidance: memory.lastInterceptGuidance,
    recent_tool_calls: memory.recentToolCalls,
  }
}

function buildAgentSessionMemory(memory: AgentSessionMemory) {
  return {
    summary: memory.summary,
    active_user_goals: memory.activeUserGoals,
    completed_facts: memory.completedFacts,
    open_loops: memory.openLoops,
    updated_at: memory.updatedAt,
  }
}

function extractSaveNoopState(outputs: AgentToolOutputPayload[]) {
  for (const output of outputs) {
    if (output.name !== 'save_current_document') continue
    const payload = output.output
    if (!payload || typeof payload !== 'object') continue
    const result = (payload as Record<string, any>).result
    if (!result || typeof result !== 'object') continue
    const saved = result.saved === true
    const saveAction = result.save_action
    const alreadySaved = result.already_saved === true || result.alreadySaved === true
    const unsavedChangesBeforeSave = result.unsaved_changes_before_save
    if (!saved && (saveAction === 'noop' || alreadySaved || unsavedChangesBeforeSave === false)) {
      return true
    }
  }
  return false
}

function isReadOrSaveOnlyBatch(roundToolCalls: AgentExecutionToolCallSummary[]) {
  if (!roundToolCalls.length) return false
  return roundToolCalls.every((call) =>
    toolHasOnlyCapabilities(call.name, ['read', 'save'], {
      stagePolicy: call.stagePolicy,
      capabilities: call.capabilities,
    }),
  )
}

function hasSuccessfulMutationToolCall(roundToolCalls: AgentExecutionToolCallSummary[]) {
  return roundToolCalls.some((call) => (
    call.outcome === 'success'
    && !isHostStateTool(call.name)
    && !toolHasOnlyCapabilities(call.name, ['read', 'save'], {
      stagePolicy: call.stagePolicy,
      capabilities: call.capabilities,
    })
  ))
}

function buildHistorySummary(messages: AgentMessage[]) {
  const userItems: string[] = []
  const assistantItems: string[] = []

  for (const message of messages) {
    const compact = compactMessageSummary(message)
    if (!compact) continue

    if (message.role === 'user') {
      if (userItems.length < REQUEST_SUMMARY_MAX_ITEMS) {
        userItems.push(`- ${compact}`)
      }
      continue
    }

    if (message.role === 'assistant') {
      if (assistantItems.length < REQUEST_SUMMARY_MAX_ITEMS) {
        assistantItems.push(`- ${compact}`)
      }
    }
  }

  if (!userItems.length && !assistantItems.length) return ''

  const parts = ['以下是当前会话中较早消息的摘要，请基于此继续对话，不要假设摘要之外的旧细节仍然准确。']
  if (userItems.length) {
    parts.push('较早的用户诉求与补充：')
    parts.push(...userItems)
  }
  if (assistantItems.length) {
    parts.push('较早的助手答复与已完成事项：')
    parts.push(...assistantItems)
  }
  return parts.join('\n')
}

function buildSessionMemory(session: AgentSession): AgentSessionMemory | null {
  const recentUserGoals = session.messages
    .filter((message) => message.role === 'user')
    .slice(-3)
    .map((message) => compactMessageText(message.content, 180))
    .filter((item): item is string => Boolean(item))

  const completedFacts: string[] = []
  if (session.lastExecutionMemory?.assistantSummary) {
    completedFacts.push(session.lastExecutionMemory.assistantSummary)
  }
  if (session.lastExecutionMemory?.planCompletedSteps?.length) {
    completedFacts.push(...session.lastExecutionMemory.planCompletedSteps.slice(-3))
  }

  const openLoops: string[] = []
  if (session.pendingPlan) {
    openLoops.push(...parsePlanSteps(session.pendingPlan))
  } else if (session.runtimePlan?.status === 'running') {
    openLoops.push(
      ...session.runtimePlan.steps
        .filter((step) => step.status === 'running' || step.status === 'pending')
        .map((step) => step.title)
        .slice(0, 3),
    )
  }

  const recentSummary = compactMessageText(buildHistorySummary(session.messages.slice(-6)), 400) || null
  if (!recentSummary && !recentUserGoals.length && !completedFacts.length && !openLoops.length) {
    return null
  }

  return {
    summary: recentSummary,
    activeUserGoals: recentUserGoals,
    completedFacts: completedFacts.slice(-4),
    openLoops: openLoops.slice(0, 4),
    updatedAt: new Date().toISOString(),
  }
}

function buildConversationMessages(
  messages: AgentMessage[],
  options: { preserveFullHistory?: boolean } = {},
): AgentRequestMessage[] {
  const nonEmptyMessages = messages
    .map((message) => ({
      ...message,
      content: message.content.trim(),
    }))
    .filter((message) => (Boolean(message.content) || Boolean(message.attachments?.length)) && message.role !== 'system')
  const normalized = nonEmptyMessages.map((message) => ({
    role: message.role,
    content: message.content,
    attachments: message.attachments || [],
  }))

  if (!normalized.length) return []
  return normalized
}

function extractPlanBlock(content: string) {
  const match = content.match(/\[\[PLAN\]\]([\s\S]*?)\[\[\/PLAN\]\]/i)
  return match?.[1]?.trim() || ''
}

function parsePlanSteps(plan: string) {
  return plan
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => line.replace(/^\d+\s*[\.\)、]\s*/, '').trim())
    .filter(Boolean)
}

function hasActivePendingPlan(session: AgentSession) {
  return Boolean(session.pendingPlan?.trim())
}

function isNeedsSaveContinuation(session: AgentSession) {
  if (hasActivePendingPlan(session)) return false
  return session.lastExecutionMemory?.awaiting === 'user_confirm_write'
}

function isLikelySaveFollowUpReply(text: string) {
  const normalized = text.trim().toLowerCase()
  if (!normalized) return false
  const compact = normalized.replace(/\s+/g, '')
  const negativePatterns = [
    '不保存',
    '先不保存',
    '暂不保存',
    '不用保存',
    '别保存',
    '取消保存',
    '稍后保存',
    "don'tsave",
    'notsave',
  ]
  if (negativePatterns.some((pattern) => compact.includes(pattern))) {
    return false
  }
  const explicitSavePatterns = [
    '保存',
    '存一下',
    '请保存',
    '确认保存',
    'save',
    'submit',
    'apply',
  ]
  if (explicitSavePatterns.some((pattern) => compact.includes(pattern))) {
    return true
  }
  return [
    '好',
    '好的',
    '行',
    '行的',
    '可以',
    '确认',
    '确定',
    '是',
    '是的',
    '嗯',
    '嗯嗯',
    'ok',
    'okay',
    'yes',
    'y',
    'sure',
  ].includes(compact)
}

function shouldTreatPlanAsPending(options: {
  planText: string
  control: AgentControlBlock | null
  session: AgentSession
  executionState: AgentExecutionState
  sawPlanSignal: boolean
}) {
  const planText = options.planText.trim()
  if (!planText) return false
  if (options.control?.awaiting === 'user_confirm_write') return false
  if (options.executionState.saveRequested) return false
  if (!options.executionState.confirmationRequired) return false
  if (options.executionState.pendingPlanUserReply?.trim()) return false
  if (options.executionState.planConfirmationDecision === 'approved') return false
  if (options.executionState.semanticContinuation) return false
  if (options.control?.currentMode === 'plan' && options.control?.awaiting === 'user_input') return true
  const runtimePlanStatus = options.session.runtimePlan?.status?.trim()
  const hasPlanRuntime = options.session.taskAnalysis?.mode === 'plan'
    || runtimePlanStatus === 'pending'
  return options.sawPlanSignal && hasPlanRuntime
}

function isPartialWriteTask(taskAnalysis: AgentTaskAnalysis | null) {
  return taskAnalysis?.writeScope === 'partial'
}

function appendToolEventsToSession(session: AgentSession, roundToolCalls: AgentExecutionToolCallSummary[]) {
  if (!roundToolCalls.length) return
  const nextEvents = roundToolCalls.map((call) => ({
    id: genId(),
    tool: call.name,
    status: call.outcome,
    summary: call.outcome === 'noop'
      ? `${call.name} 未产生新的状态变更`
      : call.outcome === 'error'
        ? `${call.name} 执行失败`
        : `${call.name} 已执行`,
  } satisfies AgentToolEvent))
  session.toolEvents = [...session.toolEvents, ...nextEvents].slice(-20)
}

function isDocumentMutationTool(name: string) {
  return name === 'rewrite_document_section'
    || name === 'replace_document_block'
    || name === 'replace_document_blocks'
    || name === 'swap_document_sections'
}

function mergePendingPlanToolOutputs(
  existing: AgentToolOutputPayload[],
  nextBatch: AgentToolOutputPayload[],
) {
  if (!nextBatch.length) return existing
  const merged = [...existing]
  const indexByCallId = new Map(merged.map((item, index) => [item.call_id, index]))
  for (const output of nextBatch) {
    const existingIndex = indexByCallId.get(output.call_id)
    if (existingIndex !== undefined) {
      merged[existingIndex] = output
      continue
    }
    indexByCallId.set(output.call_id, merged.length)
    merged.push(output)
  }
  return merged.slice(-8)
}

function syncRuntimePlanStatus(
  session: AgentSession,
  control: AgentControlBlock | null,
  executionState: AgentExecutionState,
) {
  const runtimePlan = session.runtimePlan
  if (!runtimePlan) return
  const planStepIndex = Number.isFinite(control?.planStepIndex) ? Number(control?.planStepIndex) : executionState.planStepIndex
  const actionStatus = typeof control?.currentActionStatus === 'string' && control.currentActionStatus.trim()
    ? control.currentActionStatus.trim()
    : executionState.currentActionStatus?.trim() || ''
  const awaitingConfirmation = (
    (control?.currentMode || executionState.currentMode) === 'plan'
    && (control?.awaiting || executionState.awaiting) === 'user_input'
  )
  const planCurrentStep = typeof control?.planCurrentStep === 'string' && control.planCurrentStep.trim()
    ? control.planCurrentStep.trim()
    : executionState.planCurrentStep?.trim() || ''
  const completedStepTitles = (
    (Array.isArray(control?.planCompletedSteps) && control?.planCompletedSteps?.length
      ? control.planCompletedSteps
      : executionState.planCompletedSteps
    )
      .map((step) => step.trim())
      .filter(Boolean)
  )
  const completedIndices = new Set(resolvePlanStepIndicesByTitles(runtimePlan.steps, completedStepTitles))
  let activeIndex = Number.isFinite(planStepIndex) && (planStepIndex || 0) > 0
    ? Number(planStepIndex) - 1
    : -1
  if (activeIndex < 0 && planCurrentStep) {
    activeIndex = resolveCurrentPlanStepIndexByTitle(runtimePlan.steps, planCurrentStep, completedIndices)
  }
  if (activeIndex < 0) {
    activeIndex = runtimePlan.steps.findIndex((_, index) => !completedIndices.has(index))
  }
  const lastStepIndex = runtimePlan.steps.length - 1
  const allStepsExplicitlyCompleted = runtimePlan.steps.every((_, index) => completedIndices.has(index))

  runtimePlan.steps = runtimePlan.steps.map((step, index) => {
    const explicitCompleted = completedIndices.has(index)
    if (explicitCompleted) return { ...step, status: 'completed' }
    if (awaitingConfirmation) return { ...step, status: 'pending' }
    if (activeIndex >= 0 && index < activeIndex) return { ...step, status: 'completed' }
    if (activeIndex >= 0 && index === activeIndex) {
      if (actionStatus === 'failed') return { ...step, status: 'failed' }
      if (actionStatus === 'completed' && (allStepsExplicitlyCompleted || index === lastStepIndex)) {
        return { ...step, status: 'completed' }
      }
      return {
        ...step,
        status: 'running',
      }
    }
    return { ...step, status: 'pending' }
  })

  const hasRunningSteps = runtimePlan.steps.some((step) => step.status === 'running')
  const hasPendingSteps = runtimePlan.steps.some((step) => step.status === 'pending')
  const hasFailedSteps = runtimePlan.steps.some((step) => step.status === 'failed')
  const allStepsCompleted = runtimePlan.steps.every((step) => step.status === 'completed')
  const hasStartedSteps = runtimePlan.steps.some((step) => step.status === 'running' || step.status === 'completed')
  if (hasFailedSteps) runtimePlan.status = 'failed'
  else if (awaitingConfirmation) runtimePlan.status = 'pending'
  else if (allStepsCompleted) runtimePlan.status = 'completed'
  else if (hasRunningSteps || hasStartedSteps) runtimePlan.status = 'running'
  else if (hasPendingSteps) runtimePlan.status = 'pending'
  runtimePlan.updatedAt = new Date().toISOString()
}

function currentRuntimePlanStep(
  runtimePlan: AgentRuntimePlan | null,
  executionState: AgentExecutionState,
  control: AgentControlBlock | null = null,
) {
  if (!runtimePlan?.steps?.length) return null
  const planStepIndex = Number.isFinite(control?.planStepIndex)
    ? Number(control?.planStepIndex)
    : executionState.planStepIndex
  if (planStepIndex && planStepIndex > 0 && runtimePlan.steps[planStepIndex - 1]) {
    return runtimePlan.steps[planStepIndex - 1]
  }
  const planCurrentStep = typeof control?.planCurrentStep === 'string' && control.planCurrentStep.trim()
    ? control.planCurrentStep.trim()
    : executionState.planCurrentStep?.trim() || ''
  if (planCurrentStep) {
    return runtimePlan.steps.find((step) => step.title.trim() === planCurrentStep) || null
  }
  return runtimePlan.steps.find((step) => step.status === 'running') || null
}

function currentStepRequiresDocumentWrite(
  runtimePlan: AgentRuntimePlan | null,
  executionState: AgentExecutionState,
  control: AgentControlBlock | null = null,
) {
  const runtimeStep = currentRuntimePlanStep(runtimePlan, executionState, control)
  if (runtimeStep?.requiresDocumentWrite === true) return true
  if (runtimePlan?.steps?.length) return false
  return Boolean(
    (control?.writeScope || control?.preferredWriteAction)
    && (control?.writeCompleted !== true && executionState.writeCompleted !== true),
  )
}

function isAwaitingPlanConfirmation(
  executionState: AgentExecutionState,
  control: AgentControlBlock | null = null,
) {
  if (control?.currentMode === 'plan' && control?.awaiting === 'user_input') return true
  if (!executionState.confirmationRequired) return false
  if (executionState.pendingPlanUserReply?.trim()) return false
  return executionState.planConfirmationDecision !== 'approved'
}

function currentStepRequiresDocumentSave(
  runtimePlan: AgentRuntimePlan | null,
  executionState: AgentExecutionState,
  control: AgentControlBlock | null = null,
) {
  const runtimeStep = currentRuntimePlanStep(runtimePlan, executionState, control)
  if (runtimeStep?.requiresDocumentSave === true) return true
  return executionState.saveRequested || control?.saveRequested === true
}

function currentPlanStepKey(
  executionState: AgentExecutionState,
  control: AgentControlBlock | null = null,
) {
  const planStepIndex = Number.isFinite(control?.planStepIndex)
    ? Number(control?.planStepIndex)
    : executionState.planStepIndex
  if (planStepIndex && planStepIndex > 0) {
    return `index:${planStepIndex}`
  }
  const planCurrentStep = typeof control?.planCurrentStep === 'string' && control.planCurrentStep.trim()
    ? control.planCurrentStep.trim()
    : executionState.planCurrentStep?.trim() || ''
  return planCurrentStep ? `title:${planCurrentStep}` : null
}

function hasRemainingStructuredPlanWork(
  runtimePlan: AgentRuntimePlan | null,
  executionState: AgentExecutionState,
  control: AgentControlBlock | null = null,
) {
  if (!runtimePlan?.steps?.length) return false
  if (runtimePlan.status === 'failed' || runtimePlan.status === 'blocked') {
    return false
  }

  const planStepIndex = Number.isFinite(control?.planStepIndex)
    ? Number(control?.planStepIndex)
    : executionState.planStepIndex

  if (planStepIndex && runtimePlan.steps.length >= planStepIndex) {
    return runtimePlan.steps.slice(planStepIndex - 1).some((step) => step.status !== 'completed')
  }

  return runtimePlan.steps.some((step) => step.status === 'pending' || step.status === 'running')
}

function advanceExecutionPlanStep(
  runtimePlan: AgentRuntimePlan | null,
  executionState: AgentExecutionState,
  control: AgentControlBlock | null,
  options: {
    wroteDocument: boolean
    savedDocument: boolean
    mutationCompleted?: boolean
  },
) {
  if (!runtimePlan?.steps?.length || executionState.pendingPlan) return false

  const currentStep = currentRuntimePlanStep(runtimePlan, executionState, control)
  if (!currentStep) return false

  const currentIndex = runtimePlan.steps.findIndex((step) => step.id === currentStep.id)
  if (currentIndex < 0) return false

  const writeSatisfied = options.wroteDocument || executionState.writeCompleted || control?.writeCompleted === true
  const saveSatisfied = options.savedDocument
  const mutationSatisfied = options.mutationCompleted === true
  const requiresWrite = currentStepRequiresDocumentWrite(runtimePlan, executionState, control)
  const requiresSave = currentStepRequiresDocumentSave(runtimePlan, executionState, control)
  const canAdvance = saveSatisfied
    || (requiresWrite && writeSatisfied && !requiresSave)
    || (!requiresWrite && mutationSatisfied && !requiresSave)
  if (!canAdvance) return false

  const currentTitle = currentStep.title.trim()
  if (currentTitle && !executionState.planCompletedSteps.includes(currentTitle)) {
    executionState.planCompletedSteps = [...executionState.planCompletedSteps, currentTitle]
  }

  const nextStep = runtimePlan.steps[currentIndex + 1] || null
  executionState.planStepIndex = nextStep ? currentIndex + 2 : currentIndex + 1
  executionState.planCurrentStep = nextStep ? nextStep.title : currentStep.title
  executionState.writeCompleted = false
  executionState.saveRequested = false
  return true
}

function buildExecutionProgressSignature(
  executionState: AgentExecutionState,
  control: AgentControlBlock | null,
) {
  const mode = typeof control?.currentMode === 'string' && control.currentMode.trim()
    ? control.currentMode.trim()
    : executionState.currentMode
  const awaiting = typeof control?.awaiting === 'string' && control.awaiting.trim()
    ? control.awaiting.trim()
    : executionState.awaiting || ''
  const currentStep = typeof control?.planCurrentStep === 'string' && control.planCurrentStep.trim()
    ? control.planCurrentStep.trim()
    : executionState.planCurrentStep?.trim() || ''
  const completedSteps = executionState.planCompletedSteps
    .map((step) => step.trim())
    .filter(Boolean)
    .join('||')
  return [
    mode,
    awaiting,
    String(executionState.planStepIndex || ''),
    String(executionState.planTotalSteps || ''),
    currentStep,
    completedSteps,
    executionState.writeCompleted ? 'write:1' : 'write:0',
    executionState.saveRequested ? 'save:1' : 'save:0',
    executionState.documentWriteObserved ? 'doc:1' : 'doc:0',
  ].join('::')
}

function upsertArtifactDraft(
  session: AgentSession,
  contentDelta: string,
  options: { finalize?: boolean; docId?: number | null; docName?: string | null } = {},
) {
  const title = options.docName?.trim() ? `${options.docName.trim()} 草稿` : 'Markdown 草稿'
  const relatedDocId = Number.isFinite(options.docId) ? Number(options.docId) : null
  let artifact = session.artifacts.find((item) => item.status === 'drafting' && item.relatedDocId === relatedDocId) || null
  if (!artifact) {
    artifact = {
      id: genId(),
      type: 'markdown_doc',
      title,
      status: 'drafting',
      content: '',
      relatedDocId,
    }
    session.artifacts = [...session.artifacts, artifact].slice(-6)
  }
  artifact.content = `${artifact.content}${contentDelta}`
  artifact.status = options.finalize ? 'ready' : 'drafting'
}

function buildRequestBody(
  messages: AgentRequestMessage[],
  provider: AgentProvider,
  model: string,
  options: {
    transportMode?: 'auto' | 'responses' | 'chat'
    previousResponseId?: string | null
    toolOutputs?: AgentToolOutputPayload[] | null
    agentExecution?: AgentExecutionState | null
    lastExecutionMemory?: AgentExecutionMemory | null
    sessionMemory?: AgentSessionMemory | null
  } = {},
) {
  return {
    provider: {
      provider_id: Number(provider.id),
      model,
    },
    messages: messages.map((message) => ({
      role: message.role,
      content: message.content,
      attachments: (message.attachments || []).map((attachment) => ({
        upload_id: attachment.uploadId,
        kind: attachment.kind,
        name: attachment.name,
        url: attachment.url,
        content_type: attachment.contentType,
        size: attachment.size,
      })),
    })),
    mode: 'auto',
    transport_mode: options.transportMode || 'auto',
    context: {
      page_scope: props.pageScope,
      page_state: props.pageState || null,
      project_name: props.projectName || null,
      doc_id: props.docId,
      doc_name: props.docName || null,
      project_catalog: props.projectCatalog || null,
      current_node_catalog: props.currentNodeCatalog || null,
      editor_available: currentEditorAvailable(),
      editor_snapshot_source: currentEditorSnapshotSource(),
      editor_unsaved_changes: currentDocumentHasUnsavedChanges(),
      agent_execution: options.agentExecution ? buildAgentExecutionContext(options.agentExecution) : null,
      last_execution: options.lastExecutionMemory ? buildAgentExecutionMemory(options.lastExecutionMemory) : null,
      session_memory: options.sessionMemory ? buildAgentSessionMemory(options.sessionMemory) : null,
    },
    previous_response_id: options.previousResponseId || null,
    tool_outputs: options.toolOutputs || null,
  }
}

function parseRouteMarker(raw: string): AgentRouteTarget | null {
  const match = raw.match(/^\[\[ROUTE:(overview|project|doc)(?::([\s\S]*?))?\]\]$/)
  if (!match) return null

  const kind = match[1] as RouteKind
  const name = match[2]?.trim()
  if ((kind === 'project' || kind === 'doc') && !name) return null

  return {
    kind,
    name,
  }
}

function parseSseBlock(block: string) {
  let eventName = 'message'
  const dataParts: string[] = []

  for (const line of block.split('\n')) {
    if (line.startsWith('event:')) {
      eventName = line.slice(6).trim()
      continue
    }
    if (line.startsWith('data:')) {
      dataParts.push(line.slice(5).trim())
    }
  }

  if (!dataParts.length) return null
  const raw = dataParts.join('\n')
  try {
    return {
      event: eventName,
      data: JSON.parse(raw),
    }
  } catch {
    return {
      event: eventName,
      data: { value: raw },
    }
  }
}

function scrollMessagesToBottom() {
  void nextTick().then(() => {
    window.requestAnimationFrame(() => {
      const el = messagesRef.value
      if (!el) return
      el.scrollTop = el.scrollHeight
    })
  })
}

function handleComposerKeydown(event: KeyboardEvent) {
  if (event.key !== 'Enter') return
  if (event.isComposing) return

  if (event.ctrlKey || event.metaKey) {
    event.preventDefault()
    void sendMessage()
  }
}

function stopStreaming() {
  activeStreamController?.abort()
}

async function sendMessage() {
  if (streaming.value) return
  const session = ensureSession()
  const text = prompt.value.trim()
  const attachments = [...composerAttachments.value]
  if (!text && !attachments.length) return
  if (attachmentUploading.value) {
    ElMessage.warning('附件仍在上传中，请稍候再发送')
    return
  }

  const provider = activeProvider.value
  if (!provider) {
    ElMessage.warning('请先配置并激活一个供应商')
    openProviderDialog()
    return
  }
  if (!provider.hasApiKey) {
    ElMessage.warning('当前激活供应商缺少 API Key')
    openProviderDialog()
    return
  }

  syncSessionWithActiveProvider(session)
  const pendingPlan = (session.pendingPlan?.trim() || currentConfirmationPlan.value || '').trim()
  const continuingNeedsSave = !pendingPlan && isNeedsSaveContinuation(session) && isLikelySaveFollowUpReply(text)
  const shouldPreserveExecutionContext = Boolean(pendingPlan || continuingNeedsSave)
  const carriedPendingPlanToolOutputs = shouldPreserveExecutionContext
    ? [...session.pendingPlanToolOutputs]
    : []
  let collectedPlanToolOutputs = shouldPreserveExecutionContext
    ? [...session.pendingPlanToolOutputs]
    : []
  if (!shouldPreserveExecutionContext) {
    session.pendingPlan = null
    session.pendingPlanToolOutputs = []
    session.runtimePlan = null
    session.taskAnalysis = null
    session.lastPlan = ''
  }
  const existingMessages = [...session.messages]

  if (!session.model.trim()) {
    ElMessage.warning('请先在模型管理中配置可选模型')
    showModelDialog.value = true
    return
  }

  const userMessage: AgentMessage = { id: genId(), role: 'user', content: text, attachments }
  let assistantMessage: AgentMessage = { id: genId(), role: 'assistant', content: '', reasoning: '' }
  session.messages.push(userMessage)
  session.messages.push(assistantMessage)
  session.updatedAt = Date.now()
  session.providerId = provider.id

  if (session.title === '新会话') {
    session.title = text
      ? text.slice(0, 18)
      : attachments[0]?.name?.slice(0, 18) || '附件会话'
  }

  sessions.value = [...sessions.value]
  persistSessions()
  scrollMessagesToBottom()
  prompt.value = ''
  clearComposerAttachments()
  streaming.value = true
  streamingAssistantId.value = assistantMessage.id
  liveAssistantContent.value = ''
  liveAssistantReasoning.value = ''
  let routeAction: StreamAction | null = null
  let prefixProbe = ''
  let pendingRoute: AgentRouteTarget | null = null
  let writerStarted = false
  let renderBuffer = ''
  let inThinkBlock = false
  let streamFailed = false
  let pendingToolOutputs: AgentToolOutputPayload[] | null = null
  let streamAborted = false
  let currentAgentRunId = ''
  const pendingFrontendToolRequests: Promise<void>[] = []
  let previousResponseId: string | null = (
    session.previousResponseId
    && session.previousResponseId.trim()
    && session.previousResponseId.trim() !== session.model.trim()
  )
    ? session.previousResponseId.trim()
    : null
  if (session.previousResponseId && !previousResponseId) {
    session.previousResponseId = null
  }
  const pendingPlanSteps = pendingPlan ? parsePlanSteps(pendingPlan) : []
  const continuationMemory = pendingPlan || continuingNeedsSave
    ? session.lastExecutionMemory
    : null
  const compositeWriteThenSaveRequest = false
  const executionState: AgentExecutionState = {
    currentMode: pendingPlan || session.runtimePlan?.steps?.length
      ? 'plan'
      : continuationMemory?.currentMode || 'normal',
    awaiting: pendingPlan ? 'user_input' : continuationMemory?.awaiting ?? null,
    currentActionKind: continuationMemory?.currentActionKind ?? null,
    currentActionStatus: continuationMemory?.currentActionStatus ?? null,
    currentActionMode: continuationMemory?.currentActionMode ?? null,
    currentActionTarget: continuationMemory?.currentActionTarget ?? null,
    confirmationRequired: Boolean(pendingPlan),
    pendingPlan: pendingPlan || null,
    pendingPlanUserReply: pendingPlan ? text : null,
    planConfirmationDecision: null,
    compositeWriteThenSave: compositeWriteThenSaveRequest,
    semanticContinuation: false,
    semanticContinuationRound: 0,
    previousAssistantSummary: null,
    taskKind: continuingNeedsSave ? continuationMemory?.taskKind ?? null : null,
    editIntent: continuingNeedsSave ? continuationMemory?.editIntent ?? null : null,
    editStage: continuingNeedsSave ? continuationMemory?.editStage ?? null : null,
    saveRequested: continuingNeedsSave,
    writeCompleted: continuingNeedsSave
      ? Boolean(continuationMemory?.writeCompleted || continuationMemory?.documentWriteObserved)
      : false,
    planStepIndex: continuationMemory
      ? continuationMemory.planStepIndex ?? (pendingPlanSteps.length ? 1 : null)
      : null,
    planTotalSteps: continuationMemory
      ? continuationMemory.planTotalSteps ?? (pendingPlanSteps.length || null)
      : null,
    planCurrentStep: continuationMemory
      ? continuationMemory.planCurrentStep ?? pendingPlanSteps[0] ?? null
      : null,
    planCompletedSteps: continuationMemory ? [...(continuationMemory.planCompletedSteps || [])] : [],
    documentWriteObserved: continuingNeedsSave
      ? Boolean(continuationMemory?.documentWriteObserved || continuationMemory?.writeCompleted)
      : false,
    saveAttemptWithoutDocumentChange: false,
    lastInterceptCode: null,
    lastInterceptMessage: null,
    lastInterceptGuidance: null,
    recentToolCalls: [],
  }
  let semanticContinuationRounds = 0
  let toolCallRounds = 0
  const toolCallSignatureHits = new Map<string, number>()
  let nonWritingPlanRounds = 0
  let idleSemanticContinuationRounds = 0
  let rawAssistantContent = ''
  let completedAssistantContent = ''
  let sawPlanSignalThisRound = false
  let wroteDocument = false
  let writeSatisfiedStepKey: string | null = null
  let liveAssistantRoundSummary = ''
  let saveToolAttemptedThisTurn = false
  let saveToolSucceededThisTurn = false
  let planStepAdvancedThisRound = false
  let roundDocumentWriteObserved = false
  let roundToolCalls: AgentExecutionToolCallSummary[] = []
  let consumeAssistantText = (_rawChunk: string, _force = false) => {}
  let handleAssistantChunk = (_rawChunk: string) => {}
  let routeChunk = (_rawChunk: string, _force = false) => {}
  let recoverTrailingActionMarker = () => {}
  let buildVisibleAssistantContent = (_source = rawAssistantContent, _streamMode = false) => _source
  let appendUnsavedDraftNotice = (_control: AgentControlBlock | null) => {}
  let finalizePendingAssistantOutput = (_options?: { allowIncompleteAction?: boolean }) => {}
  let finalizeAssistantMessageForDisplay = (_finalContent: string, _options?: { includeStructuredMessage?: boolean }) => {}
  let appendFinalLoopResultMessage = (_resultContent: string) => {}
  let syncExecutionPlanProgress = (_control: AgentControlBlock | null) => {}
  let syncPendingPlanPreviewFromStream = () => {}
  let finalControl: AgentControlBlock | null = null
  let latestStructuredResponse: AgentStructuredResponse | null = null
  let latestToolDrivenControl: AgentControlBlock | null = null
  let latestToolDrivenPlan: AgentRuntimePlan | null = null
  let hostStateUpdatedThisRound = false
  let lastWriterResult: AgentWriterResultDetail | null = null
  let interceptedFailure: ReturnType<typeof resolveAgentInterceptDetails> | null = null
  let runtimeSummaryAppendedThisRequest = false
  const selectedTransportMode = session.transportMode || 'auto'
  let requestRound = 0
  const handleWriterResult = (event: Event) => {
    const detail = (event as CustomEvent<AgentWriterResultDetail>).detail
    if (!detail || detail.docId !== props.docId) return
    lastWriterResult = detail
  }
  const readLatestWriterResult = (): AgentWriterResultDetail | null => lastWriterResult
  window.addEventListener(AGENT_WRITER_RESULT_EVENT, handleWriterResult as EventListener)

  try {
    const abortController = new AbortController()
    activeStreamController = abortController

    const resetSemanticContinuationBudget = () => {
      semanticContinuationRounds = 0
      executionState.semanticContinuation = false
      executionState.semanticContinuationRound = 0
      executionState.previousAssistantSummary = null
    }

    const shouldTriggerSemanticContinuation = (finalContent: string, control: AgentControlBlock | null) => {
      void finalContent
      void control
      return false
    }

    const enqueueSemanticContinuation = (finalContent: string) => {
      semanticContinuationRounds += 1
      executionState.semanticContinuation = true
      executionState.semanticContinuationRound = semanticContinuationRounds
      executionState.confirmationRequired = false
      executionState.pendingPlan = null
      executionState.pendingPlanUserReply = null
      executionState.planConfirmationDecision = null
      session.pendingPlan = null
      executionState.previousAssistantSummary =
        compactMessageText(finalContent, 320)
        || currentRuntimePlanStep(session.runtimePlan, executionState)?.title
        || executionState.planCurrentStep
        || '继续执行当前计划'
    }

    syncExecutionPlanProgress = (control: AgentControlBlock | null) => {
      if (!control) return
      executionState.currentMode = typeof control.currentMode === 'string' && control.currentMode.trim()
        ? control.currentMode.trim()
        : executionState.currentMode
      executionState.awaiting = typeof control.awaiting === 'string' && control.awaiting.trim()
        ? control.awaiting.trim()
        : control.awaiting === null
          ? null
          : executionState.awaiting
      executionState.currentActionKind = typeof control.currentActionKind === 'string' && control.currentActionKind.trim()
        ? control.currentActionKind.trim()
        : control.currentActionKind === null
          ? null
          : executionState.currentActionKind
      executionState.currentActionStatus = typeof control.currentActionStatus === 'string' && control.currentActionStatus.trim()
        ? control.currentActionStatus.trim()
        : control.currentActionStatus === null
          ? null
          : executionState.currentActionStatus
      executionState.currentActionMode = typeof control.currentActionMode === 'string' && control.currentActionMode.trim()
        ? control.currentActionMode.trim()
        : control.currentActionMode === null
          ? null
          : executionState.currentActionMode
      executionState.currentActionTarget = typeof control.currentActionTarget === 'string' && control.currentActionTarget.trim()
        ? control.currentActionTarget.trim()
        : control.currentActionTarget === null
          ? null
          : executionState.currentActionTarget
      if (control.confirmationRequired === true) {
        executionState.confirmationRequired = true
      }
      if (session.taskAnalysis) {
        if (typeof control.writeScope === 'string' && control.writeScope.trim()) {
          session.taskAnalysis.writeScope = control.writeScope.trim()
        }
        if (typeof control.preferredWriteAction === 'string' && control.preferredWriteAction.trim()) {
          session.taskAnalysis.preferredWriteAction = control.preferredWriteAction.trim()
        }
      }
      executionState.taskKind = typeof control.taskKind === 'string' && control.taskKind.trim()
        ? control.taskKind.trim()
        : executionState.taskKind
      executionState.editIntent = typeof control.editIntent === 'string' && control.editIntent.trim()
        ? control.editIntent.trim()
        : executionState.editIntent
      executionState.editStage = typeof control.editStage === 'string' && control.editStage.trim()
        ? control.editStage.trim()
        : executionState.editStage
      if (control.saveRequested === true) {
        executionState.saveRequested = true
      }
      if (control.writeCompleted === true) {
        executionState.writeCompleted = true
      }
      executionState.planStepIndex = Number.isFinite(control.planStepIndex) ? Number(control.planStepIndex) : executionState.planStepIndex
      executionState.planTotalSteps = Number.isFinite(control.planTotalSteps) ? Number(control.planTotalSteps) : executionState.planTotalSteps
      executionState.planCurrentStep = typeof control.planCurrentStep === 'string' && control.planCurrentStep.trim()
        ? control.planCurrentStep.trim()
        : executionState.planCurrentStep
      if (Array.isArray(control.planCompletedSteps) && control.planCompletedSteps.length) {
        executionState.planCompletedSteps = [...control.planCompletedSteps]
      }
      if (control.writeCompleted === true) {
        writeSatisfiedStepKey = currentPlanStepKey(executionState, control) || writeSatisfiedStepKey
      }
    }

    const hasImplicitControlSignals = () => Boolean(
      latestStructuredResponse?.state
      || latestToolDrivenControl
      || hostStateUpdatedThisRound
      || session.pendingPlan?.trim()
      || session.runtimePlan?.steps?.length
      || executionState.confirmationRequired
      || executionState.pendingPlan
      || executionState.planCurrentStep
      || executionState.planStepIndex
      || executionState.planCompletedSteps.length
      || executionState.saveRequested
      || executionState.writeCompleted
      || executionState.documentWriteObserved
      || roundToolCalls.length
      || roundDocumentWriteObserved
      || wroteDocument
    )

    const resolveEffectiveControl = (content: string) => {
      const explicitControl = latestStructuredResponse?.state || latestToolDrivenControl
      if (explicitControl) return explicitControl
      if (!hasImplicitControlSignals()) return null
      return buildControlFromExecutionState(executionState, session.taskAnalysis)
    }

    const setCurrentActionState = (
      kind: string | null,
      status: string | null,
      options: {
        mode?: string | null
        target?: string | null
      } = {},
    ) => {
      executionState.currentActionKind = kind
      executionState.currentActionStatus = status
      executionState.currentActionMode = options.mode ?? executionState.currentActionMode
      executionState.currentActionTarget = options.target ?? executionState.currentActionTarget
    }

    const adoptRuntimePlan = (plan: AgentRuntimePlan | null) => {
      if (!plan) return
      session.runtimePlan = plan
      latestToolDrivenPlan = plan
      const planText = runtimePlanToPlanText(plan)
      if (planText) {
        session.lastPlan = planText
      }
      executionState.planTotalSteps = plan.steps.length || null
    }

    const clearPendingPlanState = () => {
      executionState.confirmationRequired = false
      executionState.pendingPlan = null
      executionState.pendingPlanUserReply = null
      executionState.planConfirmationDecision = null
      session.pendingPlan = null
      session.pendingPlanToolOutputs = []
    }

    const syncPlanCursorToIndex = (plan: AgentRuntimePlan | null, index: number | null) => {
      executionState.planStepIndex = index
      if (!plan || !index || index < 1 || index > plan.steps.length) {
        executionState.planCurrentStep = null
        return
      }
      executionState.planCurrentStep = plan.steps[index - 1]?.title || null
    }

    const buildPlanFromHostArgs = (args: Record<string, any>) => {
      const explicitPlan = normalizeRuntimePlan(parseJsonLikeValue(args.plan))
      if (explicitPlan) return explicitPlan

      if (Array.isArray(args.steps)) {
        const normalizedPlan = normalizeRuntimePlan({
          id: typeof args.id === 'string' && args.id.trim() ? args.id.trim() : genId(),
          goal: typeof args.goal === 'string' && args.goal.trim() ? args.goal.trim() : (userMessage?.content || pendingPlan || '执行计划'),
          summary: typeof args.summary === 'string' && args.summary.trim() ? args.summary.trim() : null,
          status: 'pending',
          steps: args.steps,
        })
        if (normalizedPlan) return normalizedPlan
      }

      const explicitPlanText = typeof args.plan_text === 'string' && args.plan_text.trim()
        ? args.plan_text.trim()
        : typeof args.planText === 'string' && args.planText.trim()
          ? args.planText.trim()
          : ''
      if (!explicitPlanText) return null

      return buildRuntimePlanFromText(
        explicitPlanText,
        typeof args.goal === 'string' && args.goal.trim()
          ? args.goal.trim()
          : (userMessage?.content || pendingPlan || ''),
        session.runtimePlan,
      )
    }

    const flushOpenWriteStream = () => {
      if (!(routeAction && routeAction !== 'chat')) return
      routeChunk('', true)
      consumeAssistantText('', true)
      completeWriterBlock()
    }

    const executeHostStateToolCall = async (call: AgentToolCall): Promise<AgentToolOutputPayload> => {
      const args = parseToolArguments(call.arguments) || {}
      const control = normalizeControlPayload(args)
      let nextPlan = buildPlanFromHostArgs(args)

      switch (call.name) {
        case 'plan_start': {
          if (nextPlan) {
            adoptRuntimePlan(nextPlan)
            executionState.planCompletedSteps = []
            executionState.currentMode = 'plan'
            syncPlanCursorToIndex(nextPlan, nextPlan.steps.length ? 1 : null)
          }
          const shouldAwaitConfirmation = session.taskAnalysis?.requiresUserConfirmation !== false
          const pendingPlanText = (nextPlan ? runtimePlanToPlanText(nextPlan) : session.lastPlan || '').trim() || null
          if (shouldAwaitConfirmation && pendingPlanText) {
            executionState.confirmationRequired = true
            executionState.pendingPlan = pendingPlanText
            executionState.pendingPlanUserReply = null
            executionState.planConfirmationDecision = null
            executionState.awaiting = 'user_input'
            session.pendingPlan = pendingPlanText
          } else {
            clearPendingPlanState()
            executionState.awaiting = null
          }
          setCurrentActionState('plan_start', 'completed')
          break
        }
        case 'plan_step_update': {
          const stepId = typeof args.step_id === 'string' && args.step_id.trim()
            ? args.step_id.trim()
            : typeof args.stepId === 'string' && args.stepId.trim()
              ? args.stepId.trim()
              : ''
          const nextStatus = typeof args.status === 'string' && args.status.trim()
            ? args.status.trim()
            : 'completed'
          const plan = session.runtimePlan
          executionState.currentMode = 'plan'
          if (plan && stepId) {
            const stepIndex = plan.steps.findIndex((step) => step.id === stepId)
            if (stepIndex >= 0) {
              plan.steps[stepIndex] = {
                ...plan.steps[stepIndex],
                status: nextStatus === 'failed' ? 'failed' : 'completed',
              }
              if (nextStatus === 'failed') {
                plan.status = 'failed'
                executionState.awaiting = 'user_input'
                syncPlanCursorToIndex(plan, stepIndex + 1)
              } else {
                const completedTitle = plan.steps[stepIndex].title.trim()
                if (completedTitle && !executionState.planCompletedSteps.includes(completedTitle)) {
                  executionState.planCompletedSteps.push(completedTitle)
                }
                const nextIndex = plan.steps.findIndex((step) => step.status !== 'completed')
                if (nextIndex >= 0) {
                  plan.status = 'running'
                  executionState.awaiting = null
                  syncPlanCursorToIndex(plan, nextIndex + 1)
                } else {
                  plan.status = 'completed'
                  executionState.awaiting = null
                  executionState.planStepIndex = plan.steps.length || null
                  executionState.planCurrentStep = null
                }
              }
              plan.updatedAt = new Date().toISOString()
            }
          }
          setCurrentActionState('plan_step_update', nextStatus === 'failed' ? 'failed' : 'completed')
          break
        }
        case 'plan_complete': {
          if (session.runtimePlan) {
            session.runtimePlan.status = 'completed'
            session.runtimePlan.steps = session.runtimePlan.steps.map((step) => ({
              ...step,
              status: step.status === 'failed' ? step.status : 'completed',
            }))
            session.runtimePlan.updatedAt = new Date().toISOString()
            executionState.planCompletedSteps = session.runtimePlan.steps
              .map((step) => step.title.trim())
              .filter(Boolean)
          }
          executionState.currentMode = 'normal'
          executionState.awaiting = null
          executionState.planCurrentStep = null
          clearPendingPlanState()
          setCurrentActionState('plan_complete', 'completed')
          break
        }
        case 'plan_cancel': {
          if (session.runtimePlan) {
            session.runtimePlan.status = 'cancelled'
            session.runtimePlan.updatedAt = new Date().toISOString()
          }
          executionState.currentMode = 'normal'
          executionState.awaiting = null
          executionState.planCurrentStep = null
          clearPendingPlanState()
          setCurrentActionState('plan_cancel', 'completed')
          break
        }
        case 'write_start': {
          const mode = typeof args.mode === 'string' && args.mode.trim() ? args.mode.trim() : 'replace'
          const target = typeof args.target === 'string' && args.target.trim() ? args.target.trim() : 'full_doc'
          executionState.currentMode = session.runtimePlan?.steps?.length ? 'plan' : executionState.currentMode
          executionState.awaiting = null
          executionState.writeCompleted = false
          setCurrentActionState('write', 'running', { mode, target })
          if (props.docType === 'doc' && (mode === 'append' || mode === 'replace')) {
            routeAction = mode as StreamAction
          }
          break
        }
        case 'write_end': {
          const target = typeof args.target === 'string' && args.target.trim()
            ? args.target.trim()
            : executionState.currentActionTarget || 'full_doc'
          flushOpenWriteStream()
          executionState.writeCompleted = true
          if (roundDocumentWriteObserved || wroteDocument) {
            executionState.documentWriteObserved = true
            executionState.saveAttemptWithoutDocumentChange = false
            writeSatisfiedStepKey = currentPlanStepKey(executionState) || writeSatisfiedStepKey
          }
          setCurrentActionState('write', 'completed', {
            mode: executionState.currentActionMode,
            target,
          })
          break
        }
        default:
          break
      }

      if (control) {
        latestToolDrivenControl = control
        syncExecutionPlanProgress(control)
      }

      const explicitPendingPlan = Object.prototype.hasOwnProperty.call(args, 'pending_plan')
        ? coerceBooleanLike(args.pending_plan)
        : Object.prototype.hasOwnProperty.call(args, 'pendingPlan')
          ? coerceBooleanLike(args.pendingPlan)
          : null
      const pendingPlanText = (nextPlan ? runtimePlanToPlanText(nextPlan) : '').trim()
      if (explicitPendingPlan === true) {
        const nextPendingPlan = pendingPlanText || session.lastPlan || executionState.pendingPlan || pendingPlan || null
        executionState.confirmationRequired = true
        executionState.pendingPlan = nextPendingPlan
        executionState.pendingPlanUserReply = null
        executionState.planConfirmationDecision = null
        executionState.awaiting = 'user_input'
        session.pendingPlan = nextPendingPlan
      } else if (explicitPendingPlan === false || control?.awaiting !== 'user_input') {
        clearPendingPlanState()
      }

      if (control?.currentMode === 'plan' && control?.awaiting === 'user_input') {
        executionState.confirmationRequired = true
        executionState.awaiting = 'user_input'
      } else if (control?.awaiting === 'user_confirm_write') {
        executionState.confirmationRequired = false
        executionState.awaiting = 'user_confirm_write'
      } else if (control?.awaiting == null) {
        executionState.confirmationRequired = false
        executionState.awaiting = null
      }

      if (Object.prototype.hasOwnProperty.call(args, 'save_requested')) {
        executionState.saveRequested = coerceBooleanLike(args.save_requested) === true
      } else if (Object.prototype.hasOwnProperty.call(args, 'saveRequested')) {
        executionState.saveRequested = coerceBooleanLike(args.saveRequested) === true
      }
      if (Object.prototype.hasOwnProperty.call(args, 'write_completed')) {
        executionState.writeCompleted = coerceBooleanLike(args.write_completed) === true
      } else if (Object.prototype.hasOwnProperty.call(args, 'writeCompleted')) {
        executionState.writeCompleted = coerceBooleanLike(args.writeCompleted) === true
      }

      latestToolDrivenControl = buildControlFromExecutionState(executionState, session.taskAnalysis)

      hostStateUpdatedThisRound = true
      session.updatedAt = Date.now()
      sessions.value = [...sessions.value]

      return {
        call_id: call.call_id,
        name: call.name,
        arguments: call.arguments,
        output: {
          ok: true,
          tool: call.name,
          result: {
            state_updated: true,
            plan_updated: Boolean(nextPlan),
            current_mode: executionState.currentMode,
            awaiting: executionState.awaiting,
            current_action_kind: executionState.currentActionKind,
            current_action_status: executionState.currentActionStatus,
            pending_plan: executionState.pendingPlan,
            phase: latestToolDrivenControl?.phase || buildControlFromExecutionState(executionState, session.taskAnalysis).phase || null,
          },
        },
      }
    }

    const executeRoundToolCalls = async (calls: AgentToolCall[]) => {
      const hostStateCalls = calls.filter((call) => isHostStateTool(call.name))
      const externalCalls = calls.filter((call) => !isHostStateTool(call.name))
      const outputs: AgentToolOutputPayload[] = []

      for (const call of hostStateCalls) {
        outputs.push(await executeHostStateToolCall(call))
      }

      if (externalCalls.length) {
        outputs.push(...await executeAgentToolCalls(externalCalls))
      }

      return outputs
    }

    const submitFrontendToolResult = async (
      runId: string,
      callId: string,
      output: unknown,
    ) => {
      const token = localStorage.getItem('token')
      const response = await fetch('/api/agent/tool-callback', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
        body: JSON.stringify({
          run_id: runId,
          call_id: callId,
          output,
        }),
      })

      if (!response.ok) {
        const data = await response.json().catch(() => ({}))
        throw new Error(data.error || '前端工具结果回传失败')
      }
    }

    const executeFrontendToolRequest = async (eventData: any) => {
      const runId = typeof eventData?.run_id === 'string' && eventData.run_id.trim()
        ? eventData.run_id.trim()
        : currentAgentRunId
      const callId = typeof eventData?.call_id === 'string' && eventData.call_id.trim()
        ? eventData.call_id.trim()
        : ''
      const toolName = typeof eventData?.name === 'string' && eventData.name.trim()
        ? eventData.name.trim()
        : ''
      const args = eventData?.arguments && typeof eventData.arguments === 'object'
        ? eventData.arguments
        : {}

      if (!runId || !callId || !toolName) {
        return
      }

      let callbackOutput: unknown
      try {
        if (toolName === 'action_protocol_write') {
          const writerResult = readLatestWriterResult()
          const hasFreshWriteResult = Boolean(
            writerResult?.ok
            && props.docType === 'doc'
            && props.docId
            && writerResult.docId === props.docId
            && (roundDocumentWriteObserved || wroteDocument),
          )
          callbackOutput = hasFreshWriteResult
            ? {
                ok: true,
                tool: toolName,
                result: {
                  wrote_document: true,
                  write_completed: true,
                  mode: typeof args.mode === 'string' ? args.mode : null,
                  doc_id: props.docId ?? null,
                  doc_name: props.docName ?? null,
                },
              }
            : {
                ok: false,
                tool: toolName,
                error: writerResult?.reason || 'ACTION 写入结果不可用或当前文档未完成写入',
              }
        } else {
          const outputs = await executeAgentToolCalls([{
            call_id: callId,
            name: toolName,
            arguments: JSON.stringify(args),
          }])
          callbackOutput = outputs[0]?.output ?? {
            ok: false,
            tool: toolName,
            error: '工具没有返回结果',
          }
        }
      } catch (error: any) {
        callbackOutput = {
          ok: false,
          tool: toolName,
          error: error?.message || '前端工具执行失败',
        }
      }

      await submitFrontendToolResult(runId, callId, callbackOutput)
    }

    const applyLiveHostToolDelta = (eventData: any) => {
      const toolName = typeof eventData?.name === 'string' ? eventData.name.trim() : ''
      if (!toolName || !isHostStateTool(toolName)) return
      const args = parseToolArguments(typeof eventData?.arguments === 'string' ? eventData.arguments : null) || {}

      if (toolName === 'write_start') {
        const mode = typeof args.mode === 'string' && args.mode.trim() ? args.mode.trim() : 'replace'
        const target = typeof args.target === 'string' && args.target.trim() ? args.target.trim() : 'full_doc'
        executionState.currentMode = session.runtimePlan?.steps?.length ? 'plan' : executionState.currentMode
        executionState.awaiting = null
        executionState.writeCompleted = false
        setCurrentActionState('write', 'running', { mode, target })
        if (props.docType === 'doc' && (mode === 'append' || mode === 'replace')) {
          routeAction = mode as StreamAction
        }
      } else if (toolName === 'write_end') {
        const target = typeof args.target === 'string' && args.target.trim()
          ? args.target.trim()
          : executionState.currentActionTarget || 'full_doc'
        flushOpenWriteStream()
        setCurrentActionState('write', 'completed', {
          mode: executionState.currentActionMode,
          target,
        })
      }

      latestToolDrivenControl = buildControlFromExecutionState(executionState, session.taskAnalysis)
      hostStateUpdatedThisRound = true
    }

    syncPendingPlanPreviewFromStream = () => {
      if (latestStructuredResponse?.plan || latestStructuredResponse?.state || latestToolDrivenPlan || latestToolDrivenControl) {
        if (latestStructuredResponse?.plan) {
          session.runtimePlan = latestStructuredResponse.plan
          const planText = runtimePlanToPlanText(latestStructuredResponse.plan)
          if (planText) {
            session.lastPlan = planText
          }
        } else if (latestToolDrivenPlan) {
          session.runtimePlan = latestToolDrivenPlan
          const planText = runtimePlanToPlanText(latestToolDrivenPlan)
          if (planText) {
            session.lastPlan = planText
          }
        }
        return
      }
      const source = completedAssistantContent || rawAssistantContent
      const streamedControl = resolveEffectiveControl(source)
      const streamedPlan = extractPlanBlock(source)
      if (streamedPlan) {
        sawPlanSignalThisRound = true
      }
      const runtimePlanText = runtimePlanToPlanText(session.runtimePlan)
      const nextPlan = streamedPlan || runtimePlanText
      const requiresConfirmation = shouldTreatPlanAsPending({
        planText: nextPlan,
        control: streamedControl,
        session,
        executionState,
        sawPlanSignal: sawPlanSignalThisRound,
      })

      let changed = false

      if (nextPlan && session.lastPlan !== nextPlan) {
        session.lastPlan = nextPlan
        changed = true
      }

      if (!session.runtimePlan && nextPlan) {
        const fallbackRuntimePlan = buildRuntimePlanFromText(nextPlan, userMessage?.content || pendingPlan || '', session.runtimePlan)
        if (fallbackRuntimePlan) {
          session.runtimePlan = fallbackRuntimePlan
          changed = true
        }
      }

      if (requiresConfirmation && session.pendingPlan !== nextPlan) {
        session.pendingPlan = nextPlan
        changed = true
      }

      if (changed) {
        session.updatedAt = Date.now()
        sessions.value = [...sessions.value]
      }
    }

    const buildAssistantRoundSummary = (control: AgentControlBlock | null) => {
      const parts: string[] = []
      const actionWriteAlreadySummarized = roundToolCalls.some((call) => (
        call.name === 'action_protocol_write' && call.outcome === 'success'
      ))
      if (Number.isFinite(control?.planStepIndex) && Number.isFinite(control?.planTotalSteps) && control?.planCurrentStep) {
        parts.push(`当前计划进度：第 ${Number(control.planStepIndex)}/${Number(control.planTotalSteps)} 步，${control.planCurrentStep}。`)
      } else if (control?.planCurrentStep) {
        parts.push(`当前步骤：${control.planCurrentStep}。`)
      }
      if (roundDocumentWriteObserved && !actionWriteAlreadySummarized) {
        parts.push(props.docName ? `已写入《${props.docName}》正文。` : '本轮已写入文档内容。')
      }
      parts.push(...summarizeRoundActions(roundToolCalls))
      if (control?.awaiting === 'user_confirm_write') {
        parts.push('当前文档仍未保存，正在等待保存决策。')
      } else if (control?.currentMode === 'plan' && !control?.awaiting && hasRemainingStructuredPlanWork(session.runtimePlan, executionState, control)) {
        parts.push('系统将继续执行后续步骤。')
      }
      return parts.filter(Boolean).join('\n')
    }

    const buildPendingPlanConfirmationMessage = () => {
      const planText = (session.pendingPlan?.trim() || runtimePlanToPlanText(session.runtimePlan)).trim()
      if (!planText) return '我已经整理好执行计划，请确认后我再继续。'
      return [
        '我准备按这个计划继续：',
        '',
        planText,
        '',
        '请确认是否继续执行？',
      ].join('\n')
    }

    const mergeVisibleAssistantContent = (base: string, appendix: string) => {
      const normalizedBase = base.trim()
      const normalizedAppendix = appendix.trim()
      if (!normalizedAppendix) return normalizedBase
      if (!normalizedBase) return normalizedAppendix
      if (normalizedBase.includes(normalizedAppendix)) return normalizedBase
      return `${normalizedBase}\n\n${normalizedAppendix}`.trim()
    }

    const syncLiveAssistantDisplay = (baseVisible: string) => {
      liveAssistantContent.value = mergeVisibleAssistantContent(baseVisible, liveAssistantRoundSummary)
      assistantMessage.content = liveAssistantContent.value
      assistantMessage.reasoning = liveAssistantReasoning.value
      assistantMessage.internalStatus = false
      scrollMessagesToBottom()
    }

    const ensureVisibleAssistantRoundSummary = (control: AgentControlBlock | null) => {
      const fallback = buildAssistantRoundSummary(control)
      if (!fallback.trim()) return
      if (fallback !== liveAssistantRoundSummary) {
        runtimeSummaryAppendedThisRequest = true
        liveAssistantRoundSummary = fallback
      }
      const baseVisible = buildVisibleAssistantContent(
        completedAssistantContent || rawAssistantContent,
        !completedAssistantContent,
      )
      syncLiveAssistantDisplay(baseVisible)
    }

    appendFinalLoopResultMessage = (resultContent: string) => {
      const normalized = stripProtocolContent(resultContent).trim()
      if (!normalized) return
      if (!runtimeSummaryAppendedThisRequest && requestRound <= 1) return
      const currentVisibleBody = stripProtocolContent(
        assistantMessage.content
        || liveAssistantContent.value
        || buildVisibleAssistantContent(rawAssistantContent || completedAssistantContent || '')
        || '',
      ).trim()
      if (!latestStructuredResponse?.message?.trim() && currentVisibleBody && normalized === currentVisibleBody) {
        return
      }
      const currentRenderedContent = (assistantMessage.content || liveAssistantContent.value || '').trim()
      if (currentRenderedContent.includes(normalized)) {
        const cleanedCurrentContent = currentRenderedContent
          .replace(normalized, '')
          .replace(/\n{3,}/g, '\n\n')
          .trim()
        if (cleanedCurrentContent) {
          assistantMessage.content = cleanedCurrentContent
          liveAssistantContent.value = cleanedCurrentContent
        } else if (!assistantMessage.reasoning?.trim()) {
          session.messages = session.messages.filter((message) => message.id !== assistantMessage.id)
        } else {
          assistantMessage.content = ''
          liveAssistantContent.value = ''
        }
      }

      assistantMessage = {
        id: genId(),
        role: 'assistant',
        content: normalized,
        reasoning: '',
      }
      session.messages.push(assistantMessage)
      session.updatedAt = Date.now()
      sessions.value = [...sessions.value]
      scrollMessagesToBottom()
    }

    finalizeAssistantMessageForDisplay = (
      finalContent: string,
      options: { includeStructuredMessage?: boolean } = {},
    ) => {
      if (options.includeStructuredMessage === true && latestStructuredResponse?.message.trim()) {
        const baseVisible = mergeVisibleAssistantContent(
          buildVisibleAssistantContent(completedAssistantContent || rawAssistantContent),
          latestStructuredResponse.message.trim(),
        )
        syncLiveAssistantDisplay(baseVisible)
      }
      const control = resolveEffectiveControl(finalContent)
      const hideAsRuntimeMessage = Boolean(
        (roundToolCalls.length || roundDocumentWriteObserved || executionState.documentWriteObserved)
        && !(control?.currentMode === 'plan' && control?.awaiting === 'user_input')
        && hasRemainingStructuredPlanWork(session.runtimePlan, executionState, control),
      )
      void hideAsRuntimeMessage
      ensureVisibleAssistantRoundSummary(control)
    }

    const maybeAutoSaveAfterDocumentWrite = async (control: AgentControlBlock | null) => {
      if (streamFailed || streamAborted) return
      if (props.docType !== 'doc' || !props.docId) return
      if (!(wroteDocument || executionState.writeCompleted || roundDocumentWriteObserved)) return
      if (!currentStepRequiresDocumentSave(session.runtimePlan, executionState, control)) return
      if (!currentDocumentHasUnsavedChanges()) return
      if (saveToolSucceededThisTurn) return

      const autoSaveOutputs = await executeAgentToolCalls([{
        call_id: genId(),
        name: 'save_current_document',
        arguments: JSON.stringify({ doc_id: props.docId }),
      }])
      if (!autoSaveOutputs.length) return

      pendingToolOutputs = mergePendingPlanToolOutputs(pendingToolOutputs || [], autoSaveOutputs)
      collectedPlanToolOutputs = mergePendingPlanToolOutputs(collectedPlanToolOutputs, autoSaveOutputs)

      const autoSaveCalls = autoSaveOutputs.map((output) => ({
        call_id: output.call_id,
        name: output.name || 'save_current_document',
        arguments: output.arguments || JSON.stringify({ doc_id: props.docId }),
        stage_policy: 'mutation',
        capabilities: ['save'],
      }))
      const autoSaveRoundCalls = summarizeToolCallBatch(autoSaveCalls, autoSaveOutputs)
      if (autoSaveRoundCalls.length) {
        roundToolCalls = [...roundToolCalls, ...autoSaveRoundCalls]
        executionState.recentToolCalls = appendRecentToolCalls(
          executionState.recentToolCalls,
          autoSaveRoundCalls,
        )
        appendToolEventsToSession(session, autoSaveRoundCalls)
      }

      saveToolAttemptedThisTurn = true
      if (autoSaveRoundCalls.some((call) => call.outcome === 'success' || call.outcome === 'noop')) {
        saveToolSucceededThisTurn = true
        executionState.saveRequested = false
        if (!planStepAdvancedThisRound) {
          planStepAdvancedThisRound = advanceExecutionPlanStep(session.runtimePlan, executionState, control, {
            wroteDocument: roundDocumentWriteObserved || wroteDocument,
            savedDocument: true,
            mutationCompleted: hasSuccessfulMutationToolCall(autoSaveRoundCalls),
          })
        }
      }
    }

    const appendAssistantContent = (content: string) => {
      if (!content) return

      if (routeAction && routeAction !== 'chat' && props.docId) {
        if (!writerStarted) {
          const writerMode: AgentWriterMode = routeAction
          dispatchAgentWriterStart({ docId: props.docId, mode: writerMode, save: false })
          writerStarted = true
        }
        if (routeAction === 'append' || routeAction === 'replace') {
          wroteDocument = true
        }
        upsertArtifactDraft(session, content, {
          docId: props.docId,
          docName: props.docName,
        })
        dispatchAgentWriterChunk({ docId: props.docId, chunk: content })
        return
      }

      liveAssistantContent.value = `${liveAssistantContent.value}${content}`
      scrollMessagesToBottom()
    }

    const appendAssistantReasoning = (delta: string) => {
      if (!delta) return
      liveAssistantReasoning.value = `${liveAssistantReasoning.value}${delta}`
      scrollMessagesToBottom()
    }

    buildVisibleAssistantContent = (source = rawAssistantContent, streamMode = false) => {
      let visible = source

      if (streamMode) {
        const upperVisible = visible.toUpperCase()
        const openIndex = Math.max(
          ...AGENT_WRITE_ACTION_OPEN_MARKERS.map((item) => upperVisible.lastIndexOf(item.marker.toUpperCase())),
        )
        if (openIndex !== -1) {
          const closeIndex = upperVisible.lastIndexOf(AGENT_ACTION_CLOSE_MARKER)
          if (openIndex > closeIndex) {
            visible = visible.slice(0, openIndex)
          }
        }
      }

      return stripProtocolContent(visible)
    }

    appendUnsavedDraftNotice = (control: AgentControlBlock | null) => {
      if (!wroteDocument || streamAborted || streamFailed) return
      if (!currentDocumentHasUnsavedChanges()) return
      if (control?.awaiting === 'user_confirm_write') return
      const notice = '内容已写入当前文档草稿，尚未保存。是否现在保存？'
      const merged = mergeVisibleAssistantContent(liveAssistantRoundSummary, notice)
      if (merged !== liveAssistantRoundSummary) {
        liveAssistantRoundSummary = merged
        runtimeSummaryAppendedThisRequest = true
      }
      syncLiveAssistantDisplay(buildVisibleAssistantContent(completedAssistantContent || rawAssistantContent))
    }

    const appendCompletedTail = (completedContent: string) => {
      if (!completedContent) return
      if (!rawAssistantContent) {
        rawAssistantContent = completedContent
        handleAssistantChunk(completedContent)
        return
      }
      if (completedContent === rawAssistantContent) return

      if (completedContent.startsWith(rawAssistantContent)) {
        const tail = completedContent.slice(rawAssistantContent.length)
        if (!tail) return
        rawAssistantContent += tail
        handleAssistantChunk(tail)
        return
      }

      // Some providers only give a partial delta stream and then a final full
      // message. Rebuild the visible chat content from the final message and
      // only replay the unseen suffix into the editor/chat router.
      const overlapLength = (() => {
        const max = Math.min(rawAssistantContent.length, completedContent.length)
        for (let length = max; length > 0; length -= 1) {
          if (rawAssistantContent.endsWith(completedContent.slice(0, length))) {
            return length
          }
        }
        return 0
      })()
      const tail = completedContent.slice(overlapLength)
      const actionCloseMarker = AGENT_ACTION_CLOSE_MARKER
      const existingCloseIndex = rawAssistantContent.toUpperCase().indexOf(actionCloseMarker)
      const completedHasActionBlock = ACTION_OPEN_REGEX.test(completedContent)
      const mergedCompletedContent = (() => {
        if (!completedHasActionBlock) return completedContent
        if (existingCloseIndex === -1) return completedContent
        const existingTail = rawAssistantContent.slice(existingCloseIndex + actionCloseMarker.length)
        if (!existingTail.trim()) return completedContent
        return completedContent.includes(existingTail)
          ? completedContent
          : `${completedContent}${existingTail}`
      })()
      rawAssistantContent = mergedCompletedContent
      if (tail) {
        handleAssistantChunk(tail)
      }
    }

    recoverTrailingActionMarker = () => {
      if (routeAction && routeAction !== 'chat') return
      if (props.docType !== 'doc' || !props.docId) return

      const wrappedMatch = rawAssistantContent.match(ACTION_WRAPPED_REGEX)
      if (wrappedMatch) {
        const mode = wrappedMatch[1].toLowerCase() as AgentWriterMode
        const body = wrappedMatch[2] || ''
        liveAssistantContent.value = ''
        routeAction = mode
        dispatchAgentWriterStart({ docId: props.docId, mode, save: false })
        writerStarted = true
        wroteDocument = true
        if (body) {
          dispatchAgentWriterChunk({ docId: props.docId, chunk: body })
        }
      }
    }

    const flushAssistantRenderBuffer = (force = false) => {
      const openTag = '<think>'
      const closeTag = '</think>'

      while (renderBuffer) {
        if (inThinkBlock) {
          const closeIndex = renderBuffer.indexOf(closeTag)
          if (closeIndex !== -1) {
            appendAssistantReasoning(renderBuffer.slice(0, closeIndex))
            renderBuffer = renderBuffer.slice(closeIndex + closeTag.length)
            inThinkBlock = false
            continue
          }

          const safeLength = force ? renderBuffer.length : Math.max(0, renderBuffer.length - closeTag.length + 1)
          if (!safeLength) return
          appendAssistantReasoning(renderBuffer.slice(0, safeLength))
          renderBuffer = renderBuffer.slice(safeLength)
          return
        }

        const openIndex = renderBuffer.indexOf(openTag)
        if (openIndex !== -1) {
          appendAssistantContent(renderBuffer.slice(0, openIndex))
          renderBuffer = renderBuffer.slice(openIndex + openTag.length)
          inThinkBlock = true
          continue
        }

        const safeLength = force ? renderBuffer.length : Math.max(0, renderBuffer.length - openTag.length + 1)
        if (!safeLength) return
        appendAssistantContent(renderBuffer.slice(0, safeLength))
        renderBuffer = renderBuffer.slice(safeLength)
        return
      }
    }

    consumeAssistantText = (rawChunk: string, force = false) => {
      if (rawChunk) {
        renderBuffer += rawChunk
      }
      flushAssistantRenderBuffer(force)
    }

    const actionCloseMarker = AGENT_ACTION_CLOSE_MARKER
    const actionOpenMarkers = AGENT_WRITE_ACTION_OPEN_MARKERS
    const actionMarkerLookbehind = Math.max(
      actionCloseMarker.length,
      ...actionOpenMarkers.map((item) => item.marker.length),
    )
    let actionProbe = ''

    const hasIncompleteActionWrite = () => (
      Boolean(routeAction && routeAction !== 'chat')
      && ACTION_OPEN_REGEX.test(rawAssistantContent)
      && !/\[\[\/ACTION\]\]/i.test(rawAssistantContent)
    )

    const completeWriterBlock = () => {
      if (routeAction && routeAction !== 'chat' && props.docId && writerStarted) {
        lastWriterResult = null
        dispatchAgentWriterComplete({ docId: props.docId })
        const writerResult = readLatestWriterResult()
        if (!writerResult?.ok) {
          throw new Error(writerResult?.reason || '正文协议已输出，但编辑器没有成功应用本次写入。')
        }
        wroteDocument = true
        executionState.documentWriteObserved = true
        executionState.saveAttemptWithoutDocumentChange = false
        executionState.writeCompleted = true
        writeSatisfiedStepKey = currentPlanStepKey(executionState) || writeSatisfiedStepKey
        roundDocumentWriteObserved = true
        resetSemanticContinuationBudget()
        upsertArtifactDraft(session, '', {
          finalize: true,
          docId: props.docId,
          docName: props.docName,
        })
        writerStarted = false
      }
      routeAction = 'chat'
    }

    const resetStreamingRoundState = () => {
      routeAction = null
      prefixProbe = ''
      pendingRoute = null
      writerStarted = false
      lastWriterResult = null
      renderBuffer = ''
      inThinkBlock = false
      rawAssistantContent = ''
      completedAssistantContent = ''
      sawPlanSignalThisRound = false
      actionProbe = ''
      roundDocumentWriteObserved = false
      roundToolCalls = []
      liveAssistantRoundSummary = ''
      liveAssistantContent.value = ''
      liveAssistantReasoning.value = ''
    }

    const startContinuationAssistantMessage = (finalContent: string) => {
      finalizeAssistantMessageForDisplay(finalContent)

      assistantMessage = {
        id: genId(),
        role: 'assistant',
        content: '',
        reasoning: '',
      }
      session.messages.push(assistantMessage)
      session.updatedAt = Date.now()
      sessions.value = [...sessions.value]
      streamingAssistantId.value = assistantMessage.id
      resetStreamingRoundState()
      scrollMessagesToBottom()
    }

    const startFollowupAssistantMessageForCompleted = (finalContent: string) => {
      finalizeAssistantMessageForDisplay(finalContent)

      const currentContent = (assistantMessage.content || liveAssistantContent.value || '').trim()
      const currentReasoning = (assistantMessage.reasoning || liveAssistantReasoning.value || '').trim()
      if (!currentContent && !currentReasoning) {
        return
      }

      assistantMessage = {
        id: genId(),
        role: 'assistant',
        content: '',
        reasoning: '',
      }
      session.messages.push(assistantMessage)
      session.updatedAt = Date.now()
      sessions.value = [...sessions.value]
      streamingAssistantId.value = assistantMessage.id
      resetStreamingRoundState()
      scrollMessagesToBottom()
    }

    const findNextActionOpen = (input: string) => {
      const upperInput = input.toUpperCase()
      const normalizedOpenMarkers = actionOpenMarkers.map((item) => ({
        ...item,
        upperMarker: item.marker.toUpperCase(),
      }))
      let best: { index: number; marker: string; mode: AgentWriterMode } | null = null
      for (const item of normalizedOpenMarkers) {
        const index = upperInput.indexOf(item.upperMarker)
        if (index === -1) continue
        if (!best || index < best.index) {
          best = { index, marker: item.upperMarker, mode: item.mode }
        }
      }
      return best
    }

    routeChunk = (rawChunk: string, force = false) => {
      if (rawChunk) {
        actionProbe += rawChunk
      }

      while (actionProbe) {
        if (routeAction && routeAction !== 'chat') {
          const closeIndex = actionProbe.toUpperCase().indexOf(actionCloseMarker)
          if (closeIndex !== -1) {
            const beforeClose = actionProbe.slice(0, closeIndex)
            if (beforeClose) {
              consumeAssistantText(beforeClose, true)
            }
            actionProbe = actionProbe.slice(closeIndex + actionCloseMarker.length)
            completeWriterBlock()
            continue
          }

          const safeLength = force
            ? actionProbe.length
            : Math.max(0, actionProbe.length - actionCloseMarker.length + 1)
          if (!safeLength) return

          consumeAssistantText(actionProbe.slice(0, safeLength), force)
          actionProbe = actionProbe.slice(safeLength)
          return
        }

        const nextOpen = findNextActionOpen(actionProbe)
        if (nextOpen) {
          const beforeOpen = actionProbe.slice(0, nextOpen.index)
          if (beforeOpen) {
            routeAction = 'chat'
            consumeAssistantText(beforeOpen, true)
          }
          actionProbe = actionProbe.slice(nextOpen.index + nextOpen.marker.length)
          routeAction = props.docType === 'doc' ? nextOpen.mode : 'chat'
          continue
        }

        const safeLength = force
          ? actionProbe.length
          : Math.max(0, actionProbe.length - actionMarkerLookbehind + 1)
        if (!safeLength) return

        routeAction = 'chat'
        consumeAssistantText(actionProbe.slice(0, safeLength), force)
        actionProbe = actionProbe.slice(safeLength)
        return
      }
    }

    handleAssistantChunk = (rawChunk: string) => {
      if (!rawChunk) return
      if (routeAction) {
        routeChunk(rawChunk)
        return
      }

      prefixProbe += rawChunk
      const trimmedProbe = prefixProbe.trimStart()
      if (!trimmedProbe) return
      if (trimmedProbe !== prefixProbe) {
        prefixProbe = trimmedProbe
      }

      if (prefixProbe === '[') {
        return
      }

      if (!prefixProbe.startsWith('[[')) {
        routeAction = 'chat'
        const flushed = prefixProbe
        prefixProbe = ''
        routeChunk(flushed)
        return
      }

      while (prefixProbe.startsWith('[[')) {
        const markerEnd = prefixProbe.indexOf(']]')
        if (markerEnd === -1) {
          if (prefixProbe.length > 120) {
            routeAction = 'chat'
            const flushed = prefixProbe
            prefixProbe = ''
            routeChunk(flushed)
          }
          return
        }

        const marker = prefixProbe.slice(0, markerEnd + 2)
        const rest = prefixProbe.slice(markerEnd + 2)

        const upperMarker = marker.toUpperCase()

        if (upperMarker.startsWith('[[ROUTE:')) {
          const target = parseRouteMarker(marker)
          if (target) {
            pendingRoute = target
            prefixProbe = rest.replace(/^\s+/, '')
            continue
          }
          routeAction = 'chat'
          const flushed = prefixProbe
          prefixProbe = ''
          routeChunk(flushed)
          return
        }

        if (upperMarker.startsWith('[[ACTION:')) {
          prefixProbe = ''
          routeAction = resolveAgentWriteMode(upperMarker) || 'chat'

          if (routeAction === 'replace' && isPartialWriteTask(session.taskAnalysis)) {
            throw new Error('当前任务是局部编辑，不能使用 ACTION:replace 整篇覆盖。请改用 rewrite_document_section、replace_document_block、replace_document_blocks、swap_document_sections，或在必要时使用 ACTION:append。')
          }

          if (routeAction !== 'chat' && props.docType !== 'doc') {
            routeAction = 'chat'
          }

          if (pendingRoute) {
            emit('navigate', pendingRoute)
            pendingRoute = null
          }

          routeChunk(rest.replace(/^\s+/, ''))
          return
        }

        routeAction = 'chat'
        const flushed = prefixProbe
        prefixProbe = ''
        routeChunk(flushed)
        return
      }

      if (pendingRoute) {
        emit('navigate', pendingRoute)
        pendingRoute = null
      }

      routeAction = 'chat'
      const flushed = prefixProbe
      prefixProbe = ''
      routeChunk(flushed)
    }

    finalizePendingAssistantOutput = (options: { allowIncompleteAction?: boolean } = {}) => {
      if (!routeAction && prefixProbe.trim()) {
        const flushed = prefixProbe
        prefixProbe = ''
        handleAssistantChunk(flushed)
      }
      routeChunk('', true)
      consumeAssistantText('', true)
      recoverTrailingActionMarker()
      if (routeAction && routeAction !== 'chat') {
        if (!/\[\[\/ACTION\]\]/i.test(rawAssistantContent)) {
          if (options.allowIncompleteAction) {
            return
          }
          throw new Error('模型在正文动作未完整闭合时请求了后续工具，已中止执行')
        }
        completeWriterBlock()
      }
      const finalizedSource = completedAssistantContent || rawAssistantContent
      liveAssistantContent.value = buildVisibleAssistantContent(finalizedSource)
    }

    while (true) {
      requestRound += 1
      planStepAdvancedThisRound = false
      const progressSignatureBeforeRound = buildExecutionProgressSignature(executionState, finalControl)
      assistantMessage.content = liveAssistantContent.value
      assistantMessage.reasoning = liveAssistantReasoning.value
      const requestMessages = buildConversationMessages(session.messages, {
        preserveFullHistory: Boolean(
          requestRound > 1
          || session.pendingPlan
          || session.runtimePlan?.status === 'running'
          || session.runtimePlan?.status === 'pending',
        ),
      })
      const requestBody = buildRequestBody(requestMessages, provider, session.model.trim(), {
        transportMode: selectedTransportMode,
        previousResponseId,
        toolOutputs: pendingToolOutputs || (pendingPlan ? carriedPendingPlanToolOutputs : null),
        agentExecution: executionState,
        lastExecutionMemory: session.lastExecutionMemory,
        sessionMemory: session.sessionMemory,
      })
      const roundEvents: Array<Record<string, unknown>> = []
      const roundLabel = `session=${session.id} round=${requestRound}`
      const token = localStorage.getItem('token')
      logAgentDebugGroup(`${roundLabel} request`, {
        providerId: provider.id,
        providerName: provider.name,
        model: session.model,
        previousResponseId,
        executionSnapshotBeforeRound: buildDebugExecutionSnapshot(session, executionState, finalControl),
        requestBody,
      })
      const response = await fetch('/api/agent/chat/stream', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
        signal: abortController.signal,
        body: JSON.stringify(requestBody),
      })

      pendingToolOutputs = null

      if (!response.ok) {
        const data = await response.json().catch(() => ({}))
        throw new Error(data.error || '智能体请求失败')
      }

      const reader = response.body?.getReader()
      if (!reader) throw new Error('流式响应不可用')

      const decoder = new TextDecoder('utf-8')
      let buffer = ''
      let cycleReceivedDelta = false
      let requiredToolCalls: AgentToolCall[] = []
      let toolResponseId = ''
      let streamDone = false

      const processParsedEvent = (parsed: ReturnType<typeof parseSseBlock>) => {
        if (!parsed) return
        roundEvents.push(summarizeAgentEventForDebug(parsed.event, parsed.data))

        if (parsed.event === 'message.started') {
          currentAgentRunId = typeof parsed.data.run_id === 'string' && parsed.data.run_id.trim()
            ? parsed.data.run_id.trim()
            : currentAgentRunId
        } else if (parsed.event === 'message.delta') {
          cycleReceivedDelta = true
          const content = parsed.data.content || ''
          rawAssistantContent += content
          handleAssistantChunk(content)
          syncLiveAssistantDisplay(buildVisibleAssistantContent(rawAssistantContent, true))
          syncPendingPlanPreviewFromStream()
        } else if (parsed.event === 'tool.call.delta') {
          applyLiveHostToolDelta(parsed.data)
        } else if (parsed.event === 'structured_response') {
          const normalized = normalizeStructuredResponse(parsed.data)
          if (normalized) {
            latestStructuredResponse = normalized
            if (normalized.plan) {
              session.runtimePlan = normalized.plan
              const planText = runtimePlanToPlanText(normalized.plan)
              if (planText) {
                session.lastPlan = planText
              }
              sawPlanSignalThisRound = true
            }
            syncPendingPlanPreviewFromStream()
          }
        } else if (parsed.event === 'task_analysis') {
          const normalized = normalizeTaskAnalysis(parsed.data)
          if (normalized) {
            session.taskAnalysis = normalized
            executionState.confirmationRequired = normalized.requiresUserConfirmation === true
            syncPendingPlanPreviewFromStream()
          }
        } else if (parsed.event === 'plan_event') {
          const normalized = normalizeRuntimePlan(parsed.data.plan)
          if (normalized) {
            session.runtimePlan = normalized
            sawPlanSignalThisRound = true
            syncPendingPlanPreviewFromStream()
          }
        } else if (parsed.event === 'tool_event') {
          const normalized = normalizeToolEvent(parsed.data)
          if (normalized) {
            session.toolEvents = [...session.toolEvents, normalized].slice(-20)
            const roundCallSummary = buildRoundToolCallSummaryFromToolEvent(parsed.data)
            if (roundCallSummary) {
              roundToolCalls = [...roundToolCalls, roundCallSummary]
              executionState.recentToolCalls = appendRecentToolCalls(
                executionState.recentToolCalls,
                [roundCallSummary],
              )
              if (roundCallSummary.name === 'action_protocol_write' && roundCallSummary.outcome === 'success') {
                executionState.documentWriteObserved = true
                executionState.writeCompleted = true
                executionState.saveAttemptWithoutDocumentChange = false
                roundDocumentWriteObserved = true
              }
              ensureVisibleAssistantRoundSummary(resolveEffectiveControl(completedAssistantContent || rawAssistantContent))
            }
          }
        } else if (parsed.event === 'tool.request') {
          const requestPromise = executeFrontendToolRequest(parsed.data).catch((error) => {
            console.error('frontend tool request failed', error)
          })
          pendingFrontendToolRequests.push(requestPromise)
        } else if (parsed.event === 'artifact_delta') {
          if (typeof parsed.data.delta === 'string' && parsed.data.delta) {
            upsertArtifactDraft(session, parsed.data.delta, {
              docId: props.docId,
              docName: props.docName,
            })
          }
        } else if (parsed.event === 'artifact_done') {
          upsertArtifactDraft(session, '', {
            finalize: true,
            docId: props.docId,
            docName: props.docName,
          })
        } else if (parsed.event === 'reasoning.delta') {
          const delta = parsed.data.delta || parsed.data.content || ''
          if (delta) {
            appendAssistantReasoning(delta)
          }
        } else if (parsed.event === 'message.completed') {
          const completedContent = parsed.data.content || ''
          if (completedContent) {
            completedAssistantContent = completedContent
          }
          const responseId = typeof parsed.data.response_id === 'string' && parsed.data.response_id.trim()
            ? parsed.data.response_id.trim()
            : null
          if (responseId) {
            previousResponseId = responseId
          }
          const normalizedCompleted = stripProtocolContent(completedContent).trim()
          const shouldDeferCompletedIntoFinalBlock = Boolean(
            normalizedCompleted
            && runtimeSummaryAppendedThisRequest
            && (roundToolCalls.length || roundDocumentWriteObserved || executionState.documentWriteObserved)
          )
          if (!shouldDeferCompletedIntoFinalBlock) {
            appendCompletedTail(completedContent)
            const completedVisibleContent = buildVisibleAssistantContent(completedAssistantContent || rawAssistantContent)
            syncLiveAssistantDisplay(completedVisibleContent)
          } else {
            const currentVisibleContent = buildVisibleAssistantContent(rawAssistantContent)
            syncLiveAssistantDisplay(currentVisibleContent)
          }
          syncPendingPlanPreviewFromStream()
          streamDone = true
        } else if (parsed.event === 'agent.transport') {
          const mode = parsed.data.mode === 'chat_fallback'
            ? 'chat_fallback'
            : parsed.data.mode === 'chat'
              ? 'chat'
              : 'responses'
          agentTransportMode.value = mode
        } else if (parsed.event === 'agent.debug.model_request') {
          logAgentModelIo(
            `${roundLabel} model_request`,
            parsed.data && typeof parsed.data === 'object'
              ? parsed.data as Record<string, unknown>
              : { data: parsed.data },
          )
        } else if (parsed.event === 'agent.debug.model_response') {
          logAgentModelIo(
            `${roundLabel} model_response`,
            parsed.data && typeof parsed.data === 'object'
              ? parsed.data as Record<string, unknown>
              : { data: parsed.data },
          )
        } else if (parsed.event === 'agent.debug.model_error') {
          logAgentModelIo(
            `${roundLabel} model_error`,
            parsed.data && typeof parsed.data === 'object'
              ? parsed.data as Record<string, unknown>
              : { data: parsed.data },
          )
        } else if (parsed.event === 'tool.calls.required') {
          const completedContent = typeof parsed.data.content === 'string' ? parsed.data.content : ''
          if (completedContent) {
            completedAssistantContent = completedContent
            appendCompletedTail(completedContent)
          }
          finalizePendingAssistantOutput()
          syncPendingPlanPreviewFromStream()
          const confirmationPlan = (
            runtimePlanToPlanText(session.runtimePlan)
            || session.lastPlan?.trim()
            || session.pendingPlan?.trim()
            || ''
          ).trim()
          const confirmationControl = resolveEffectiveControl(completedAssistantContent || rawAssistantContent)
          const confirmationRequired = currentPlanNeedsConfirmation.value || shouldTreatPlanAsPending({
            planText: confirmationPlan,
            control: confirmationControl,
            session,
            executionState,
            sawPlanSignal: sawPlanSignalThisRound || Boolean(confirmationPlan),
          })
          if (confirmationRequired && confirmationPlan) {
            session.pendingPlan = confirmationPlan
            session.lastPlan = confirmationPlan
            session.updatedAt = Date.now()
            sessions.value = [...sessions.value]
          }
          toolResponseId = typeof parsed.data.response_id === 'string' ? parsed.data.response_id : ''
          if (toolResponseId.trim()) {
            previousResponseId = toolResponseId.trim()
          }
          requiredToolCalls = Array.isArray(parsed.data.calls) ? parsed.data.calls : []
          if (requiredToolCalls.length) {
            startContinuationAssistantMessage(completedContent || rawAssistantContent)
          }
        } else if (parsed.event === 'done') {
          streamDone = true
        } else if (parsed.event === 'error') {
          throw new Error(parsed.data.error || '智能体流式请求失败')
        }
      }

      while (true) {
        const { value, done } = await reader.read()
        if (done) {
          break
        }

        buffer += decoder.decode(value, { stream: true })
        const blocks = buffer.split(/\r?\n\r?\n/)
        buffer = blocks.pop() || ''

        for (const block of blocks) {
          processParsedEvent(parseSseBlock(block))
          if (streamDone) break
        }

        if (streamDone) {
          break
        }
      }

      if (buffer.trim()) {
        processParsedEvent(parseSseBlock(buffer))
      }

      if (pendingFrontendToolRequests.length) {
        await Promise.allSettled(pendingFrontendToolRequests.splice(0))
      }

      if (!requiredToolCalls.length) {
        finalizePendingAssistantOutput({ allowIncompleteAction: true })
        const finalContent = completedAssistantContent || rawAssistantContent
        if (hasIncompleteActionWrite()) {
          executionState.lastInterceptCode = 'action_block_not_closed'
          executionState.lastInterceptMessage = '模型输出的 ACTION 块未完整闭合'
          executionState.lastInterceptGuidance = 'Continue the current ACTION block from exactly where it stopped. Output only the remaining markdown body, then close with [[/ACTION]]. Do not repeat the earlier content, do not start a new ACTION block, and do not request any tools before closing it.'
          enqueueSemanticContinuation(finalContent)
          startContinuationAssistantMessage(finalContent)
          continue
        }
        const control = resolveEffectiveControl(finalContent)
        syncExecutionPlanProgress(control)
        const currentStepKey = currentPlanStepKey(executionState, control)
        const currentStepRequiresWrite = currentStepRequiresDocumentWrite(
          session.runtimePlan,
          executionState,
          control,
        )
        const awaitingPlanConfirmation = isAwaitingPlanConfirmation(executionState, control)
        const writeAlreadySatisfied = Boolean(currentStepKey && writeSatisfiedStepKey === currentStepKey)
        if (
          currentStepRequiresWrite
          && !awaitingPlanConfirmation
          && !writeAlreadySatisfied
          && !roundDocumentWriteObserved
          && !ACTION_OPEN_REGEX.test(finalContent)
        ) {
          throw new Error('当前步骤要求执行正文修改，但本轮没有完成有效的正文写入。请先调用 rewrite_document_section / replace_document_block / replace_document_blocks / swap_document_sections，或在空文档/文末写入时产出 ACTION:append，在整篇重写时产出 ACTION:replace。')
        }
        if (!planStepAdvancedThisRound) {
          planStepAdvancedThisRound = advanceExecutionPlanStep(session.runtimePlan, executionState, control, {
            wroteDocument: roundDocumentWriteObserved || wroteDocument,
            savedDocument: saveToolSucceededThisTurn,
            mutationCompleted: hasSuccessfulMutationToolCall(roundToolCalls),
          })
        }
        const shouldContinue = shouldTriggerSemanticContinuation(finalContent, control)
        const progressSignatureAfterRound = buildExecutionProgressSignature(executionState, control)
        const roundMadeProgress = Boolean(
          planStepAdvancedThisRound
          || roundDocumentWriteObserved
          || saveToolSucceededThisTurn
          || progressSignatureAfterRound !== progressSignatureBeforeRound,
        )
        if (shouldContinue && !roundMadeProgress) {
          idleSemanticContinuationRounds += 1
        } else {
          idleSemanticContinuationRounds = 0
        }
        if (shouldContinue && idleSemanticContinuationRounds >= 3) {
          throw new Error('计划执行空转：连续多轮没有新的工具结果、正文写入或步骤推进。请重新规划当前步骤后再继续。')
        }
        logAgentDebugGroup(`${roundLabel} response`, {
          events: roundEvents,
          finalContent,
          finalControl: control,
          roundToolCalls,
          roundDocumentWriteObserved,
          saveToolAttemptedThisTurn,
          saveToolSucceededThisTurn,
          roundMadeProgress,
          idleSemanticContinuationRounds,
          progressSignatureBeforeRound,
          progressSignatureAfterRound,
          shouldContinue,
          breakReason: shouldContinue ? 'semantic_continuation' : 'model_finished_without_tool_calls',
          executionSnapshotAfterRound: buildDebugExecutionSnapshot(session, executionState, control),
        })
        if (shouldContinue) {
          enqueueSemanticContinuation(finalContent)
          startContinuationAssistantMessage(finalContent)
          continue
        }
        break
      }

      toolCallRounds += 1
      if (toolCallRounds > MAX_TOOL_CALL_ROUNDS) {
        throw new Error('工具调用轮次过多，已停止本次生成，请补充更明确的目标或范围')
      }

      const toolSignature = requiredToolCalls
        .map((call) => `${call.name}:${(call.arguments || '').trim()}`)
        .sort()
        .join('||')
      if (toolSignature) {
        const hitCount = (toolCallSignatureHits.get(toolSignature) || 0) + 1
        toolCallSignatureHits.set(toolSignature, hitCount)
        if (!cycleReceivedDelta && hitCount > MAX_REPEAT_TOOL_SIGNATURE_HITS) {
          throw new Error('检测到重复工具调用循环，已停止本次生成，请调整指令后重试')
        }
      }

      if (!pendingToolOutputs) {
        pendingToolOutputs = await executeRoundToolCalls(requiredToolCalls)
      }
      logAgentDebugGroup(`${roundLabel} tool_calls`, {
        requiredToolCalls,
        toolOutputs: pendingToolOutputs,
        executionSnapshotAfterTools: buildDebugExecutionSnapshot(session, executionState, finalControl),
      })
      const toolDrivenControl: AgentControlBlock | null = latestToolDrivenControl as AgentControlBlock | null
      const structuredResponseForHostBatch: AgentStructuredResponse | null = latestStructuredResponse as AgentStructuredResponse | null
      const hostOnlyBatch = requiredToolCalls.length > 0 && requiredToolCalls.every((call) => isHostStateTool(call.name))
      const awaitingConfirmationAfterHostSync = hostOnlyBatch
        && toolDrivenControl?.currentMode === 'plan'
        && toolDrivenControl?.awaiting === 'user_input'
        && Boolean(session.pendingPlan?.trim() || session.runtimePlan?.steps?.length)
      if (awaitingConfirmationAfterHostSync) {
        liveAssistantContent.value = structuredResponseForHostBatch?.message?.trim()
          || buildPendingPlanConfirmationMessage()
        pendingToolOutputs = null
        break
      }
      if (pendingToolOutputs.length) {
        resetSemanticContinuationBudget()
        collectedPlanToolOutputs = mergePendingPlanToolOutputs(collectedPlanToolOutputs, pendingToolOutputs)
      }
      roundToolCalls = summarizeToolCallBatch(requiredToolCalls, pendingToolOutputs)
      const saveCallOutcomes = roundToolCalls.filter((call) => call.name === 'save_current_document')
      if (saveCallOutcomes.length) {
        saveToolAttemptedThisTurn = true
        if (saveCallOutcomes.some((call) => call.outcome === 'success' || call.outcome === 'noop')) {
          saveToolSucceededThisTurn = true
          executionState.saveRequested = false
        }
      }
      executionState.recentToolCalls = appendRecentToolCalls(
        executionState.recentToolCalls,
        roundToolCalls,
      )
      if (roundToolCalls.some((call) => isDocumentMutationTool(call.name) && call.outcome === 'success')) {
        executionState.documentWriteObserved = true
        executionState.saveAttemptWithoutDocumentChange = false
        executionState.writeCompleted = true
        writeSatisfiedStepKey = currentPlanStepKey(executionState) || writeSatisfiedStepKey
        roundDocumentWriteObserved = true
      }
      appendToolEventsToSession(session, roundToolCalls)
      const saveNoopDetected = !wroteDocument && extractSaveNoopState(pendingToolOutputs)
      if (saveNoopDetected) {
        executionState.saveAttemptWithoutDocumentChange = true
      }
      const controlAfterToolRound = resolveEffectiveControl(completedAssistantContent || rawAssistantContent)
      await maybeAutoSaveAfterDocumentWrite(controlAfterToolRound)
      ensureVisibleAssistantRoundSummary(resolveEffectiveControl(completedAssistantContent || rawAssistantContent))
      if (
        pendingPlan
        && !executionState.documentWriteObserved
        && isReadOrSaveOnlyBatch(roundToolCalls)
      ) {
        nonWritingPlanRounds += 1
      } else {
        nonWritingPlanRounds = 0
      }
      if (saveNoopDetected && nonWritingPlanRounds >= 3) {
        throw new Error('计划执行停滞：连续多轮只读取或保存当前文档，但没有真正写入正文。请重新规划当前步骤后再继续执行。')
      }
      if (runtimeSummaryAppendedThisRequest || liveAssistantRoundSummary.trim()) {
        startFollowupAssistantMessageForCompleted(completedAssistantContent || rawAssistantContent)
      }
    }
  } catch (error: any) {
    if (error?.name === 'AbortError') {
      streamAborted = true
      ElMessage.info('已停止生成')
      logAgentDebugGroup(`session=${session.id} aborted`, {
        previousResponseId,
        liveAssistantContent: liveAssistantContent.value,
        liveAssistantReasoning: liveAssistantReasoning.value,
      })
    } else {
      streamFailed = true
      const message = error?.message || '智能体请求失败'
      const intercepted = isAgentInternalInterceptError(message)
      interceptedFailure = intercepted ? resolveAgentInterceptDetails(message) : null
      if (interceptedFailure) {
        executionState.lastInterceptCode = interceptedFailure.code
        executionState.lastInterceptMessage = message
        executionState.lastInterceptGuidance = interceptedFailure.modelGuidance
      }
      logAgentPanelError('send_message', error, {
        sessionId: session.id,
        providerId: provider.id,
        model: session.model,
        docId: props.docId,
        docName: props.docName,
        intercepted,
      })
      logAgentDebugGroup(`session=${session.id} error`, {
        error,
        intercepted,
        previousResponseId,
        liveAssistantContent: liveAssistantContent.value,
        liveAssistantReasoning: liveAssistantReasoning.value,
        executionState,
        runtimePlan: session.runtimePlan,
        executionSnapshotAtError: buildDebugExecutionSnapshot(session, executionState, finalControl),
      })
      if (!intercepted) {
        ElMessage.error(message)
      }
    }
  } finally {
    window.removeEventListener(AGENT_WRITER_RESULT_EVENT, handleWriterResult as EventListener)
    activeStreamController = null
    if (!streamFailed) {
      try {
        finalizePendingAssistantOutput()
      } catch (finalizeError) {
        console.error('agent stream finalize failed', finalizeError)
      }
    }
    const finalAssistantContent = completedAssistantContent || rawAssistantContent || assistantMessage.content || liveAssistantContent.value
    const structuredResponse: AgentStructuredResponse | null = latestStructuredResponse as AgentStructuredResponse | null
    const effectiveStructuredPlan = structuredResponse?.plan || latestToolDrivenPlan
    const structuredPlanText = effectiveStructuredPlan
      ? runtimePlanToPlanText(effectiveStructuredPlan)
      : ''
    const extractedPlan = structuredPlanText || extractPlanBlock(finalAssistantContent)
    if (extractedPlan) {
      session.lastPlan = extractedPlan
      session.runtimePlan = effectiveStructuredPlan || buildRuntimePlanFromText(
        extractedPlan,
        userMessage?.content || pendingPlan || '',
        session.runtimePlan,
      ) || session.runtimePlan
    } else if (pendingPlan && !session.lastPlan) {
      session.lastPlan = pendingPlan
    }
    finalControl = structuredResponse?.state
      || latestToolDrivenControl
      || (
        (
          hostStateUpdatedThisRound
          || session.pendingPlan?.trim()
          || session.runtimePlan?.steps?.length
          || executionState.confirmationRequired
          || executionState.pendingPlan
          || executionState.planCurrentStep
          || executionState.planStepIndex
          || executionState.planCompletedSteps.length
          || executionState.saveRequested
          || executionState.writeCompleted
          || executionState.documentWriteObserved
          || roundToolCalls.length
          || roundDocumentWriteObserved
          || wroteDocument
        )
          ? buildControlFromExecutionState(executionState, session.taskAnalysis)
          : null
      )
    syncExecutionPlanProgress(finalControl)
    syncRuntimePlanStatus(session, finalControl, executionState)
    if (!streamFailed && !streamAborted) {
      if (
        executionState.semanticContinuation
        || finalControl?.awaiting === 'user_confirm_write'
        || finalControl?.currentMode === 'normal'
      ) {
        executionState.confirmationRequired = false
      }
      executionState.lastInterceptCode = null
      executionState.lastInterceptMessage = null
      executionState.lastInterceptGuidance = null
    }
    const finalStepRequiresSave = currentStepRequiresDocumentSave(session.runtimePlan, executionState, finalControl)
    const shouldAutoSaveAfterWrite = (
      !streamFailed
      && !streamAborted
      && props.docType === 'doc'
      && props.docId
      && finalStepRequiresSave
      && (wroteDocument || executionState.writeCompleted || roundDocumentWriteObserved)
      && currentDocumentHasUnsavedChanges()
      && !saveToolSucceededThisTurn
    )
    if (shouldAutoSaveAfterWrite) {
      const autoSaveOutputs = await executeAgentToolCalls([{
        call_id: genId(),
        name: 'save_current_document',
        arguments: JSON.stringify({ doc_id: props.docId }),
      }])
      const autoSaveOutput = autoSaveOutputs[0]
      const autoSaveResult = autoSaveOutput?.output && typeof autoSaveOutput.output === 'object'
        ? autoSaveOutput.output as Record<string, any>
        : null
      const autoSavePayload = autoSaveResult?.result && typeof autoSaveResult.result === 'object'
        ? autoSaveResult.result as Record<string, any>
        : null
      saveToolAttemptedThisTurn = true
      if (autoSaveResult?.ok === true && (autoSavePayload?.saved === true || autoSavePayload?.already_saved === true)) {
        saveToolSucceededThisTurn = true
        if (!planStepAdvancedThisRound) {
          planStepAdvancedThisRound = advanceExecutionPlanStep(session.runtimePlan, executionState, finalControl, {
            wroteDocument: roundDocumentWriteObserved || wroteDocument,
            savedDocument: true,
            mutationCompleted: true,
          })
        }
      } else if (currentDocumentHasUnsavedChanges()) {
        streamFailed = true
        const errorMessage = typeof autoSaveResult?.error === 'string' && autoSaveResult.error.trim()
          ? autoSaveResult.error.trim()
          : '模型已完成正文修改，但未成功保存当前文档。'
        ElMessage.error(errorMessage)
      }
    }
    if (!streamFailed) {
      appendUnsavedDraftNotice(finalControl)
    }
    const runtimePlanText = runtimePlanToPlanText(session.runtimePlan)
    const finalPendingPlanCandidate = (
      extractedPlan
      || runtimePlanText
      || session.lastPlan?.trim()
      || pendingPlan
    ).trim()
    const runtimePlanStillPending = session.runtimePlan?.status === 'pending'
    const finalHasPlanSignal = Boolean(extractedPlan || sawPlanSignalThisRound)
    const finalPhase = typeof finalControl?.phase === 'string' ? finalControl.phase : null
    const finalStillNeedsConfirmation = finalPhase !== 'completed'
      && shouldTreatPlanAsPending({
        planText: finalPendingPlanCandidate,
        control: finalControl,
        session,
        executionState,
        sawPlanSignal: finalHasPlanSignal || runtimePlanStillPending,
      })
    if (finalStillNeedsConfirmation) {
      session.pendingPlan = finalPendingPlanCandidate
      session.pendingPlanToolOutputs = [...collectedPlanToolOutputs]
    } else if (session.pendingPlan) {
      session.pendingPlan = null
      session.pendingPlanToolOutputs = []
    } else if (session.pendingPlanToolOutputs.length) {
      session.pendingPlanToolOutputs = []
    }
    finalizeAssistantMessageForDisplay(finalAssistantContent, { includeStructuredMessage: false })
    const finalLoopResultContent = structuredResponse?.message?.trim()
      || stripProtocolContent(finalAssistantContent).trim()
    appendFinalLoopResultMessage(finalLoopResultContent)
    if ((!streamFailed || Boolean(interceptedFailure)) && !streamAborted) {
      const memoryPlan = session.lastPlan || null
      const memorySummary = compactMessageText(assistantMessage.content || finalAssistantContent, 400) || null
      const memoryHasSignals = Boolean(
        memorySummary
        || executionState.recentToolCalls.length
        || executionState.documentWriteObserved
        || executionState.lastInterceptCode,
      )
      if (memoryHasSignals) {
        session.lastExecutionMemory = {
          currentMode: executionState.currentMode,
          awaiting: executionState.awaiting,
          currentActionKind: executionState.currentActionKind,
          currentActionStatus: executionState.currentActionStatus,
          currentActionMode: executionState.currentActionMode,
          currentActionTarget: executionState.currentActionTarget,
          confirmationRequired: executionState.confirmationRequired,
          plan: memoryPlan,
          assistantSummary: memorySummary,
          controlPhase: typeof finalControl?.phase === 'string' && finalControl.phase.trim() ? finalControl.phase.trim() : null,
          taskKind: executionState.taskKind,
          editIntent: executionState.editIntent,
          editStage: executionState.editStage,
          saveRequested: executionState.saveRequested,
          writeCompleted: executionState.writeCompleted,
          planStepIndex: executionState.planStepIndex,
          planTotalSteps: executionState.planTotalSteps,
          planCurrentStep: executionState.planCurrentStep,
          planCompletedSteps: [...executionState.planCompletedSteps],
          documentWriteObserved: executionState.documentWriteObserved,
          saveAttemptWithoutDocumentChange: executionState.saveAttemptWithoutDocumentChange,
          lastInterceptCode: executionState.lastInterceptCode,
          lastInterceptMessage: executionState.lastInterceptMessage,
          lastInterceptGuidance: executionState.lastInterceptGuidance,
          recentToolCalls: [...executionState.recentToolCalls],
        }
      }
      session.sessionMemory = buildSessionMemory(session)
    }
    if (!assistantMessage.content.trim() && !assistantMessage.reasoning?.trim()) {
      session.messages = session.messages.filter((message) => message.id !== assistantMessage.id)
    }
    session.previousResponseId = previousResponseId
    session.lastSyncedMessageCount = 0
    session.updatedAt = Date.now()
    logAgentDebugGroup(`session=${session.id} final`, {
      finalAssistantContent,
      finalControl,
      previousResponseId,
      executionState,
      runtimePlan: session.runtimePlan,
      pendingPlan: session.pendingPlan,
      lastExecutionMemory: session.lastExecutionMemory,
      saveToolAttemptedThisTurn,
      saveToolSucceededThisTurn,
      streamFailed,
      streamAborted,
      executionSnapshotFinal: buildDebugExecutionSnapshot(session, executionState, finalControl),
    })
    streaming.value = false
    streamingAssistantId.value = ''
    liveAssistantContent.value = ''
    liveAssistantReasoning.value = ''
    sessions.value = [...sessions.value]
    persistSessions()
  }
}

watch(
  () => currentSession.value?.messages.length,
  () => {
    scrollMessagesToBottom()
  },
)

watch(currentSessionId, () => {
  scrollMessagesToBottom()
})

watch([collapsed, panelX, panelY, currentSessionId], () => {
  const clamped = clampPanelPosition(panelX.value, panelY.value, collapsed.value)
  if (clamped.x !== panelX.value) panelX.value = clamped.x
  if (clamped.y !== panelY.value) panelY.value = clamped.y
  if (collapsed.value) {
    collapsedPanelX.value = panelX.value
    collapsedPanelY.value = panelY.value
  } else {
    expandedPanelX.value = panelX.value
    expandedPanelY.value = panelY.value
  }
  persistPanelState()
})

watch([activeProviderId, currentSessionId, activeModelKey], () => {
  syncSessionWithActiveProvider(currentSession.value)
})

watch([currentSessionId, () => currentSessionModel.value, activeProviderId], () => {
  clearComposerAttachments()
})

watch(composerAttachmentEnabled, (enabled) => {
  if (!enabled && composerAttachments.value.length) {
    clearComposerAttachments()
  }
})

watch(showProviderDialog, (visible) => {
  if (!visible) return
  ensureProviderDraftLoaded()
})

watch(showMcpDialog, (visible) => {
  if (!visible) return
  void refreshMcpManagementState()
})

watch(
  () => mcpDraft.value.transport,
  (transport) => {
    if (transport === 'stdio') {
      mcpDraft.value.authType = 'none'
    }
  },
)

watch(
  () => mcpRuntimeCapabilities.value.stdioEnabled,
  (enabled) => {
    if (!enabled && mcpDraft.value.transport === 'stdio') {
      mcpDraft.value.transport = 'sse'
    }
  },
)

watch(
  () => mcpDraft.value.customHeadersMode,
  (mode) => {
    if (mode !== 'replace') {
      mcpDraft.value.customHeadersText = ''
    }
  },
)

watch(
  () => mcpDraft.value.stdioEnvMode,
  (mode) => {
    if (mode !== 'replace') {
      mcpDraft.value.stdioEnvText = ''
    }
  },
)

watch(showModelDialog, (visible) => {
  customModelInput.value = ''
  modelSearchQuery.value = ''
  if (!visible) return
  resetModelDraft()
})

onMounted(async () => {
  mounted.value = true

  const panelState = loadJson(PANEL_STATE_KEY, defaultPanelState(true))
  const defaultExpanded = clampPanelPosition(
    Number.isFinite(panelState.expandedPanelX) ? panelState.expandedPanelX : defaultPanelState(false).panelX,
    Number.isFinite(panelState.expandedPanelY) ? panelState.expandedPanelY : defaultPanelState(false).panelY,
    false,
  )
  const defaultCollapsed = clampPanelPosition(
    Number.isFinite(panelState.collapsedPanelX) ? panelState.collapsedPanelX : defaultPanelState(true).panelX,
    Number.isFinite(panelState.collapsedPanelY) ? panelState.collapsedPanelY : defaultPanelState(true).panelY,
    true,
  )
  collapsed.value = Boolean(panelState.collapsed)
  expandedPanelX.value = defaultExpanded.x
  expandedPanelY.value = defaultExpanded.y
  collapsedPanelX.value = defaultCollapsed.x
  collapsedPanelY.value = defaultCollapsed.y
  const initialPosition = collapsed.value ? defaultCollapsed : defaultExpanded
  panelX.value = initialPosition.x
  panelY.value = initialPosition.y

  try {
    await refreshProvidersState()
  } catch (error: any) {
    logAgentPanelError('refresh_providers_state', error)
    ElMessage.error(error.response?.data?.error || error.message || '加载供应商失败')
  }
  sessions.value = loadSessions(activeProvider.value)

  if (!sessions.value.length) {
    createSession()
  } else {
    currentSessionId.value = panelState.currentSessionId && sessions.value.some((session) => session.id === panelState.currentSessionId)
      ? panelState.currentSessionId
      : sessions.value[0].id
  }

  ensureProviderDraftLoaded()
  syncSessionWithActiveProvider(currentSession.value)
  persistSessions()
})

onUnmounted(() => {
  stopDrag()
})
</script>

<style scoped>
.agent-panel {
  position: fixed;
  top: 0;
  left: 0;
  z-index: 1800;
  width: 460px;
  max-width: calc(100vw - 32px);
}

.agent-panel.collapsed {
  width: auto;
}

.agent-fab {
  border: none;
  border-radius: 999px;
  padding: 12px 18px;
  background: linear-gradient(135deg, #6f9a4f, #537535);
  color: #f8fff1;
  font-size: 13px;
  font-weight: 700;
  box-shadow: 0 18px 36px rgba(83, 117, 53, 0.24);
  cursor: pointer;
}

.agent-shell {
  display: flex;
  flex-direction: column;
  height: min(760px, calc(100vh - 64px));
  min-height: 460px;
  max-height: calc(100vh - 64px);
  border-radius: 20px;
  overflow: hidden;
  border: 1px solid rgba(122, 147, 91, 0.22);
  background:
    radial-gradient(circle at top right, rgba(215, 227, 196, 0.72), transparent 28%),
    linear-gradient(180deg, rgba(251, 252, 247, 0.96), rgba(246, 248, 239, 0.94));
  backdrop-filter: blur(16px);
  box-shadow: 0 32px 72px rgba(67, 86, 50, 0.16);
}

.agent-header {
  display: block;
  padding: 14px 16px 12px;
  border-bottom: 1px solid rgba(122, 147, 91, 0.16);
  cursor: move;
}

.agent-header-content {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.agent-meta-line,
.agent-session-line {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.agent-meta-line {
  flex: 1;
  font-size: 12px;
  color: #708067;
}

.agent-meta-label {
  color: #607057;
  flex-shrink: 0;
}

.agent-meta-value {
  color: #24311f;
  min-width: 0;
}

.agent-meta-url {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.agent-toolbar-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.agent-meta-divider {
  color: #9ca994;
}

.agent-header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  flex-wrap: nowrap;
}

.agent-session-line {
  flex: 1;
}

.agent-session-select {
  width: min(240px, 100%);
}

.agent-header-icon,
.agent-icon-btn {
  --el-button-bg-color: rgba(255, 255, 255, 0.78);
  --el-button-border-color: rgba(122, 147, 91, 0.24);
  --el-button-text-color: #607057;
  --el-button-hover-bg-color: rgba(232, 240, 220, 0.92);
  --el-button-hover-border-color: rgba(111, 154, 79, 0.34);
  --el-button-hover-text-color: #24311f;
  --el-button-disabled-bg-color: rgba(255, 255, 255, 0.68);
  --el-button-disabled-border-color: rgba(122, 147, 91, 0.14);
  --el-button-disabled-text-color: rgba(96, 112, 87, 0.45);
}

.header-btn,
.session-create,
.ghost-action,
.primary-action {
  height: 32px;
  border-radius: 10px;
  padding: 0 12px;
  font-size: 12px;
  font-weight: 700;
  cursor: pointer;
}

.header-btn,
.session-create,
.ghost-action {
  border: 1px solid rgba(122, 147, 91, 0.24);
  background: rgba(255, 255, 255, 0.78);
  color: #607057;
}

.primary-action {
  min-width: 72px;
  border: none;
  background: linear-gradient(135deg, #5d7f3f, #87a948);
  color: #f8fce9;
}

.header-btn:hover,
.session-create:hover,
.ghost-action:hover {
  background: rgba(232, 240, 220, 0.92);
}

.agent-main {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: row;
}

.agent-sidebar {
  display: none;
  width: 308px;
  flex-shrink: 0;
  border-right: 1px solid rgba(122, 147, 91, 0.12);
  background: rgba(247, 249, 242, 0.72);
  padding: 14px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.agent-sidebar-toggle {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 14px;
  border: 1px solid rgba(122, 147, 91, 0.16);
  background: rgba(255, 255, 255, 0.88);
  cursor: pointer;
  text-align: left;
}

.agent-sidebar-toggle-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.agent-sidebar-toggle-title {
  font-size: 12px;
  font-weight: 700;
  color: #31402b;
}

.agent-sidebar-toggle-summary {
  font-size: 12px;
  color: #6b7961;
  line-height: 1.5;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.agent-sidebar-toggle-meta {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.agent-sidebar-scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding-right: 4px;
}

.agent-workspace {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.agent-messages {
  flex: 1;
  min-height: 0;
  overflow: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
  scrollbar-width: thin;
  scrollbar-color: rgba(111, 154, 79, 0.58) rgba(122, 147, 91, 0.12);
  padding: 18px 24px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.agent-messages::-webkit-scrollbar,
.agent-sidebar-scroll::-webkit-scrollbar,
.provider-list::-webkit-scrollbar,
.model-pane-scroll::-webkit-scrollbar {
  width: 10px;
}

.agent-messages::-webkit-scrollbar-track,
.agent-sidebar-scroll::-webkit-scrollbar-track,
.provider-list::-webkit-scrollbar-track,
.model-pane-scroll::-webkit-scrollbar-track {
  background: rgba(122, 147, 91, 0.12);
  border-radius: 999px;
}

.agent-messages::-webkit-scrollbar-thumb,
.agent-sidebar-scroll::-webkit-scrollbar-thumb,
.provider-list::-webkit-scrollbar-thumb,
.model-pane-scroll::-webkit-scrollbar-thumb {
  background: rgba(111, 154, 79, 0.58);
  border-radius: 999px;
  border: 2px solid rgba(246, 248, 239, 0.9);
}

.agent-plan-card {
  border: 1px solid rgba(122, 147, 91, 0.2);
  border-radius: 16px;
  padding: 11px 12px;
  background: rgba(250, 252, 244, 0.92);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.72);
}

.agent-plan-card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 8px;
}

.agent-plan-card-title {
  font-size: 12px;
  font-weight: 700;
  color: #405037;
}

.agent-plan-card-status {
  border-radius: 999px;
  padding: 2px 8px;
  background: rgba(157, 172, 140, 0.18);
  color: #607057;
  font-size: 11px;
  font-weight: 700;
}

.agent-plan-card-status.active {
  background: rgba(111, 154, 79, 0.16);
  color: #4d6a34;
}

.agent-plan-card-content {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 12px;
  line-height: 1.6;
  color: #2c3a26;
}

.agent-runtime-card {
  border: 1px solid rgba(122, 147, 91, 0.16);
  border-radius: 16px;
  padding: 11px 12px;
  background: rgba(255, 255, 255, 0.72);
}

.agent-runtime-overview {
  gap: 10px;
}

.agent-runtime-chip-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.agent-runtime-chip {
  display: inline-flex;
  align-items: center;
  padding: 4px 8px;
  border-radius: 999px;
  background: rgba(122, 147, 91, 0.1);
  color: #506046;
  font-size: 11px;
  line-height: 1.4;
}

.agent-runtime-current-step {
  font-size: 13px;
  line-height: 1.6;
  color: #24311f;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.agent-runtime-empty {
  min-height: 140px;
  justify-content: center;
}

.agent-runtime-empty-text {
  font-size: 12px;
  line-height: 1.7;
  color: #6b7961;
}

.agent-plan-steps {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 10px;
}

.agent-plan-step {
  border-radius: 12px;
  padding: 10px 12px;
  background: rgba(246, 248, 239, 0.88);
  border: 1px solid rgba(122, 147, 91, 0.14);
}

.agent-plan-step.running {
  border-color: rgba(111, 154, 79, 0.38);
  background: rgba(240, 247, 231, 0.95);
}

.agent-plan-step.completed {
  opacity: 0.78;
}

.agent-plan-step-title {
  font-size: 12px;
  font-weight: 700;
  color: #24311f;
}

.agent-plan-step-meta {
  margin-top: 4px;
  font-size: 11px;
  color: #6a7861;
}

.agent-plan-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
}

.agent-tool-events {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.agent-tool-event {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px 10px;
  border-radius: 12px;
  background: rgba(248, 250, 243, 0.92);
  border: 1px solid rgba(122, 147, 91, 0.1);
}

.agent-tool-event-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.agent-tool-event-name {
  min-width: 0;
  font-size: 12px;
  font-weight: 700;
  color: #31402b;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.agent-tool-event-status {
  flex-shrink: 0;
  font-size: 11px;
  color: #6f7e65;
}

.agent-tool-event-summary {
  font-size: 11px;
  line-height: 1.55;
  color: #4d5b44;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.agent-artifact-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.agent-artifact-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.agent-artifact-title {
  font-size: 12px;
  font-weight: 700;
  color: #24311f;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.agent-artifact-status {
  font-size: 11px;
  color: #607057;
}

.agent-empty {
  margin: auto 0;
  padding: 18px 20px;
  border-radius: 16px;
  background: rgba(255, 255, 255, 0.72);
  border: 1px dashed rgba(122, 147, 91, 0.24);
  color: #708067;
}

.agent-empty-title {
  font-size: 14px;
  font-weight: 700;
  color: #24311f;
}

.agent-empty-desc {
  margin-top: 8px;
  font-size: 12px;
  line-height: 1.6;
}

.agent-mode-tip-warning {
  color: #a0552a;
}

.agent-message {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: min(100%, 400px);
  align-self: center;
}

.message-role {
  font-size: 12px;
  font-weight: 700;
  color: #51604a;
}

.message-content {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: inherit;
  font-size: 13px;
  line-height: 1.6;
  color: #24311f;
  background: transparent;
}

.message-reasoning {
  width: 100%;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.84);
  border: 1px solid rgba(122, 147, 91, 0.14);
  overflow: hidden;
}

.message-reasoning summary {
  list-style: none;
  cursor: pointer;
  user-select: none;
  padding: 8px 10px;
  font-size: 12px;
  font-weight: 700;
  color: #607057;
}

.message-reasoning summary::-webkit-details-marker {
  display: none;
}

.message-reasoning summary::before {
  content: '▸';
  margin-right: 8px;
  color: #6f9a4f;
}

.message-reasoning[open] summary::before {
  content: '▾';
}

.message-reasoning-content {
  margin: 0;
  padding: 0 10px 10px;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: inherit;
  font-size: 12px;
  line-height: 1.6;
  color: #708067;
}

.message-attachments {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 10px;
}

.message-attachment-chip {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.9);
  border: 1px solid rgba(122, 147, 91, 0.14);
  color: #51604a;
  font-size: 12px;
  cursor: pointer;
  transition: border-color 0.16s ease, background 0.16s ease;
}

.message-attachment-chip:hover {
  background: rgba(244, 248, 235, 0.98);
  border-color: rgba(122, 147, 91, 0.22);
}

.message-attachment-name {
  font-weight: 700;
  color: #31402b;
}

.message-attachment-meta {
  color: #7a846f;
}

.agent-composer {
  flex-shrink: 0;
  border-top: 1px solid rgba(122, 147, 91, 0.12);
  padding: 12px 18px 16px;
  background: rgba(251, 252, 247, 0.94);
}

.agent-runtime-pill {
  display: inline-flex;
  align-items: center;
  padding: 4px 8px;
  border-radius: 999px;
  background: rgba(122, 147, 91, 0.12);
  color: #537535;
  font-size: 11px;
  font-weight: 700;
}

.agent-runtime-chevron {
  font-size: 11px;
  color: #7a846f;
}

.agent-mode-tip {
  width: min(100%, 400px);
  margin: 0 auto;
  font-size: 11px;
  color: #7b8771;
  line-height: 1.6;
}

.agent-textarea {
  width: min(100%, 400px);
  margin: 10px auto 0;
  min-height: 112px;
  resize: none;
  border-radius: 16px;
  border: 1px solid rgba(122, 147, 91, 0.18);
  background: rgba(255, 255, 255, 0.92);
  color: #1d2719;
  padding: 10px 12px;
  font: inherit;
  font-size: 13px;
  line-height: 1.6;
}

.agent-textarea::placeholder {
  color: #87937e;
}

.agent-textarea:focus {
  outline: none;
  border-color: rgba(111, 154, 79, 0.46);
  box-shadow: 0 0 0 3px rgba(111, 154, 79, 0.12);
}

.agent-hidden-input {
  display: none;
}

.agent-attachment-panel {
  width: min(100%, 400px);
  margin: 10px auto 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.agent-attachment-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.agent-attachment-tip {
  font-size: 12px;
  line-height: 1.6;
  color: #718068;
}

.agent-attachment-trigger {
  flex-shrink: 0;
  margin-left: auto;
  border: 1px solid rgba(122, 147, 91, 0.18);
  border-radius: 999px;
  padding: 6px 12px;
  background: rgba(122, 147, 91, 0.12);
  color: #537535;
  font-size: 12px;
  font-weight: 700;
  cursor: pointer;
}

.agent-attachment-trigger:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.agent-attachment-list {
  display: flex;
  flex: 1;
  gap: 8px;
  flex-wrap: nowrap;
  min-width: 0;
  overflow-x: auto;
  overflow-y: hidden;
  padding-bottom: 2px;
}

.agent-attachment-item {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  gap: 8px;
  max-width: 260px;
  padding: 6px 10px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.88);
  border: 1px solid rgba(122, 147, 91, 0.12);
  cursor: pointer;
  transition: border-color 0.16s ease, background 0.16s ease;
}

.agent-attachment-item:hover {
  background: rgba(244, 248, 235, 0.98);
  border-color: rgba(122, 147, 91, 0.22);
}

.agent-attachment-item-main {
  min-width: 0;
  display: flex;
  align-items: center;
}

.agent-attachment-item-name {
  max-width: 180px;
  font-size: 12px;
  font-weight: 700;
  color: #31402b;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.agent-attachment-item-meta {
  font-size: 11px;
  color: #7a846f;
  white-space: nowrap;
}

.agent-attachment-remove {
  flex-shrink: 0;
  border: none;
  background: transparent;
  color: #c65b4d;
  font-size: 12px;
  padding: 0;
  cursor: pointer;
}

.agent-attachment-remove:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.attachment-preview-body {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 240px;
}

.attachment-preview-image {
  display: block;
  max-width: 100%;
  max-height: 72vh;
  object-fit: contain;
  border-radius: 16px;
  box-shadow: 0 18px 40px rgba(34, 48, 25, 0.18);
}

.agent-composer-footer {
  width: min(100%, 400px);
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 10px;
  margin: 12px auto 0;
}

.agent-shortcut-tip {
  font-size: 12px;
  color: #7b8771;
  line-height: 1.6;
}

.agent-bottom-bar {
  display: flex;
  align-items: center;
  gap: 12px;
}

.agent-controls {
  flex: 1;
  min-width: 0;
  display: flex;
}

.agent-inline-selects {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  width: 100%;
}

.agent-select-row {
  display: grid;
  grid-template-columns: 42px minmax(0, 1fr);
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.agent-control-label {
  font-size: 12px;
  color: #607057;
  text-align: right;
  white-space: nowrap;
}

.agent-provider-select,
.agent-model-select,
.agent-session-select {
  flex: 1;
  min-width: 0;
}

.agent-action-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.session-select-option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.session-select-info,
.session-select-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.session-select-info {
  flex: 1;
}

.session-select-title,
.session-select-meta,
.session-select-time {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.session-select-meta {
  color: #7f8b76;
  font-size: 12px;
}

.session-select-time {
  color: #8a957f;
  font-size: 11px;
}

.session-select-delete {
  flex-shrink: 0;
  color: #b45f52;
  font-size: 11px;
  cursor: pointer;
}

.provider-manager {
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
  gap: 12px;
}

.provider-list-pane {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.provider-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-height: 360px;
  overflow: auto;
  scrollbar-width: thin;
}

.provider-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  align-items: flex-start;
  padding: 12px;
  border-radius: 14px;
  border: 1px solid rgba(122, 147, 91, 0.14);
  background: rgba(248, 250, 242, 0.9);
  color: #51604a;
  text-align: left;
  cursor: pointer;
}

.provider-item.active {
  border-color: rgba(111, 154, 79, 0.38);
  background: rgba(232, 240, 220, 0.92);
}

.provider-item-name {
  font-size: 13px;
  font-weight: 700;
  color: #24311f;
}

.provider-item-tag {
  margin-left: 6px;
  display: inline-flex;
  align-items: center;
  padding: 1px 6px;
  border-radius: 999px;
  background: rgba(83, 117, 53, 0.14);
  color: #537535;
  font-size: 10px;
  font-weight: 700;
}

.provider-item-meta {
  font-size: 11px;
  line-height: 1.5;
  color: #708067;
  word-break: break-word;
}

.provider-editor {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.mcp-editor {
  gap: 10px;
}

.provider-hint {
  font-size: 12px;
  color: #708067;
  line-height: 1.6;
}

.mcp-settings-card,
.mcp-status-card,
.mcp-snapshot-card,
.mcp-secret-block {
  border-radius: 14px;
  border: 1px solid rgba(122, 147, 91, 0.14);
  background: rgba(248, 250, 242, 0.84);
}

.mcp-settings-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
}

.mcp-settings-copy {
  min-width: 0;
}

.mcp-settings-title {
  font-size: 13px;
  font-weight: 700;
  color: #24311f;
}

.mcp-settings-desc,
.mcp-runtime-hint,
.mcp-field-help,
.mcp-secret-summary {
  font-size: 12px;
  line-height: 1.6;
  color: #708067;
}

.mcp-runtime-hint {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.mcp-item {
  gap: 4px;
}

.mcp-item-error {
  font-size: 11px;
  line-height: 1.5;
  color: #b45f52;
  word-break: break-word;
}

.mcp-form-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.mcp-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.mcp-field-full {
  grid-column: 1 / -1;
}

.mcp-field-label {
  font-size: 12px;
  font-weight: 700;
  color: #4f6047;
}

.mcp-field-inline {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 40px;
  flex-wrap: wrap;
}

.mcp-secret-block {
  padding: 10px 12px;
}

.mcp-secret-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 10px;
}

.mcp-secret-mode-select {
  width: 136px;
  flex-shrink: 0;
}

.mcp-status-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px;
}

.mcp-status-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.mcp-status-label {
  font-size: 12px;
  color: #708067;
}

.mcp-status-value {
  font-size: 12px;
  font-weight: 700;
  color: #24311f;
  text-align: right;
}

.mcp-status-error {
  padding: 10px 12px;
  border-radius: 12px;
  background: rgba(180, 95, 82, 0.08);
  color: #a14b40;
  font-size: 12px;
  line-height: 1.6;
  word-break: break-word;
}

.mcp-snapshot-card {
  padding: 10px 12px;
}

.mcp-snapshot-tabs {
  margin-bottom: 8px;
}

.mcp-snapshot-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-height: 320px;
  overflow: auto;
  padding-right: 4px;
  scrollbar-width: thin;
  scrollbar-color: rgba(111, 154, 79, 0.58) rgba(122, 147, 91, 0.12);
}

.mcp-snapshot-list::-webkit-scrollbar {
  width: 10px;
}

.mcp-snapshot-list::-webkit-scrollbar-track {
  background: rgba(122, 147, 91, 0.12);
  border-radius: 999px;
}

.mcp-snapshot-list::-webkit-scrollbar-thumb {
  background: rgba(111, 154, 79, 0.58);
  border-radius: 999px;
  border: 2px solid rgba(246, 248, 239, 0.9);
}

.mcp-snapshot-item {
  border-radius: 12px;
  border: 1px solid rgba(122, 147, 91, 0.12);
  background: rgba(255, 255, 255, 0.88);
  padding: 10px 12px;
}

.mcp-snapshot-title {
  font-size: 12px;
  font-weight: 700;
  color: #24311f;
  margin-bottom: 6px;
}

.mcp-snapshot-raw {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 11px;
  line-height: 1.6;
  color: #51604a;
}

.model-manager {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.model-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 14px;
}

.model-pane {
  display: flex;
  flex-direction: column;
  min-height: 0;
  border-radius: 16px;
  border: 1px solid rgba(122, 147, 91, 0.14);
  background: rgba(248, 250, 242, 0.78);
}

.model-pane-head {
  padding: 12px 12px 10px;
  border-bottom: 1px solid rgba(122, 147, 91, 0.1);
}

.model-pane-head-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 10px;
}

.model-pane-body {
  padding: 12px;
}

.model-pane-foot {
  padding: 0 12px 12px;
}

.model-pane-scroll {
  min-height: 320px;
  max-height: 320px;
  overflow: auto;
  scrollbar-width: thin;
  scrollbar-color: rgba(111, 154, 79, 0.58) rgba(122, 147, 91, 0.12);
}

.model-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.model-header-title {
  font-size: 14px;
  font-weight: 700;
  color: #24311f;
}

.model-header-meta {
  margin-top: 4px;
  font-size: 12px;
  color: #708067;
  line-height: 1.6;
}

.model-custom-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 10px;
}

.model-section-title {
  font-size: 13px;
  font-weight: 700;
  color: #24311f;
}

.model-search-input {
  min-width: 0;
}

.current-model-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.current-model-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.88);
  border: 1px solid rgba(122, 147, 91, 0.12);
}

.current-model-main {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
}

.current-model-name {
  min-width: 0;
  font-size: 13px;
  font-weight: 600;
  color: #24311f;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.current-model-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.model-action-button {
  --el-button-text-color: #537535;
  --el-button-hover-text-color: #405a29;
  --el-button-bg-color: rgba(122, 147, 91, 0.14);
  --el-button-hover-bg-color: rgba(122, 147, 91, 0.2);
  --el-button-active-bg-color: rgba(122, 147, 91, 0.24);
  padding: 4px 10px;
  border-radius: 999px;
  font-weight: 700;
}

.model-source-badge {
  display: inline-flex;
  align-items: center;
  padding: 2px 6px;
  border-radius: 999px;
  background: rgba(83, 117, 53, 0.14);
  color: #537535;
  font-size: 10px;
  font-weight: 700;
}

.model-source-badge.is-custom {
  background: rgba(37, 99, 235, 0.12);
  color: #2563eb;
}

.model-source-badge.is-configured {
  background: rgba(217, 119, 6, 0.12);
  color: #b45309;
}

.model-configured-dot {
  display: inline-flex;
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: #d97706;
  box-shadow: 0 0 0 4px rgba(217, 119, 6, 0.12);
  flex-shrink: 0;
}

.model-check-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.model-check-item {
  display: flex;
  align-items: center;
  min-height: 32px;
  color: #51604a;
}

.model-empty,
.model-empty-state {
  font-size: 12px;
  line-height: 1.6;
  color: #708067;
}

.model-empty-state {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 12px;
}

.model-config-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.model-config-toolbar {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.model-config-reference {
  min-width: 0;
  flex: 1;
  padding: 10px 12px;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.68);
  border: 1px solid rgba(122, 147, 91, 0.12);
}

.model-config-reference-title {
  font-size: 12px;
  font-weight: 700;
  color: #24311f;
  margin-bottom: 4px;
}

.model-config-reference-text {
  font-size: 11px;
  line-height: 1.5;
  color: #708067;
}

.model-config-toolbar-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.model-config-scroll {
  max-height: 48vh;
  overflow: auto;
  padding-right: 4px;
}

.model-config-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.model-config-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.model-config-field-full {
  grid-column: 1 / -1;
}

.model-config-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 700;
  color: #4f6047;
}

.param-help-icon {
  color: #8a957f;
  cursor: help;
}

.model-modality-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  min-height: 36px;
  padding: 8px 10px;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.82);
  border: 1px solid rgba(122, 147, 91, 0.14);
}

.model-modality-item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: #51604a;
}

:deep(.provider-dialog .el-dialog),
:deep(.model-dialog .el-dialog),
:deep(.model-config-dialog .el-dialog) {
  background: linear-gradient(180deg, rgba(251, 252, 247, 0.98), rgba(246, 248, 239, 0.96));
  border: 1px solid rgba(122, 147, 91, 0.18);
  box-shadow: 0 24px 60px rgba(67, 86, 50, 0.18);
}

:deep(.mcp-dialog .el-dialog) {
  width: min(840px, calc(100vw - 36px)) !important;
  max-height: calc(100vh - 28px);
  display: flex;
  flex-direction: column;
}

:deep(.model-config-dialog .el-dialog) {
  width: min(760px, calc(100vw - 36px)) !important;
}

:deep(.provider-dialog .el-dialog__title),
:deep(.model-dialog .el-dialog__title),
:deep(.model-config-dialog .el-dialog__title) {
  color: #24311f;
}

:deep(.provider-dialog .el-dialog__header),
:deep(.model-dialog .el-dialog__header),
:deep(.model-config-dialog .el-dialog__header) {
  margin-right: 0;
  padding-bottom: 8px;
  border-bottom: 1px solid rgba(122, 147, 91, 0.1);
}

:deep(.model-config-dialog .el-dialog__body) {
  padding-top: 16px;
  padding-bottom: 12px;
}

:deep(.mcp-dialog .el-dialog__body) {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  padding-top: 14px;
  padding-bottom: 12px;
}

:deep(.mcp-dialog .el-dialog__footer) {
  flex-shrink: 0;
  padding-top: 10px;
  border-top: 1px solid rgba(122, 147, 91, 0.1);
  background: linear-gradient(180deg, rgba(251, 252, 247, 0.98), rgba(246, 248, 239, 0.96));
}

.mcp-dialog .provider-manager {
  align-items: stretch;
  height: min(68vh, 680px);
  min-height: 0;
  overflow: hidden;
}

.mcp-dialog .provider-list-pane {
  min-height: 0;
}

.mcp-dialog .provider-list {
  flex: 1;
  min-height: 0;
  max-height: none;
}

.mcp-dialog .provider-editor {
  min-height: 0;
  overflow: auto;
  padding-right: 4px;
  scrollbar-width: thin;
  scrollbar-color: rgba(111, 154, 79, 0.58) rgba(122, 147, 91, 0.12);
}

.mcp-dialog .mcp-status-card,
.mcp-dialog .mcp-snapshot-card,
.mcp-dialog .mcp-secret-block {
  border-radius: 12px;
}

.mcp-dialog .provider-editor::-webkit-scrollbar,
.mcp-dialog .provider-list::-webkit-scrollbar {
  width: 10px;
}

.mcp-dialog .provider-editor::-webkit-scrollbar-track,
.mcp-dialog .provider-list::-webkit-scrollbar-track {
  background: rgba(122, 147, 91, 0.12);
  border-radius: 999px;
}

.mcp-dialog .provider-editor::-webkit-scrollbar-thumb,
.mcp-dialog .provider-list::-webkit-scrollbar-thumb {
  background: rgba(111, 154, 79, 0.58);
  border-radius: 999px;
  border: 2px solid rgba(246, 248, 239, 0.9);
}

:deep(.provider-dialog .el-input__wrapper),
:deep(.provider-dialog .el-select__wrapper),
:deep(.model-dialog .el-input__wrapper),
:deep(.model-dialog .el-select__wrapper),
.agent-shell :deep(.el-input__wrapper),
.agent-shell :deep(.el-select__wrapper) {
  background: rgba(255, 255, 255, 0.92);
  box-shadow: 0 0 0 1px rgba(122, 147, 91, 0.14) inset;
}

:deep(.provider-dialog .el-input__inner),
:deep(.provider-dialog .el-select__selected-item),
:deep(.model-dialog .el-input__inner),
:deep(.model-dialog .el-select__selected-item),
.agent-shell :deep(.el-input__inner),
.agent-shell :deep(.el-select__selected-item) {
  color: #24311f;
}

:deep(.provider-dialog .el-input__inner::placeholder),
:deep(.model-dialog .el-input__inner::placeholder),
.agent-shell :deep(.el-input__inner::placeholder) {
  color: #87937e;
}

:deep(.provider-dialog .el-button),
:deep(.model-dialog .el-button),
:deep(.model-config-dialog .el-button) {
  --el-button-bg-color: rgba(255, 255, 255, 0.78);
  --el-button-border-color: rgba(122, 147, 91, 0.18);
  --el-button-text-color: #607057;
}

.model-config-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  width: 100%;
}

.model-config-footer-tip {
  font-size: 12px;
  color: #7a846f;
  text-align: left;
}

.model-config-footer-actions {
  display: flex;
  align-items: center;
  gap: 10px;
}

.model-check-list :deep(.el-checkbox__label) {
  color: #51604a;
}

@media (max-width: 1200px) {
  .agent-panel {
    width: min(460px, calc(100vw - 32px));
  }

  .model-config-toolbar {
    flex-direction: column;
    align-items: stretch;
  }

  .model-config-toolbar-actions {
    justify-content: flex-start;
  }

  .model-config-footer {
    flex-direction: column;
    align-items: stretch;
  }

  .model-config-footer-actions {
    justify-content: flex-end;
  }
}

@media (max-width: 720px) {
  .agent-panel {
    width: calc(100vw - 24px);
  }

  .agent-shell {
    height: min(760px, calc(100vh - 40px));
    min-height: 420px;
    max-height: calc(100vh - 40px);
  }

  .agent-main {
    flex-direction: column;
  }

  .agent-sidebar {
    width: 100%;
    max-height: 34vh;
    border-right: none;
    border-bottom: 1px solid rgba(122, 147, 91, 0.12);
  }

  .agent-inline-selects {
    grid-template-columns: 1fr;
  }

  .agent-toolbar-line,
  .agent-bottom-bar,
  .agent-attachment-head {
    flex-direction: column;
    align-items: stretch;
  }

  .agent-attachment-list {
    flex: none;
  }

  .agent-session-line,
  .agent-session-select,
  .agent-controls {
    width: 100%;
  }

  .provider-manager,
  .model-grid,
  .model-pane-head-row,
  .mcp-form-grid {
    grid-template-columns: 1fr;
  }

  .model-config-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
