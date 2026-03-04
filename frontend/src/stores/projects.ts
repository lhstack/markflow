import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import request from '@/utils/request'

export interface ProjectItem {
  id: string
  name: string
  description: string
  background_image?: string | null
  sort_order: number
  created_at: string
  updated_at: string
}

export const useProjectsStore = defineStore('projects', () => {
  const projects = ref<ProjectItem[]>([])
  const currentProjectId = ref<string | null>(null)
  const loading = ref(false)

  const currentProject = computed(() =>
    projects.value.find((project) => project.id === currentProjectId.value) || null
  )

  async function fetchProjects() {
    loading.value = true
    try {
      const data = await request.get('/projects') as any
      projects.value = data.projects || []
      if (projects.value.length === 0) {
        currentProjectId.value = null
      } else if (!currentProjectId.value || !projects.value.some((project) => project.id === currentProjectId.value)) {
        currentProjectId.value = projects.value[0].id
      }
    } finally {
      loading.value = false
    }
  }

  function clearCurrentProject() {
    currentProjectId.value = null
  }

  function selectProject(projectId: string | null) {
    currentProjectId.value = projectId
  }

  async function createProject(payload: {
    name: string
    description?: string
    background_image?: string
  }) {
    const data = await request.post('/projects', payload) as any
    const created = data.project as ProjectItem
    projects.value.push(created)
    projects.value.sort((a, b) => a.sort_order - b.sort_order)
    currentProjectId.value = created.id
    return created
  }

  async function updateProject(
    id: string,
    payload: { name?: string; description?: string; background_image?: string }
  ) {
    const data = await request.put(`/projects/${id}`, payload) as any
    const updated = data.project as ProjectItem
    projects.value = projects.value.map((project) => (project.id === id ? updated : project))
    projects.value.sort((a, b) => a.sort_order - b.sort_order)
    return updated
  }

  async function deleteProject(id: string) {
    const data = await request.delete(`/projects/${id}`) as any
    projects.value = projects.value.filter((project) => project.id !== id)

    if (currentProjectId.value === id) {
      const fallbackId = data.fallback_project_id as string | undefined
      if (fallbackId && projects.value.some((project) => project.id === fallbackId)) {
        currentProjectId.value = fallbackId
      } else {
        currentProjectId.value = projects.value[0]?.id || null
      }
    }

    return data
  }

  return {
    projects,
    currentProjectId,
    currentProject,
    loading,
    fetchProjects,
    clearCurrentProject,
    selectProject,
    createProject,
    updateProject,
    deleteProject,
  }
})
