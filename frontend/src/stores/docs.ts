import { defineStore } from 'pinia'
import { ref } from 'vue'
import request from '@/utils/request'

export interface DocNode {
  id: string
  project_id?: string | null
  parent_id?: string | null
  name: string
  node_type: 'dir' | 'doc'
  content?: string
  sort_order: number
  created_at: string
  updated_at: string
  children: DocNode[]
}

export interface DirStats {
  doc_count: number
  dir_count: number
}

export const useDocsStore = defineStore('docs', () => {
  const tree = ref<DocNode[]>([])
  const currentNode = ref<DocNode | null>(null)
  const currentStats = ref<DirStats>({ doc_count: 0, dir_count: 0 })
  const loading = ref(false)

  async function fetchTree(projectId?: string | null) {
    loading.value = true
    try {
      const config = projectId ? { params: { project_id: projectId } } : undefined
      const data = await request.get('/docs', config) as any
      tree.value = data.tree
    } finally {
      loading.value = false
    }
  }

  async function fetchNode(id: string) {
    const data = await request.get(`/docs/${id}`) as any
    currentNode.value = data.node
    if (data.stats) {
      currentStats.value = data.stats
    }
    return data
  }

  async function createNode(payload: {
    project_id?: string
    parent_id?: string
    name: string
    node_type: 'dir' | 'doc'
    content?: string
  }, projectId?: string | null) {
    const data = await request.post('/docs', payload) as any
    await fetchTree(projectId ?? payload.project_id ?? null)
    return data.node as DocNode
  }

  async function updateNode(id: string, payload: { name?: string; content?: string }, projectId?: string | null) {
    const data = await request.put(`/docs/${id}`, payload) as any
    if (currentNode.value?.id === id) {
      currentNode.value = { ...currentNode.value, ...data.node }
    }
    // Only refresh tree on name change (not on content save to avoid re-render)
    if (payload.name !== undefined) {
      await fetchTree(projectId)
    }
    return data.node as DocNode
  }

  async function deleteNode(id: string, projectId?: string | null) {
    await request.delete(`/docs/${id}`)
    if (currentNode.value?.id === id) {
      currentNode.value = null
    }
    await fetchTree(projectId)
  }

  async function moveNode(
    id: string,
    parent_id: string | null,
    sort_order: number,
    projectId?: string | null,
    refresh = true
  ) {
    await request.put(`/docs/${id}/move`, { parent_id, sort_order })
    if (refresh) {
      await fetchTree(projectId)
    }
  }

  return {
    tree,
    currentNode,
    currentStats,
    loading,
    fetchTree,
    fetchNode,
    createNode,
    updateNode,
    deleteNode,
    moveNode,
  }
})
