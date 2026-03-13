export type AgentWriterMode = 'append' | 'replace' | 'rewrite_section' | 'replace_block'

export interface AgentWriterStartDetail {
  docId: number
  mode: AgentWriterMode
  save?: boolean
}

export interface AgentWriterChunkDetail {
  docId: number
  chunk: string
}

export interface AgentWriterCompleteDetail {
  docId: number
}

export interface AgentWriterResultDetail {
  docId: number
  mode: AgentWriterMode
  ok: boolean
  reason?: string
  payload?: string
  replacementPreview?: string
  target?: string | null
}

export const AGENT_WRITER_START_EVENT = 'markflow:agent-writer-start'
export const AGENT_WRITER_CHUNK_EVENT = 'markflow:agent-writer-chunk'
export const AGENT_WRITER_COMPLETE_EVENT = 'markflow:agent-writer-complete'
export const AGENT_WRITER_RESULT_EVENT = 'markflow:agent-writer-result'

let currentEditorSnapshot: { docId: number; content: string } | null = null

export function setAgentEditorSnapshot(docId: number, content: string) {
  currentEditorSnapshot = { docId, content }
}

export function clearAgentEditorSnapshot(docId?: number) {
  if (!currentEditorSnapshot) return
  if (docId === undefined || currentEditorSnapshot.docId === docId) {
    currentEditorSnapshot = null
  }
}

export function getAgentEditorSnapshot(docId?: number): string | null {
  if (!currentEditorSnapshot) return null
  if (docId !== undefined && currentEditorSnapshot.docId !== docId) return null
  return currentEditorSnapshot.content
}

export function dispatchAgentWriterStart(detail: AgentWriterStartDetail) {
  window.dispatchEvent(new CustomEvent<AgentWriterStartDetail>(AGENT_WRITER_START_EVENT, { detail }))
}

export function dispatchAgentWriterChunk(detail: AgentWriterChunkDetail) {
  window.dispatchEvent(new CustomEvent<AgentWriterChunkDetail>(AGENT_WRITER_CHUNK_EVENT, { detail }))
}

export function dispatchAgentWriterComplete(detail: AgentWriterCompleteDetail) {
  window.dispatchEvent(new CustomEvent<AgentWriterCompleteDetail>(AGENT_WRITER_COMPLETE_EVENT, { detail }))
}

export function dispatchAgentWriterResult(detail: AgentWriterResultDetail) {
  window.dispatchEvent(new CustomEvent<AgentWriterResultDetail>(AGENT_WRITER_RESULT_EVENT, { detail }))
}
