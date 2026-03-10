const DOC_DRAFT_CACHE_KEY = 'markflow.doc_drafts'

interface DocDraftEntry {
  content: string
  updatedAt: number
}

let draftCache: Record<string, DocDraftEntry> | null = null

function getStorage() {
  if (typeof window === 'undefined') return null
  return window.localStorage
}

function loadDraftCache() {
  if (draftCache) return draftCache
  const storage = getStorage()
  if (!storage) {
    draftCache = {}
    return draftCache
  }

  try {
    const raw = storage.getItem(DOC_DRAFT_CACHE_KEY)
    const parsed = raw ? JSON.parse(raw) : {}
    draftCache = parsed && typeof parsed === 'object' ? parsed as Record<string, DocDraftEntry> : {}
  } catch {
    draftCache = {}
  }

  return draftCache
}

function persistDraftCache() {
  const storage = getStorage()
  if (!storage || !draftCache) return
  storage.setItem(DOC_DRAFT_CACHE_KEY, JSON.stringify(draftCache))
}

export function getDocDraftContent(docId: number): string | null {
  const cache = loadDraftCache()
  const entry = cache[String(docId)]
  return entry ? entry.content : null
}

export function hasDocDraft(docId: number): boolean {
  const cache = loadDraftCache()
  return Boolean(cache[String(docId)])
}

export function setDocDraftContent(docId: number, content: string) {
  const cache = loadDraftCache()
  cache[String(docId)] = {
    content,
    updatedAt: Date.now(),
  }
  persistDraftCache()
}

export function clearDocDraftContent(docId: number) {
  const cache = loadDraftCache()
  if (!cache[String(docId)]) return
  delete cache[String(docId)]
  persistDraftCache()
}
