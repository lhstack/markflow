<script setup lang="ts">
import { watch, ref } from 'vue'

import { mcpStatusLabel, mcpTransportLabel } from '@/api/agentMcp'
import { formatOptionalTimestamp } from '@/components/agent/format'
import { useAgentSkills } from '@/composables/useAgentSkills'
import { useMcpServers } from '@/composables/useMcpServers'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ (event: 'update:modelValue', value: boolean): void }>()

const activeResourceTab = ref<'mcp' | 'skills'>('mcp')

const {
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
  testConnection,
  refreshCapabilities,
} = useMcpServers()

const {
  loading: skillsLoading,
  fileLoading: skillFileLoading,
  fileSaving: skillFileSaving,
  skills,
  page: skillsPage,
  pageSize: skillsPageSize,
  pagedSkills,
  dialogVisible: skillDialogVisible,
  dialogMode: skillDialogMode,
  dialogTitle: skillDialogTitle,
  selectedFile: selectedSkillFile,
  fileContent: skillFileContent,
  treeData: skillTreeData,
  canSaveFile: canSaveSkillFile,
  loadSkills,
  openSkillDialog,
  selectSkillFile,
  saveSkillFile,
  removeSkill,
} = useAgentSkills()

function setVisible(value: boolean) {
  emit('update:modelValue', value)
}

function mcpSnapshotEntryTitle(entry: unknown, index: number): string {
  if (typeof entry === 'string' && entry.trim()) return entry.trim()
  if (entry && typeof entry === 'object') {
    const anyEntry = entry as Record<string, unknown>
    const candidate = anyEntry.title || anyEntry.name || anyEntry.uri || anyEntry.path || anyEntry.id
    if (typeof candidate === 'string' && candidate.trim()) return candidate.trim()
  }
  return `项目 ${index + 1}`
}

function mcpSnapshotEntryDetail(entry: unknown): string {
  try {
    return JSON.stringify(entry, null, 2)
  } catch {
    return String(entry)
  }
}

function skillTreeProps() {
  return { label: 'name', children: 'children' }
}

function skillNodeClass(data: any) {
  return data.kind === 'dir' ? 'skill-tree-dir' : 'skill-tree-file'
}

function skillAvailabilityType(skill: any) {
  return skill.available === false ? 'danger' : 'success'
}

function skillAvailabilityText(skill: any) {
  return skill.available === false ? '不可用' : '可用'
}

watch(
  () => props.modelValue,
  (visible) => {
    if (!visible) return
    void refreshState()
    void loadSkills()
  },
  { immediate: true },
)
</script>

<template>
  <el-dialog
    :model-value="modelValue"
    class="agent-dialog provider-dialog external-resource-dialog"
    title="外部资源管理"
    width="980px"
    append-to-body
    destroy-on-close
    align-center
    @update:model-value="setVisible"
  >
    <el-tabs v-model="activeResourceTab" class="external-resource-tabs">
      <el-tab-pane label="MCP" name="mcp">
        <div class="provider-manager">
          <aside class="provider-list-pane">
            <div class="mcp-settings-card">
              <div class="mcp-settings-copy">
                <div class="mcp-settings-title">启用 MCP</div>
                <div class="mcp-settings-desc">开启后，聊天会把已启用的 MCP 服务注入到当前工具链。</div>
              </div>
              <el-switch
                v-model="settingsEnabled"
                :loading="settingsSaving"
                inline-prompt
                active-text="开"
                inactive-text="关"
                @change="handleSettingsChange"
              />
            </div>

            <div class="provider-list">
              <button
                v-for="server in servers"
                :key="server.id"
                class="provider-item mcp-item"
                :class="{ active: server.id === draft.id }"
                @click="editServer(server.id)"
              >
                <span class="provider-item-name">
                  {{ server.name }}
                  <span v-if="server.enabled" class="provider-item-tag">已启用</span>
                </span>
                <span class="provider-item-meta">{{ mcpTransportLabel(server.transport) }} · {{ mcpStatusLabel(server.lastStatus) }}</span>
                <span v-if="server.lastError" class="mcp-item-error">{{ server.lastError }}</span>
              </button>

              <div v-if="!servers.length && !loading" class="provider-hint">还没有 MCP 服务，点击“新增”开始配置。</div>
              <div v-else-if="loading" class="provider-hint">正在加载 MCP 配置…</div>
            </div>

            <div class="mcp-runtime-hint">
              <div>可用传输：{{ availableTransports.length ? availableTransports.map((item) => mcpTransportLabel(item)).join(' / ') : '加载中' }}</div>
              <div v-if="runtimeCapabilities.stdioEnabled">
                允许的 STDIO 命令：{{ runtimeCapabilities.stdioAllowedCommands.length ? runtimeCapabilities.stdioAllowedCommands.join('、') : '未限制' }}
              </div>
              <div v-else>当前后端未开启 `stdio`，因此不会显示对应配置项。</div>
            </div>
          </aside>

          <section class="provider-editor mcp-editor">
            <div class="mcp-form-grid">
              <label class="mcp-field">
                <span class="mcp-field-label">名称</span>
                <el-input v-model="draft.name" placeholder="例如 文档知识库 / 项目检索 / GitHub MCP" />
              </label>

              <label class="mcp-field">
                <span class="mcp-field-label">启用</span>
                <div class="mcp-field-inline">
                  <el-switch v-model="draft.enabled" inline-prompt active-text="开" inactive-text="关" />
                  <span class="mcp-field-help">关闭后仍会保留配置，但不会注入到聊天工具链。</span>
                </div>
              </label>

              <label class="mcp-field">
                <span class="mcp-field-label">传输</span>
                <el-select v-model="draft.transport" placeholder="选择传输方式">
                  <el-option
                    v-for="transport in availableTransports"
                    :key="transport"
                    :label="mcpTransportLabel(transport)"
                    :value="transport"
                  />
                </el-select>
              </label>

              <template v-if="isHttpTransport">
                <label class="mcp-field mcp-field-full">
                  <span class="mcp-field-label">HTTP URL</span>
                  <el-input
                    v-model="draft.url"
                    :placeholder="draft.transport === 'sse' ? 'https://example.com/mcp/sse' : 'https://example.com/mcp'"
                  />
                </label>

                <label class="mcp-field">
                  <span class="mcp-field-label">认证方式</span>
                  <el-select v-model="draft.authType">
                    <el-option label="无认证" value="none" />
                    <el-option label="Bearer Token" value="bearer" />
                    <el-option label="Basic Auth" value="basic" />
                    <el-option label="Header" value="header" />
                    <el-option label="Query" value="query" />
                  </el-select>
                </label>

                <template v-if="draft.authType === 'bearer'">
                  <label class="mcp-field">
                    <span class="mcp-field-label">Scheme</span>
                    <el-input v-model="draft.bearerScheme" placeholder="默认 Bearer，可改成 Token 等" />
                  </label>
                  <label class="mcp-field">
                    <span class="mcp-field-label">Token</span>
                    <el-input v-model="draft.bearerToken" type="password" show-password placeholder="Bearer Token" />
                  </label>
                </template>

                <template v-else-if="draft.authType === 'basic'">
                  <label class="mcp-field">
                    <span class="mcp-field-label">用户名</span>
                    <el-input v-model="draft.authUsername" placeholder="basic auth 用户名" />
                  </label>
                  <label class="mcp-field">
                    <span class="mcp-field-label">密码</span>
                    <el-input v-model="draft.authPassword" type="password" show-password placeholder="Basic Auth 密码" />
                  </label>
                </template>

                <template v-else-if="draft.authType === 'header'">
                  <label class="mcp-field">
                    <span class="mcp-field-label">Header 名称</span>
                    <el-input v-model="draft.authHeaderName" placeholder="例如 Authorization / X-Api-Key" />
                  </label>
                  <label class="mcp-field">
                    <span class="mcp-field-label">Header 值</span>
                    <el-input v-model="draft.authHeaderValue" type="password" show-password placeholder="Header 值" />
                  </label>
                </template>

                <template v-else-if="draft.authType === 'query'">
                  <label class="mcp-field">
                    <span class="mcp-field-label">Query 参数名</span>
                    <el-input v-model="draft.authQueryName" placeholder="例如 api_key / token" />
                  </label>
                  <label class="mcp-field">
                    <span class="mcp-field-label">Query 参数值</span>
                    <el-input v-model="draft.authQueryValue" type="password" show-password placeholder="Query 参数值" />
                  </label>
                </template>

                <div class="mcp-secret-block mcp-field-full">
                  <div class="mcp-secret-head">
                    <div>
                      <div class="mcp-field-label">自定义 Headers</div>
                      <div class="mcp-field-help">按 `KEY: VALUE` 每行一条。保存时会和认证信息一起发给 MCP 服务。</div>
                    </div>
                    <el-select v-model="draft.customHeadersMode" class="mcp-secret-mode-select">
                      <el-option label="保留当前" value="keep" />
                      <el-option label="整体替换" value="replace" />
                      <el-option label="清空" value="clear" />
                    </el-select>
                  </div>

                  <el-input
                    v-if="draft.customHeadersMode === 'replace'"
                    v-model="draft.customHeadersText"
                    type="textarea"
                    :rows="5"
                    placeholder="例如&#10;X-Workspace: markflow&#10;X-Trace-Id: local-dev"
                  />
                </div>
              </template>

              <template v-else>
                <label class="mcp-field">
                  <span class="mcp-field-label">STDIO Command</span>
                  <el-input v-model="draft.command" placeholder="例如 npx / uvx / node / python" />
                </label>

                <label class="mcp-field">
                  <span class="mcp-field-label">允许范围</span>
                  <div class="mcp-field-inline">
                    <span class="mcp-field-help">
                      {{ runtimeCapabilities.stdioAllowedCommands.length ? `当前允许：${runtimeCapabilities.stdioAllowedCommands.join('、')}` : '当前未配置命令白名单' }}
                    </span>
                  </div>
                </label>

                <label class="mcp-field mcp-field-full">
                  <span class="mcp-field-label">STDIO Args</span>
                  <el-input
                    v-model="draft.argsText"
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
                    <el-select v-model="draft.stdioEnvMode" class="mcp-secret-mode-select">
                      <el-option label="保留当前" value="keep" />
                      <el-option label="整体替换" value="replace" />
                      <el-option label="清空" value="clear" />
                    </el-select>
                  </div>

                  <el-input
                    v-if="draft.stdioEnvMode === 'replace'"
                    v-model="draft.stdioEnvText"
                    type="textarea"
                    :rows="5"
                    placeholder="例如&#10;OPENAI_API_KEY=xxxx&#10;GITHUB_TOKEN=yyyy"
                  />
                </div>
              </template>
            </div>

            <div class="mcp-status-card">
              <div class="mcp-status-row"><span class="mcp-status-label">状态</span><span class="mcp-status-value">{{ mcpStatusLabel(draft.lastStatus) }}</span></div>
              <div class="mcp-status-row"><span class="mcp-status-label">配置版本</span><span class="mcp-status-value">{{ draft.configVersion || '-' }}</span></div>
              <div class="mcp-status-row"><span class="mcp-status-label">最近同步</span><span class="mcp-status-value">{{ formatOptionalTimestamp(draft.lastSyncAt) }}</span></div>
              <div v-if="draft.lastError" class="mcp-status-error">{{ draft.lastError }}</div>
            </div>

            <div class="mcp-snapshot-card">
              <el-tabs v-model="activeSnapshotTab" class="mcp-snapshot-tabs">
                <el-tab-pane label="Tools" name="tools" />
                <el-tab-pane label="Resources" name="resources" />
                <el-tab-pane label="Prompts" name="prompts" />
              </el-tabs>
              <div v-if="activeSnapshotEntries.length" class="mcp-snapshot-list">
                <article v-for="(entry, index) in activeSnapshotEntries" :key="`${activeSnapshotTab}-${index}`" class="mcp-snapshot-item">
                  <div class="mcp-snapshot-title">{{ mcpSnapshotEntryTitle(entry, index) }}</div>
                  <pre class="mcp-snapshot-raw">{{ mcpSnapshotEntryDetail(entry) }}</pre>
                </article>
              </div>
              <div v-else class="provider-hint">还没有能力快照。可以直接测试当前表单参数，确认没问题后再保存。</div>
            </div>
          </section>
        </div>
      </el-tab-pane>

      <el-tab-pane label="Skills" name="skills">
        <section class="skills-panel" v-loading="skillsLoading">
          <div class="skills-panel-head">
            <div>
              <div class="skills-title">Skills</div>
              <p>管理 skills 目录下的技能手册，模型通过 skills_list / skills_view 按需加载。</p>
            </div>
            <el-button class="resource-refresh-btn" @click="loadSkills">刷新</el-button>
          </div>
          <div v-if="pagedSkills.length" class="skill-cards">
            <article v-for="skill in pagedSkills" :key="skill.name" class="skill-card">
              <div class="skill-card-head">
                <el-tooltip :content="skill.name" placement="top"><strong>{{ skill.display_name || skill.name }}</strong></el-tooltip>
                <el-tooltip :disabled="skill.available !== false || !skill.fail_reason" :content="skill.fail_reason || ''" placement="top">
                  <el-tag size="small" :type="skillAvailabilityType(skill)" effect="plain">{{ skillAvailabilityText(skill) }}</el-tag>
                </el-tooltip>
              </div>
              <div class="skill-tags">
                <el-tag size="small" type="success" effect="plain">SKILL.md</el-tag>
                <el-tag size="small" effect="plain">{{ skill.file_count || 0 }} files</el-tag>
                <el-tooltip :content="skill.relative_path || skill.name" placement="top">
                  <el-tag size="small" effect="plain" class="skill-path-tag">{{ skill.relative_path || skill.name }}</el-tag>
                </el-tooltip>
              </div>
              <div class="skill-card-body">
                <p>{{ skill.description || '暂无描述' }}</p>
                <span>{{ skill.path }}</span>
              </div>
              <div class="skill-card-footer">
                <el-button class="resource-card-btn" size="small" @click="openSkillDialog(skill, 'view')">查看</el-button>
                <el-button class="resource-card-btn primary" size="small" type="primary" plain @click="openSkillDialog(skill, 'edit')">编辑</el-button>
                <el-button class="resource-card-btn danger" size="small" type="danger" plain @click="removeSkill(skill)">删除</el-button>
              </div>
            </article>
          </div>
          <div v-else class="provider-hint">还没有 Skills。请在配置的 skills_root_dir 下放置包含 SKILL.md 的技能目录。</div>
          <div class="skills-pagination">
            <el-pagination
              v-model:current-page="skillsPage"
              v-model:page-size="skillsPageSize"
              background
              layout="prev, pager, next, sizes, total"
              :page-sizes="[6, 8, 12, 16, 20]"
              :total="skills.length"
            />
          </div>
        </section>
      </el-tab-pane>
    </el-tabs>

    <template v-if="activeResourceTab === 'mcp'" #footer>
      <div class="resource-footer-actions">
        <el-button class="resource-footer-btn" @click="startCreate">新增</el-button>
        <el-button class="resource-footer-btn" :disabled="!draft.id" @click="duplicate">复制</el-button>
        <el-button class="resource-footer-btn danger" type="danger" plain :disabled="!draft.id" @click="removeServer(draft.id)">删除</el-button>
        <el-button class="resource-footer-btn" :loading="testing" @click="testConnection">测试连接</el-button>
        <el-button class="resource-footer-btn" :loading="refreshing" @click="refreshCapabilities">刷新能力</el-button>
        <el-button class="resource-footer-btn primary" type="primary" :loading="saving" @click="saveDraft">保存</el-button>
      </div>
    </template>
  </el-dialog>

  <el-dialog v-model="skillDialogVisible" :title="skillDialogTitle" width="980px" class="skill-dialog" append-to-body align-center>
    <div class="skill-dialog-body" v-loading="skillFileLoading">
      <aside class="skill-file-panel">
        <div class="skill-panel-title">Skill 文件</div>
        <el-tree
          class="skill-file-tree"
          :data="skillTreeData"
          :props="skillTreeProps()"
          node-key="path"
          default-expand-all
          :expand-on-click-node="false"
          :highlight-current="true"
          :node-class-name="skillNodeClass"
          @node-click="selectSkillFile"
        />
      </aside>
      <section class="skill-content-panel">
        <div class="skill-content-head">
          <div>
            <div class="skill-panel-title">文件内容</div>
            <p>{{ selectedSkillFile?.path || '请选择文件' }}</p>
          </div>
          <el-tag v-if="selectedSkillFile?.path" size="small" effect="plain">path</el-tag>
        </div>
        <el-input
          v-if="selectedSkillFile?.kind === 'file'"
          v-model="skillFileContent"
          type="textarea"
          resize="none"
          :readonly="skillDialogMode !== 'edit'"
          class="skill-content-editor"
        />
        <div v-else class="skill-empty-state">请选择 Skill 文件</div>
      </section>
    </div>
    <template #footer>
      <el-button @click="skillDialogVisible = false">取消</el-button>
      <el-button v-if="skillDialogMode === 'edit'" type="primary" :disabled="!canSaveSkillFile" :loading="skillFileSaving" @click="saveSkillFile">保存</el-button>
    </template>
  </el-dialog>
</template>

<style scoped>
.external-resource-tabs {
  height: min(68vh, 680px);
  min-height: 0;
}
.external-resource-tabs :deep(.el-tabs__content) {
  height: calc(min(68vh, 680px) - 54px);
  overflow: hidden;
}
.external-resource-tabs :deep(.el-tab-pane) {
  height: 100%;
}
.provider-manager {
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
  gap: 12px;
  height: 100%;
  min-height: 0;
  overflow: hidden;
}
.provider-list-pane,
.provider-editor {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0;
}
.provider-list,
.provider-editor,
.mcp-snapshot-list,
.skill-cards,
.skill-file-tree,
.skill-content-panel {
  scrollbar-width: thin;
  scrollbar-color: rgba(111, 154, 79, 0.58) rgba(122, 147, 91, 0.12);
}
.provider-list {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
  overflow: auto;
}
.provider-editor {
  overflow: auto;
  padding-right: 4px;
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
.provider-item-name,
.skill-card-head strong {
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
.provider-item-meta,
.provider-hint,
.mcp-settings-desc,
.mcp-runtime-hint,
.mcp-field-help,
.skill-card-body span,
.skills-panel-head p,
.skill-content-head p {
  font-size: 12px;
  line-height: 1.6;
  color: #708067;
}
.mcp-item-error {
  font-size: 11px;
  line-height: 1.5;
  color: #b45f52;
  word-break: break-word;
}
.mcp-settings-card,
.mcp-status-card,
.mcp-snapshot-card,
.mcp-secret-block,
.skill-card {
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
.mcp-settings-title,
.skills-title,
.skill-panel-title {
  font-size: 13px;
  font-weight: 700;
  color: #24311f;
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
.mcp-secret-block,
.mcp-status-card,
.mcp-snapshot-card {
  padding: 10px 12px;
}
.mcp-secret-head,
.mcp-status-row,
.skills-panel-head,
.skill-card-head,
.skill-card-footer,
.skill-content-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}
.mcp-secret-mode-select {
  width: 136px;
  flex-shrink: 0;
}
.mcp-status-card {
  gap: 8px;
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
.skills-panel {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 12px;
}
.skill-cards {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 12px;
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding-right: 4px;
}
.skill-card {
  display: flex;
  min-height: 190px;
  flex-direction: column;
  gap: 10px;
  padding: 12px;
}
.skill-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.skill-path-tag {
  max-width: 180px;
}
.skill-path-tag :deep(.el-tag__content) {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.skill-card-body {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 6px;
  min-height: 0;
}
.skill-card-body p {
  margin: 0;
  color: #51604a;
  font-size: 12px;
  line-height: 1.6;
}
.skills-pagination {
  display: flex;
  justify-content: flex-end;
}
</style>

<style>
.external-resource-dialog .el-dialog {
  width: min(980px, calc(100vw - 36px)) !important;
  max-height: calc(100vh - 28px);
  display: flex;
  flex-direction: column;
}
.external-resource-dialog .el-dialog__header {
  margin-right: 0;
  padding-bottom: 8px;
  border-bottom: 1px solid rgba(122, 147, 91, 0.1);
}
.external-resource-dialog .el-dialog__body {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  padding-top: 12px;
  padding-bottom: 12px;
}
.external-resource-dialog .el-dialog__footer {
  flex-shrink: 0;
  padding-top: 10px;
  border-top: 1px solid rgba(122, 147, 91, 0.1);
  background: linear-gradient(180deg, rgba(251, 252, 247, 0.98), rgba(246, 248, 239, 0.96));
}
.skill-dialog .el-dialog {
  width: min(980px, calc(100vw - 36px)) !important;
}
.skill-dialog-body {
  display: grid;
  grid-template-columns: 260px minmax(0, 1fr);
  gap: 12px;
  height: min(66vh, 640px);
  min-height: 0;
}
.skill-file-panel,
.skill-content-panel {
  min-height: 0;
  border-radius: 14px;
  border: 1px solid rgba(122, 147, 91, 0.14);
  background: rgba(248, 250, 242, 0.78);
  padding: 12px;
}
.skill-file-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.skill-file-tree {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: transparent;
}
.skill-content-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.skill-content-editor {
  flex: 1;
  min-height: 0;
}
.skill-content-editor .el-textarea__inner {
  height: 100%;
  min-height: 100%;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  line-height: 1.6;
}
.skill-empty-state {
  display: flex;
  flex: 1;
  align-items: center;
  justify-content: center;
  color: #708067;
  font-size: 13px;
}
/* polished external resource manager */
.external-resource-tabs {
  --resource-accent: #16a36b;
  --resource-accent-deep: #0f8f5c;
  --resource-ink: #1f2a24;
  --resource-muted: #6c7a70;
  --resource-line: rgba(35, 72, 52, 0.10);
  --resource-soft: #f6faf2;
}
.external-resource-tabs :deep(.el-tabs__header) {
  margin: 0 0 16px;
}
.external-resource-tabs :deep(.el-tabs__nav-wrap::after) {
  display: none;
}
.external-resource-tabs :deep(.el-tabs__nav) {
  display: inline-flex;
  gap: 4px;
  padding: 4px;
  border: 1px solid var(--resource-line);
  border-radius: 999px;
  background: linear-gradient(180deg, #f9fbf6, #eef5e9);
  box-shadow: inset 0 1px 0 rgba(255,255,255,.85);
}
.external-resource-tabs :deep(.el-tabs__item) {
  height: 34px;
  padding: 0 18px;
  border-radius: 999px;
  color: #5d6c62;
  font-size: 13px;
  font-weight: 700;
  letter-spacing: .01em;
}
.external-resource-tabs :deep(.el-tabs__item.is-active) {
  color: var(--resource-accent-deep);
  background: #ffffff;
  box-shadow: 0 8px 18px rgba(26, 92, 58, .10), inset 0 0 0 1px rgba(22, 163, 107, .12);
}
.external-resource-tabs :deep(.el-tabs__active-bar) {
  display: none;
}
.skills-panel-head {
  align-items: center;
  padding: 16px 18px;
  border: 1px solid var(--resource-line);
  border-radius: 18px;
  background:
    radial-gradient(circle at 0 0, rgba(78, 185, 123, .16), transparent 34%),
    linear-gradient(135deg, rgba(250, 253, 247, .98), rgba(239, 247, 234, .88));
  box-shadow: 0 12px 30px rgba(43, 72, 49, .06);
}
.skills-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 17px;
  letter-spacing: .01em;
}
.skills-title::before {
  content: '';
  width: 9px;
  height: 9px;
  border-radius: 999px;
  background: var(--resource-accent);
  box-shadow: 0 0 0 5px rgba(22, 163, 107, .12);
}
.skills-panel-head p {
  margin: 4px 0 0;
  color: var(--resource-muted);
}
.skill-cards {
  grid-template-columns: repeat(auto-fill, minmax(292px, 332px));
  grid-auto-rows: auto;
  align-items: start;
  align-content: start;
  gap: 14px;
  padding: 2px 6px 8px 2px;
}
.skill-card {
  position: relative;
  min-height: 236px;
  max-height: 280px;
  padding: 16px;
  border-radius: 18px;
  border: 1px solid rgba(49, 96, 63, .12);
  background:
    linear-gradient(180deg, rgba(255,255,255,.92), rgba(249,252,245,.96)),
    radial-gradient(circle at 100% 0, rgba(22, 163, 107, .13), transparent 34%);
  box-shadow: 0 16px 34px rgba(38, 67, 44, .08);
  overflow: hidden;
  transition: transform .18s ease, box-shadow .18s ease, border-color .18s ease;
}
.skill-card::after {
  content: '';
  position: absolute;
  inset: 0 0 auto;
  height: 3px;
  background: linear-gradient(90deg, #16a36b, #8dcc66);
  opacity: .88;
}
.skill-card:hover {
  transform: translateY(-2px);
  border-color: rgba(22, 163, 107, .26);
  box-shadow: 0 22px 44px rgba(38, 67, 44, .12);
}
.skill-card-head {
  align-items: center;
}
.skill-card-head strong {
  display: block;
  max-width: 190px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 15px;
  color: var(--resource-ink);
}
.skill-tags {
  gap: 7px;
}
.skill-tags :deep(.el-tag) {
  border-radius: 999px;
  font-weight: 700;
  background: rgba(255,255,255,.75);
}
.skill-card-body {
  padding: 10px 0 4px;
}
.skill-card-body p {
  display: -webkit-box;
  min-height: 58px;
  max-height: 78px;
  overflow: hidden;
  color: #445246;
  font-size: 13px;
  line-height: 1.55;
  -webkit-line-clamp: 4;
  -webkit-box-orient: vertical;
}
.skill-card-body span {
  display: block;
  margin-top: 8px;
  padding: 8px 10px;
  border-radius: 12px;
  background: rgba(42, 64, 45, .045);
  color: #758377;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 11px;
  line-height: 1.45;
  word-break: break-all;
}
.skill-card-footer {
  margin-top: auto;
  padding-top: 10px;
  border-top: 1px solid rgba(49, 96, 63, .08);
  align-items: center;
}
.resource-footer-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}
.resource-card-btn,
.resource-footer-btn,
.resource-refresh-btn {
  border-radius: 999px !important;
  font-weight: 700 !important;
}
.resource-card-btn.primary,
.resource-footer-btn.primary {
  box-shadow: 0 10px 20px rgba(22, 163, 107, .18);
}
.resource-card-btn.danger,
.resource-footer-btn.danger {
  color: #c34f4f !important;
}
.provider-item,
.mcp-settings-card,
.mcp-status-card,
.mcp-snapshot-card,
.mcp-secret-block {
  border-radius: 18px;
  box-shadow: 0 12px 28px rgba(38, 67, 44, .06);
}
.provider-item {
  transition: transform .18s ease, box-shadow .18s ease, border-color .18s ease;
}
.provider-item:hover {
  transform: translateY(-1px);
  border-color: rgba(22, 163, 107, .22);
  box-shadow: 0 16px 34px rgba(38, 67, 44, .10);
}
@media (max-width: 768px) {
  .provider-manager,
  .skill-dialog-body {
    grid-template-columns: minmax(0, 1fr);
  }
  .skill-cards {
    grid-template-columns: minmax(0, 1fr);
  }
}

.external-resource-dialog .el-dialog {
  border-radius: 22px;
  overflow: hidden;
  background: linear-gradient(180deg, #ffffff, #fbfcf7);
  box-shadow: 0 28px 90px rgba(22, 35, 28, .28);
}
.external-resource-dialog .el-dialog__title {
  color: #1f2a24;
  font-weight: 800;
  letter-spacing: .01em;
}
.external-resource-dialog .el-dialog__body {
  background: linear-gradient(180deg, #ffffff 0%, #fbfcf7 100%);
}
.external-resource-dialog .el-dialog__footer {
  padding: 14px 24px 16px;
  background: rgba(250, 252, 247, .96);
  backdrop-filter: blur(10px);
}
.external-resource-dialog .el-button {
  min-width: 72px;
  height: 34px;
  border-radius: 999px;
  font-weight: 700;
}
.external-resource-dialog .el-button--primary {
  border-color: #16a36b;
  background: linear-gradient(135deg, #19ae72, #0f8f5c);
}
.external-resource-dialog .el-button--primary.is-plain {
  color: #0f8f5c;
  border-color: rgba(22, 163, 107, .30);
  background: rgba(22, 163, 107, .08);
}
.external-resource-dialog .el-button--danger.is-plain {
  border-color: rgba(220, 88, 88, .22);
  background: rgba(220, 88, 88, .07);
}
.external-resource-dialog .el-input__wrapper,
.external-resource-dialog .el-textarea__inner,
.external-resource-dialog .el-select__wrapper {
  border-radius: 12px;
  box-shadow: 0 0 0 1px rgba(45, 82, 55, .10) inset;
}
.external-resource-dialog .el-pagination.is-background .el-pager li.is-active {
  background: #16a36b;
}
.skill-dialog .el-dialog {
  border-radius: 20px;
  overflow: hidden;
}

/* external resource theme controls: neutral buttons + green active states */
.external-resource-dialog {
  --resource-primary: #1b9a63;
  --resource-primary-hover: #168553;
  --resource-primary-soft: #edf8f1;
  --resource-border: #dce6d8;
  --resource-text: #24311f;
  --resource-muted: #697768;
  --resource-danger: #c6534b;
  --resource-danger-soft: #fff2f0;
}
.external-resource-dialog .el-tabs__nav-wrap::after {
  height: 1px;
  background: var(--resource-border);
}
.external-resource-dialog .el-tabs__item {
  color: #34423a;
  font-weight: 650;
}
.external-resource-dialog .el-tabs__item.is-active,
.external-resource-dialog .el-tabs__item:hover {
  color: var(--resource-primary);
}
.external-resource-dialog .el-tabs__active-bar {
  height: 2px;
  border-radius: 999px;
  background: var(--resource-primary);
}
.external-resource-dialog .el-button {
  min-width: 64px;
  height: 32px;
  padding: 0 14px;
  border-radius: 10px;
  border-color: #d8e1d4;
  background: #fff;
  color: #405045;
  font-weight: 650;
  box-shadow: none;
}
.external-resource-dialog .el-button:hover,
.external-resource-dialog .el-button:focus {
  color: var(--resource-primary);
  border-color: rgba(27, 154, 99, .38);
  background: var(--resource-primary-soft);
  outline: none;
  box-shadow: none;
}
.external-resource-dialog .el-button--primary {
  border-color: var(--resource-primary);
  background: var(--resource-primary);
  color: #fff;
  box-shadow: none;
}
.external-resource-dialog .el-button--primary:hover,
.external-resource-dialog .el-button--primary:focus {
  border-color: var(--resource-primary-hover);
  background: var(--resource-primary-hover);
  color: #fff;
  box-shadow: none;
}
.external-resource-dialog .el-button--primary.is-plain {
  border-color: rgba(27, 154, 99, .36);
  background: var(--resource-primary-soft);
  color: var(--resource-primary);
}
.external-resource-dialog .el-button--danger,
.external-resource-dialog .el-button--danger.is-plain {
  border-color: rgba(198, 83, 75, .28);
  background: var(--resource-danger-soft);
  color: var(--resource-danger);
}
.external-resource-dialog .el-button--danger:hover,
.external-resource-dialog .el-button--danger:focus,
.external-resource-dialog .el-button--danger.is-plain:hover,
.external-resource-dialog .el-button--danger.is-plain:focus {
  border-color: rgba(198, 83, 75, .45);
  background: #ffe9e6;
  color: #aa4039;
  box-shadow: none;
}
.external-resource-dialog .el-button.is-disabled,
.external-resource-dialog .el-button.is-disabled:hover {
  border-color: #e5e9e2;
  background: #f7f8f5;
  color: #a7b0a6;
}
.external-resource-dialog .resource-refresh-btn {
  background: #fff;
  color: #405045;
}
.external-resource-dialog .skill-card-footer .el-button + .el-button,
.external-resource-dialog .resource-footer-actions .el-button + .el-button {
  margin-left: 0;
}
.external-resource-dialog .el-pagination.is-background .btn-prev,
.external-resource-dialog .el-pagination.is-background .btn-next,
.external-resource-dialog .el-pagination.is-background .el-pager li {
  min-width: 32px;
  height: 32px;
  border-radius: 9px;
  border: 1px solid #dfe7dc;
  background: #fff;
  color: #607064;
  box-shadow: none;
}
.external-resource-dialog .el-pagination.is-background .btn-prev:hover,
.external-resource-dialog .el-pagination.is-background .btn-next:hover,
.external-resource-dialog .el-pagination.is-background .el-pager li:hover {
  color: var(--resource-primary);
  border-color: rgba(27, 154, 99, .36);
  background: var(--resource-primary-soft);
}
.external-resource-dialog .el-pagination.is-background .el-pager li.is-active {
  border-color: var(--resource-primary);
  background: var(--resource-primary);
  color: #fff;
}
.external-resource-dialog .el-pagination .el-select .el-select__wrapper,
.external-resource-dialog .el-pagination .el-input__wrapper {
  height: 32px;
  border-radius: 9px;
  background: #fff;
  box-shadow: 0 0 0 1px #dfe7dc inset;
}
.external-resource-dialog .el-pagination__total {
  color: #607064;
}
</style>

<style>
/* Element UI / Element Plus neutral style reset for external resource manager */
.external-resource-dialog {
  --resource-primary: var(--el-color-primary);
  --resource-primary-hover: var(--el-color-primary-light-3);
  --resource-primary-soft: var(--el-color-primary-light-9);
  --resource-border: var(--el-border-color-light);
  --resource-text: var(--el-text-color-primary);
  --resource-muted: var(--el-text-color-secondary);
  --resource-danger: var(--el-color-danger);
  --resource-danger-soft: var(--el-color-danger-light-9);
}
.external-resource-dialog .el-dialog {
  border-radius: 4px !important;
  background: var(--el-bg-color) !important;
  box-shadow: var(--el-box-shadow-dark) !important;
}
.external-resource-dialog .el-dialog__header {
  border-bottom: 1px solid var(--el-border-color-lighter) !important;
}
.external-resource-dialog .el-dialog__title {
  color: var(--el-text-color-primary) !important;
  font-weight: 600 !important;
}
.external-resource-dialog .el-dialog__body,
.external-resource-dialog .el-dialog__footer {
  background: var(--el-bg-color) !important;
  backdrop-filter: none !important;
}
.external-resource-dialog .external-resource-tabs .el-tabs__header {
  margin: 0 0 16px !important;
}
.external-resource-dialog .external-resource-tabs .el-tabs__nav-wrap::after {
  display: block !important;
  height: 2px !important;
  background: var(--el-border-color-light) !important;
}
.external-resource-dialog .external-resource-tabs .el-tabs__nav {
  display: flex !important;
  gap: 0 !important;
  padding: 0 !important;
  border: 0 !important;
  border-radius: 0 !important;
  background: transparent !important;
  box-shadow: none !important;
}
.external-resource-dialog .external-resource-tabs .el-tabs__item {
  height: 40px !important;
  padding: 0 20px !important;
  border-radius: 0 !important;
  background: transparent !important;
  box-shadow: none !important;
  color: var(--el-text-color-primary) !important;
  font-size: 14px !important;
  font-weight: 500 !important;
}
.external-resource-dialog .external-resource-tabs .el-tabs__item.is-active,
.external-resource-dialog .external-resource-tabs .el-tabs__item:hover {
  color: var(--el-color-primary) !important;
}
.external-resource-dialog .external-resource-tabs .el-tabs__active-bar {
  display: block !important;
  height: 2px !important;
  border-radius: 0 !important;
  background: var(--el-color-primary) !important;
}
.external-resource-dialog .skills-panel-head {
  padding: 0 0 12px !important;
  border: 0 !important;
  border-radius: 0 !important;
  background: transparent !important;
  box-shadow: none !important;
}
.external-resource-dialog .skills-title {
  font-size: 16px !important;
  font-weight: 600 !important;
  color: var(--el-text-color-primary) !important;
}
.external-resource-dialog .skills-title::before {
  display: none !important;
}
.external-resource-dialog .skills-panel-head p {
  color: var(--el-text-color-secondary) !important;
}
.external-resource-dialog .skill-cards {
  grid-template-columns: repeat(auto-fill, minmax(300px, 320px)) !important;
  gap: 16px !important;
  padding: 0 4px 8px 0 !important;
}
.external-resource-dialog .skill-card {
  min-height: 248px !important;
  max-height: none !important;
  padding: 16px !important;
  border: 1px solid var(--el-border-color-light) !important;
  border-radius: 4px !important;
  background: var(--el-bg-color) !important;
  box-shadow: var(--el-box-shadow-light) !important;
  transition: box-shadow .2s ease, border-color .2s ease !important;
}
.external-resource-dialog .skill-card::after {
  display: none !important;
}
.external-resource-dialog .skill-card:hover {
  transform: none !important;
  border-color: var(--el-border-color) !important;
  box-shadow: var(--el-box-shadow) !important;
}
.external-resource-dialog .skill-card-head strong {
  color: var(--el-text-color-primary) !important;
  font-size: 14px !important;
  font-weight: 600 !important;
}
.external-resource-dialog .skill-tags .el-tag {
  border-radius: 4px !important;
  font-weight: 400 !important;
  background: var(--el-bg-color) !important;
}
.external-resource-dialog .skill-card-body p {
  color: var(--el-text-color-regular) !important;
  font-size: 13px !important;
  line-height: 1.55 !important;
}
.external-resource-dialog .skill-card-body span {
  padding: 8px !important;
  border: 1px solid var(--el-border-color-lighter) !important;
  border-radius: 4px !important;
  background: var(--el-fill-color-lighter) !important;
  color: var(--el-text-color-secondary) !important;
}
.external-resource-dialog .skill-card-footer {
  border-top: 1px solid var(--el-border-color-lighter) !important;
}
.external-resource-dialog .provider-item,
.external-resource-dialog .mcp-settings-card,
.external-resource-dialog .mcp-status-card,
.external-resource-dialog .mcp-snapshot-card,
.external-resource-dialog .mcp-secret-block {
  border: 1px solid var(--el-border-color-light) !important;
  border-radius: 4px !important;
  background: var(--el-bg-color) !important;
  box-shadow: none !important;
}
.external-resource-dialog .provider-item:hover {
  transform: none !important;
  border-color: var(--el-border-color) !important;
  box-shadow: var(--el-box-shadow-lighter) !important;
}
.external-resource-dialog .provider-item.active {
  border-color: var(--el-color-primary-light-5) !important;
  background: var(--el-color-primary-light-9) !important;
}
.external-resource-dialog .el-button,
.external-resource-dialog .resource-card-btn,
.external-resource-dialog .resource-footer-btn,
.external-resource-dialog .resource-refresh-btn {
  min-width: auto !important;
  height: 32px !important;
  padding: 8px 15px !important;
  border-radius: 4px !important;
  font-weight: 500 !important;
  box-shadow: none !important;
}
.external-resource-dialog .el-button--primary {
  border-color: var(--el-color-primary) !important;
  background: var(--el-color-primary) !important;
  color: #fff !important;
}
.external-resource-dialog .el-button--primary:hover,
.external-resource-dialog .el-button--primary:focus {
  border-color: var(--el-color-primary-light-3) !important;
  background: var(--el-color-primary-light-3) !important;
  color: #fff !important;
}
.external-resource-dialog .el-button--primary.is-plain {
  border-color: var(--el-color-primary-light-5) !important;
  background: var(--el-color-primary-light-9) !important;
  color: var(--el-color-primary) !important;
}
.external-resource-dialog .el-button--primary.is-plain:hover,
.external-resource-dialog .el-button--primary.is-plain:focus {
  border-color: var(--el-color-primary) !important;
  background: var(--el-color-primary) !important;
  color: #fff !important;
}
.external-resource-dialog .el-button--danger,
.external-resource-dialog .el-button--danger.is-plain {
  border-color: var(--el-color-danger-light-5) !important;
  background: var(--el-color-danger-light-9) !important;
  color: var(--el-color-danger) !important;
}
.external-resource-dialog .el-button--danger:hover,
.external-resource-dialog .el-button--danger:focus,
.external-resource-dialog .el-button--danger.is-plain:hover,
.external-resource-dialog .el-button--danger.is-plain:focus {
  border-color: var(--el-color-danger) !important;
  background: var(--el-color-danger) !important;
  color: #fff !important;
}
.external-resource-dialog .el-button.is-disabled,
.external-resource-dialog .el-button.is-disabled:hover {
  border-color: var(--el-border-color-lighter) !important;
  background: var(--el-fill-color-light) !important;
  color: var(--el-text-color-placeholder) !important;
}
.external-resource-dialog .el-pagination.is-background .btn-prev,
.external-resource-dialog .el-pagination.is-background .btn-next,
.external-resource-dialog .el-pagination.is-background .el-pager li {
  min-width: 32px !important;
  height: 32px !important;
  border: 0 !important;
  border-radius: 2px !important;
  background: var(--el-fill-color-light) !important;
  color: var(--el-text-color-regular) !important;
  box-shadow: none !important;
}
.external-resource-dialog .el-pagination.is-background .btn-prev:hover,
.external-resource-dialog .el-pagination.is-background .btn-next:hover,
.external-resource-dialog .el-pagination.is-background .el-pager li:hover {
  color: var(--el-color-primary) !important;
}
.external-resource-dialog .el-pagination.is-background .el-pager li.is-active {
  background: var(--el-color-primary) !important;
  color: #fff !important;
}
.external-resource-dialog .el-pagination .el-select .el-select__wrapper,
.external-resource-dialog .el-pagination .el-input__wrapper,
.external-resource-dialog .el-input__wrapper,
.external-resource-dialog .el-textarea__inner,
.external-resource-dialog .el-select__wrapper {
  border-radius: 4px !important;
  box-shadow: 0 0 0 1px var(--el-border-color) inset !important;
}
.skill-dialog .el-dialog {
  border-radius: 4px !important;
}
</style>
