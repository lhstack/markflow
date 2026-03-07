<template>
  <el-dialog v-model="visible" title="用户管理" width="980px" append-to-body destroy-on-close>
    <div class="user-admin-shell">
      <div class="toolbar">
        <div class="toolbar-search">
          <el-input v-model="keyword" placeholder="搜索用户名" clearable />
          <span class="toolbar-count">共 {{ filteredUsers.length }} 个用户</span>
        </div>
        <div class="toolbar-actions">
          <el-button type="primary" @click="openCreateUser">新增用户</el-button>
          <el-button :loading="loading" @click="fetchUsers">刷新</el-button>
        </div>
      </div>

      <div class="user-panel">
        <el-table
          v-loading="loading"
          :data="filteredUsers"
          class="user-table"
          height="520"
          table-layout="fixed"
          empty-text=""
        >
          <el-table-column label="用户" width="260" header-align="left" align="left">
            <template #default="{ row }">
              <div class="user-cell">
                <div class="user-avatar">
                  <img v-if="row.avatar" :src="row.avatar" />
                  <span v-else>{{ row.username?.[0]?.toUpperCase() }}</span>
                </div>
                <div class="user-meta">
                  <div class="user-name">{{ row.username }}</div>
                  <div class="user-date">{{ row.created_at }}</div>
                </div>
              </div>
            </template>
          </el-table-column>

          <el-table-column label="状态" width="150" header-align="left" align="left">
            <template #default="{ row }">
              <div class="status-cell">
                <div class="switch-line">
                  <el-switch
                    :model-value="row.is_active"
                    :loading="Boolean(statusLoading[row.id])"
                    @change="(value: boolean | string | number) => updateStatus(row, Boolean(value))"
                  />
                  <span class="state-pill" :class="row.is_active ? 'active' : 'inactive'">
                    {{ row.is_active ? '已启用' : '已停用' }}
                  </span>
                </div>
              </div>
            </template>
          </el-table-column>

          <el-table-column label="2FA" width="150" header-align="left" align="left">
            <template #default="{ row }">
              <div class="twofa-cell">
                <div class="switch-line">
                  <el-switch
                    :model-value="row.totp_enabled"
                    :loading="Boolean(twofaLoading[row.id])"
                    @change="(value: boolean | string | number) => updateTwoFA(row, Boolean(value))"
                  />
                  <span class="state-pill" :class="row.has_totp_secret ? 'ready' : 'plain'">
                    {{ row.has_totp_secret ? '已初始化' : '未初始化' }}
                  </span>
                </div>
              </div>
            </template>
          </el-table-column>

          <el-table-column label="更新时间" width="180" header-align="left" align="left">
            <template #default="{ row }">
              <div class="updated-cell">
                <div class="updated-time">{{ row.updated_at }}</div>
              </div>
            </template>
          </el-table-column>

          <el-table-column
            label="操作"
            width="160"
            fixed="right"
            align="right"
            header-align="right"
            class-name="action-column"
          >
            <template #default="{ row }">
              <div class="action-cell">
                <el-button size="small" @click="openPasswordReset(row)">重置</el-button>
                <el-button size="small" type="danger" plain @click="deleteUser(row)">删除</el-button>
              </div>
            </template>
          </el-table-column>

          <template #empty>
            <div class="user-empty">
              <div class="user-empty-title">没有匹配的用户</div>
              <div class="user-empty-desc">调整搜索条件，或者直接创建一个新用户。</div>
            </div>
          </template>
        </el-table>
      </div>
    </div>

    <template #footer>
      <el-button @click="visible = false">关闭</el-button>
    </template>
  </el-dialog>

  <el-dialog v-model="passwordDialogVisible" title="重置密码" width="380px" append-to-body destroy-on-close>
    <div class="password-form">
      <div class="password-user">用户：{{ passwordTarget?.username }}</div>
      <el-input
        v-model="newPassword"
        type="password"
        show-password
        placeholder="请输入新密码（至少6位）"
        @keydown.enter="submitPasswordReset"
      />
    </div>
    <template #footer>
      <el-button @click="passwordDialogVisible = false">取消</el-button>
      <el-button type="primary" :loading="passwordLoading" @click="submitPasswordReset">确认</el-button>
    </template>
  </el-dialog>

  <el-dialog v-model="createDialogVisible" title="新增用户" width="400px" append-to-body destroy-on-close>
    <div class="password-form">
      <el-input v-model="createForm.username" placeholder="用户名（3-32位）" />
      <el-input
        v-model="createForm.password"
        type="password"
        show-password
        placeholder="初始密码（至少6位）"
        @keydown.enter="submitCreateUser"
      />
      <div class="create-status">
        <span>创建后立即启用</span>
        <el-switch v-model="createForm.is_active" />
      </div>
    </div>
    <template #footer>
      <el-button @click="createDialogVisible = false">取消</el-button>
      <el-button type="primary" :loading="createLoading" @click="submitCreateUser">创建</el-button>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import request from '@/utils/request'

interface ManagedUser {
  id: number
  username: string
  avatar?: string
  totp_enabled: boolean
  has_totp_secret: boolean
  is_active: boolean
  created_at: string
  updated_at: string
}

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()

const visible = ref(props.modelValue)
const loading = ref(false)
const keyword = ref('')
const users = ref<ManagedUser[]>([])
const statusLoading = ref<Record<number, boolean>>({})
const twofaLoading = ref<Record<number, boolean>>({})
const passwordDialogVisible = ref(false)
const passwordTarget = ref<ManagedUser | null>(null)
const newPassword = ref('')
const passwordLoading = ref(false)
const createDialogVisible = ref(false)
const createLoading = ref(false)
const createForm = ref({
  username: '',
  password: '',
  is_active: true,
})

const filteredUsers = computed(() => {
  const q = keyword.value.trim().toLowerCase()
  if (!q) return users.value
  return users.value.filter((user) => user.username.toLowerCase().includes(q))
})

function replaceUser(nextUser: ManagedUser) {
  users.value = users.value.map((user) => (user.id === nextUser.id ? nextUser : user))
}

async function fetchUsers() {
  loading.value = true
  try {
    const data = await request.get('/admin/users') as any
    users.value = data.users || []
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || '读取用户列表失败')
  } finally {
    loading.value = false
  }
}

async function updateStatus(user: ManagedUser, isActive: boolean) {
  statusLoading.value = { ...statusLoading.value, [user.id]: true }
  try {
    const data = await request.put(`/admin/users/${user.id}/status`, { is_active: isActive }) as any
    replaceUser(data.user)
    ElMessage.success(isActive ? '用户已启用' : '用户已停用')
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || '更新用户状态失败')
  } finally {
    statusLoading.value = { ...statusLoading.value, [user.id]: false }
  }
}

async function updateTwoFA(user: ManagedUser, enabled: boolean) {
  twofaLoading.value = { ...twofaLoading.value, [user.id]: true }
  try {
    const data = await request.put(`/admin/users/${user.id}/2fa`, { enabled }) as any
    replaceUser(data.user)
    ElMessage.success(enabled ? '已开启用户 2FA' : '已关闭用户 2FA')
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || '更新 2FA 失败')
  } finally {
    twofaLoading.value = { ...twofaLoading.value, [user.id]: false }
  }
}

function openPasswordReset(user: ManagedUser) {
  passwordTarget.value = user
  newPassword.value = ''
  passwordDialogVisible.value = true
}

function openCreateUser() {
  createForm.value = {
    username: '',
    password: '',
    is_active: true,
  }
  createDialogVisible.value = true
}

async function submitPasswordReset() {
  if (!passwordTarget.value) return
  if (newPassword.value.trim().length < 6) {
    ElMessage.warning('新密码至少需要 6 位')
    return
  }

  passwordLoading.value = true
  try {
    await request.put(`/admin/users/${passwordTarget.value.id}/password`, {
      new_password: newPassword.value,
    })
    ElMessage.success('密码已重置')
    passwordDialogVisible.value = false
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || '重置密码失败')
  } finally {
    passwordLoading.value = false
  }
}

async function submitCreateUser() {
  if (createForm.value.username.trim().length < 3) {
    ElMessage.warning('用户名至少需要 3 位')
    return
  }
  if (createForm.value.password.trim().length < 6) {
    ElMessage.warning('初始密码至少需要 6 位')
    return
  }

  createLoading.value = true
  try {
    const data = await request.post('/admin/users', {
      username: createForm.value.username,
      password: createForm.value.password,
      is_active: createForm.value.is_active,
    }) as any
    users.value = [data.user, ...users.value]
    ElMessage.success('用户已创建')
    createDialogVisible.value = false
  } catch (error: any) {
    ElMessage.error(error.response?.data?.error || '创建用户失败')
  } finally {
    createLoading.value = false
  }
}

async function deleteUser(user: ManagedUser) {
  try {
    await ElMessageBox.confirm(
      `确认删除用户「${user.username}」？该用户的项目、文档、分享和文件资源都会被永久删除。`,
      '删除用户',
      {
        type: 'warning',
        confirmButtonText: '删除',
        cancelButtonText: '取消',
        confirmButtonClass: 'el-button--danger',
      }
    )

    await request.delete(`/admin/users/${user.id}`)
    users.value = users.value.filter((item) => item.id !== user.id)
    ElMessage.success('用户已删除')
  } catch (error: any) {
    if (error === 'cancel' || error === 'close' || error?.message === 'cancel') return
    ElMessage.error(error.response?.data?.error || '删除用户失败')
  }
}

watch(() => props.modelValue, (value) => {
  visible.value = value
  if (value) {
    void fetchUsers()
  }
})

watch(visible, (value) => {
  emit('update:modelValue', value)
})
</script>

<style scoped>
.user-admin-shell {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.toolbar-search {
  display: flex;
  align-items: center;
  gap: 14px;
  flex: 1;
  min-width: 0;
}

.toolbar-search :deep(.el-input) {
  flex: 1;
}

.toolbar-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
}

.toolbar-count {
  flex-shrink: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--text3);
}

.user-panel {
  border: 1px solid rgba(31, 41, 55, 0.08);
  border-radius: 18px;
  background:
    linear-gradient(180deg, rgba(252, 253, 248, 0.98), rgba(248, 250, 244, 0.96));
  overflow: hidden;
}

.user-table {
  --el-table-border-color: rgba(31, 41, 55, 0.08);
  --el-table-header-bg-color: rgba(239, 243, 231, 0.92);
  --el-table-row-hover-bg-color: rgba(243, 247, 236, 0.88);
  --el-table-current-row-bg-color: rgba(243, 247, 236, 0.88);
  --el-table-bg-color: transparent;
  --el-table-tr-bg-color: transparent;
  --el-table-header-text-color: #6f7d68;
  --el-table-text-color: #24311e;
}

.user-table :deep(.el-table__inner-wrapper::before) {
  display: none;
}

.user-table :deep(.el-table__header-wrapper th) {
  height: 58px;
  background: rgba(239, 243, 231, 0.92);
  font-size: 12px;
  font-weight: 700;
  color: var(--text2);
}

.user-table :deep(.el-table__header-wrapper .cell) {
  font-size: 12px;
  font-weight: 700;
  padding: 0 20px;
}

.user-table :deep(.el-table__body td) {
  padding: 14px 0;
  background: transparent;
}

.user-table :deep(.el-table__body .cell) {
  overflow: hidden;
  padding: 0 20px;
}

.user-table :deep(.el-table__row) {
  background: transparent;
}

.user-table :deep(.el-table__fixed-right::before) {
  display: none;
}

.user-table :deep(.el-table-fixed-column--right) {
  background:
    linear-gradient(90deg, rgba(248, 250, 244, 0.2), rgba(248, 250, 244, 0.98) 14px);
}

.user-table :deep(.action-column .cell) {
  overflow: visible;
  padding-right: 20px;
}

.user-table :deep(.el-table__empty-block) {
  min-height: 280px;
}

.user-cell {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.user-avatar {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #6f9a4f, #3d7a4b);
  box-shadow: 0 8px 18px rgba(82, 115, 53, 0.18);
  color: #fff;
  font-size: 13px;
  font-weight: 700;
}

.user-avatar img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.user-meta {
  min-width: 0;
}

.user-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.user-date {
  font-size: 12px;
  color: var(--text3);
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.status-cell,
.twofa-cell {
  display: flex;
  align-items: center;
}

.switch-line {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: nowrap;
}

.state-pill {
  display: inline-flex;
  align-items: center;
  min-height: 24px;
  padding: 0 8px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.01em;
  line-height: 1;
  white-space: nowrap;
}

.state-pill.active {
  background: rgba(95, 145, 74, 0.14);
  color: #447031;
}

.state-pill.inactive {
  background: rgba(168, 113, 92, 0.14);
  color: #9d533e;
}

.state-pill.ready {
  background: rgba(68, 120, 140, 0.12);
  color: #2d6275;
}

.state-pill.plain {
  background: rgba(120, 131, 145, 0.12);
  color: #627081;
}

.updated-cell {
  display: flex;
  align-items: center;
  min-height: 32px;
  width: 100%;
}

.updated-time {
  font-size: 12px;
  font-weight: 500;
  color: var(--text2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.action-cell {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 6px;
  flex-wrap: nowrap;
  white-space: nowrap;
  width: 100%;
}

.action-cell :deep(.el-button) {
  margin-left: 0;
  min-width: 0;
  padding: 8px 10px;
}

.user-empty {
  padding: 52px 24px;
  text-align: center;
}

.user-empty-title {
  font-size: 16px;
  font-weight: 700;
  color: #22301d;
}

.user-empty-desc {
  margin-top: 8px;
  font-size: 13px;
  color: #74806f;
}

.password-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.create-status {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 13px;
  color: var(--text2);
}

.password-user {
  font-size: 13px;
  color: var(--text2);
}

@media (max-width: 980px) {
  .toolbar {
    flex-direction: column;
    align-items: stretch;
  }

  .toolbar-search {
    flex-direction: column;
    align-items: stretch;
  }

  .toolbar-count {
    text-transform: none;
    letter-spacing: 0;
  }
}
</style>
