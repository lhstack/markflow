import type { AxiosProgressEvent } from 'axios'
import request from '@/utils/request'
import {
  createManagedUploadTask,
  failManagedUpload,
  finishManagedUpload,
  startManagedUpload,
  updateManagedUploadProgress,
  type ManagedUploadTask,
} from '@/utils/managedUploads'

export type ImageUploadKind = 'avatar' | 'project-background' | 'doc-image'
export type FileUploadKind = 'doc-file'

export interface UploadOptions {
  task?: ManagedUploadTask
}

async function uploadManagedAsset(file: File, kind: string, options: UploadOptions = {}): Promise<string> {
  const formData = new FormData()
  formData.append('kind', kind)
  formData.append('file', file)

  const task = options.task ?? createManagedUploadTask(kind, file)
  startManagedUpload(task)

  try {
    const data = (await request.post('/uploads', formData, {
      onUploadProgress(event: AxiosProgressEvent) {
        const loaded = event.loaded ?? 0
        const total = event.total ?? file.size
        updateManagedUploadProgress(task, loaded, total)
      },
    })) as { url: string }

    finishManagedUpload(task, data.url)
    return data.url
  } catch (error: any) {
    failManagedUpload(task, error?.response?.data?.error || error?.message || '上传失败')
    throw error
  }
}

export async function uploadImage(file: File, kind: ImageUploadKind, options: UploadOptions = {}): Promise<string> {
  return uploadManagedAsset(file, kind, options)
}

export async function uploadFile(file: File, kind: FileUploadKind, options: UploadOptions = {}): Promise<string> {
  return uploadManagedAsset(file, kind, options)
}
