export interface AgentEditorSaveResult {
  saved: boolean
  alreadySaved?: boolean
  inFlight?: boolean
  contentLength?: number
}

export interface AgentEditorBridge {
  docId: number
  docName: string
  getValue: () => string
  setValue: (value: string) => void
  insertValue: (value: string) => void
  appendValue: (value: string) => void
  replaceValue: (value: string) => void
  focus: () => void
  scrollToBottom: () => void
  save: () => Promise<AgentEditorSaveResult>
}

let currentBridge: AgentEditorBridge | null = null

export function registerAgentEditorBridge(bridge: AgentEditorBridge) {
  currentBridge = bridge
}

export function unregisterAgentEditorBridge(docId?: number) {
  if (!currentBridge) return
  if (typeof docId === 'number' && currentBridge.docId !== docId) return
  currentBridge = null
}

export function getAgentEditorBridge() {
  return currentBridge
}

export function describeAgentEditorBridge() {
  const bridge = currentBridge
  if (!bridge) {
    return {
      available: false,
      message: '当前没有处于激活状态的 Markdown 编辑器。',
      methods: [],
    }
  }

  return {
    available: true,
    docId: bridge.docId,
    docName: bridge.docName,
    methods: [
      {
        name: 'getValue',
        description: '读取当前编辑器完整 Markdown 内容。',
      },
      {
        name: 'setValue',
        description: '直接设置当前编辑器内容。',
      },
      {
        name: 'insertValue',
        description: '在当前光标位置插入内容。',
      },
      {
        name: 'appendValue',
        description: '在文档末尾追加内容。',
      },
      {
        name: 'replaceValue',
        description: '整体替换文档内容。',
      },
      {
        name: 'focus',
        description: '让编辑器获得焦点。',
      },
      {
        name: 'scrollToBottom',
        description: '将编辑器滚动到底部。',
      },
      {
        name: 'save',
        description: '保存当前文档。',
      },
    ],
  }
}
