import { reactive } from 'vue'

export type ManagedUploadStatus = 'idle' | 'uploading' | 'success' | 'error'

export interface ManagedUploadTask {
  id: string
  kind: string
  fileName: string
  progress: number
  loaded: number
  total: number
  status: ManagedUploadStatus
  error: string | null
  url: string | null
  startedAt: number | null
  finishedAt: number | null
}

let uploadSequence = 0

export const managedUploads = reactive<ManagedUploadTask[]>([])

export function createManagedUploadTask(kind: string, file: File): ManagedUploadTask {
  const task: ManagedUploadTask = reactive({
    id: `upload-${Date.now()}-${uploadSequence += 1}`,
    kind,
    fileName: file.name,
    progress: 0,
    loaded: 0,
    total: file.size,
    status: 'idle',
    error: null,
    url: null,
    startedAt: null,
    finishedAt: null,
  })

  managedUploads.unshift(task)
  return task
}

export function startManagedUpload(task: ManagedUploadTask) {
  task.progress = 0
  task.loaded = 0
  task.status = 'uploading'
  task.error = null
  task.url = null
  task.startedAt = Date.now()
  task.finishedAt = null
}

export function updateManagedUploadProgress(task: ManagedUploadTask, loaded: number, total: number) {
  task.loaded = loaded
  task.total = total
  task.progress = total > 0 ? Math.min(100, Math.round((loaded / total) * 100)) : 0
}

export function finishManagedUpload(task: ManagedUploadTask, url: string) {
  task.loaded = task.total
  task.progress = 100
  task.status = 'success'
  task.url = url
  task.error = null
  task.finishedAt = Date.now()
}

export function failManagedUpload(task: ManagedUploadTask, error: string) {
  task.status = 'error'
  task.error = error
  task.finishedAt = Date.now()
}

export function removeManagedUpload(taskId: string) {
  const index = managedUploads.findIndex((task) => task.id === taskId)
  if (index >= 0) {
    managedUploads.splice(index, 1)
  }
}
