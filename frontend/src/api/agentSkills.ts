import request from '@/utils/request'

export interface AgentSkillSummary {
  name: string
  display_name?: string
  description?: string | null
  path?: string
  relative_path?: string
  file_count?: number
  available?: boolean
  fail_reason?: string | null
}

export interface AgentSkillFileNode {
  name: string
  path: string
  kind: 'dir' | 'file'
  children?: AgentSkillFileNode[]
}

export interface AgentSkillFileContent {
  skill: string
  path: string
  content: string
  bytes_read?: number
  truncated?: boolean
}

export function fetchAgentSkills(): Promise<{ skills?: AgentSkillSummary[] }> {
  return request.get('/agent/skills')
}

export function fetchAgentSkillFiles(skill: string): Promise<{ tree?: AgentSkillFileNode }> {
  return request.post('/agent/skills/files', { skill })
}

export function readAgentSkillFile(skill: string, path?: string): Promise<{ file?: AgentSkillFileContent }> {
  return request.post('/agent/skills/files/read', { skill, path })
}

export function saveAgentSkillFile(skill: string, path: string, content: string): Promise<unknown> {
  return request.post('/agent/skills/files/save', { skill, path, content })
}

export function deleteAgentSkill(skill: string): Promise<unknown> {
  return request.post('/agent/skills/delete', { skill })
}
