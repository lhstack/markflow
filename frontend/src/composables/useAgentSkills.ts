import { computed, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'

import {
  deleteAgentSkill,
  fetchAgentSkillFiles,
  fetchAgentSkills,
  readAgentSkillFile,
  saveAgentSkillFile,
  type AgentSkillFileNode,
  type AgentSkillSummary,
} from '@/api/agentSkills'
import { logAgentPanelError } from '@/components/agent/debug'

export type AgentSkillDialogMode = 'view' | 'edit'

export function useAgentSkills() {
  const loading = ref(false)
  const fileLoading = ref(false)
  const fileSaving = ref(false)
  const skills = ref<AgentSkillSummary[]>([])
  const page = ref(1)
  const pageSize = ref(8)
  const dialogVisible = ref(false)
  const dialogMode = ref<AgentSkillDialogMode>('view')
  const selectedSkill = ref<AgentSkillSummary | null>(null)
  const fileTree = ref<AgentSkillFileNode | null>(null)
  const selectedFile = ref<AgentSkillFileNode | null>(null)
  const fileContent = ref('')

  const pagedSkills = computed(() => {
    const start = (page.value - 1) * pageSize.value
    return skills.value.slice(start, start + pageSize.value)
  })
  const treeData = computed(() => (fileTree.value ? [fileTree.value] : []))
  const dialogTitle = computed(() => {
    const action = dialogMode.value === 'edit' ? '编辑' : '查看'
    return selectedSkill.value ? `${action} ${selectedSkill.value.name}` : action
  })
  const canSaveFile = computed(() => dialogMode.value === 'edit' && selectedFile.value?.kind === 'file')

  async function loadSkills() {
    loading.value = true
    try {
      const data = await fetchAgentSkills()
      skills.value = data.skills || []
      const maxPage = Math.max(1, Math.ceil(skills.value.length / pageSize.value))
      if (page.value > maxPage) page.value = maxPage
    } catch (error: any) {
      logAgentPanelError('load_agent_skills', error)
      ElMessage.error(error.response?.data?.error || error.message || '加载 Skills 失败')
    } finally {
      loading.value = false
    }
  }

  function findFile(node: AgentSkillFileNode | null, path: string): AgentSkillFileNode | null {
    if (!node) return null
    if (node.path === path) return node
    for (const child of node.children || []) {
      const found = findFile(child, path)
      if (found) return found
    }
    return null
  }

  async function openSkillDialog(skill: AgentSkillSummary, mode: AgentSkillDialogMode) {
    selectedSkill.value = skill
    dialogMode.value = mode
    fileTree.value = null
    selectedFile.value = null
    fileContent.value = ''
    dialogVisible.value = true
    fileLoading.value = true
    try {
      const data = await fetchAgentSkillFiles(skill.name)
      fileTree.value = data.tree || null
      const skillMd = findFile(fileTree.value, 'SKILL.md')
      if (skillMd) await selectSkillFile(skillMd)
    } catch (error: any) {
      logAgentPanelError('open_agent_skill', error, { skill: skill.name })
      ElMessage.error(error.response?.data?.error || error.message || '读取 Skill 文件列表失败')
    } finally {
      fileLoading.value = false
    }
  }

  async function selectSkillFile(node: AgentSkillFileNode) {
    selectedFile.value = node
    if (!selectedSkill.value || node.kind !== 'file') {
      fileContent.value = ''
      return
    }
    fileLoading.value = true
    try {
      const data = await readAgentSkillFile(selectedSkill.value.name, node.path)
      if (data.file) {
        selectedFile.value = { ...node, path: data.file.path }
        fileContent.value = data.file.content || ''
      }
    } catch (error: any) {
      logAgentPanelError('read_agent_skill_file', error, { skill: selectedSkill.value.name, path: node.path })
      ElMessage.error(error.response?.data?.error || error.message || '读取 Skill 文件失败')
    } finally {
      fileLoading.value = false
    }
  }

  async function saveSkillFile() {
    if (!selectedSkill.value || !selectedFile.value) return
    fileSaving.value = true
    try {
      await saveAgentSkillFile(selectedSkill.value.name, selectedFile.value.path, fileContent.value)
      ElMessage.success('Skill 文件已保存')
      await loadSkills()
    } catch (error: any) {
      logAgentPanelError('save_agent_skill_file', error, { skill: selectedSkill.value.name, path: selectedFile.value.path })
      ElMessage.error(error.response?.data?.error || error.message || '保存 Skill 文件失败')
    } finally {
      fileSaving.value = false
    }
  }

  async function removeSkill(skill: AgentSkillSummary) {
    try {
      await ElMessageBox.confirm(`确定删除 Skill「${skill.name}」吗？该操作会删除整个技能目录。`, '删除 Skill', {
        confirmButtonText: '删除',
        cancelButtonText: '取消',
        type: 'warning',
        lockScroll: false,
      })
      await deleteAgentSkill(skill.name)
      ElMessage.success('Skill 已删除')
      await loadSkills()
    } catch (error: any) {
      if (error === 'cancel') return
      logAgentPanelError('delete_agent_skill', error, { skill: skill.name })
      ElMessage.error(error.response?.data?.error || error.message || '删除 Skill 失败')
    }
  }

  watch(pageSize, () => {
    page.value = 1
  })

  return {
    loading,
    fileLoading,
    fileSaving,
    skills,
    page,
    pageSize,
    pagedSkills,
    dialogVisible,
    dialogMode,
    dialogTitle,
    selectedSkill,
    fileTree,
    selectedFile,
    fileContent,
    treeData,
    canSaveFile,
    loadSkills,
    openSkillDialog,
    selectSkillFile,
    saveSkillFile,
    removeSkill,
  }
}
