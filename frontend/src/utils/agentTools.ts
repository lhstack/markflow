import { describeAgentEditorBridge, getAgentEditorBridge } from '@/utils/agentEditorBridge'
import type { AgentToolCapability, AgentToolStagePolicy } from '@/agent/protocol'

export interface AgentToolCall {
  call_id: string
  name: string
  arguments: string
  stage_policy?: AgentToolStagePolicy | string
  capabilities?: AgentToolCapability[] | string[]
}

export interface AgentToolOutputPayload {
  call_id: string
  name?: string
  arguments?: string
  output: unknown
}

export interface AgentToolRuntime {
  getCurrentPageState: () => Promise<unknown> | unknown
  listPageRoutes: () => Promise<unknown> | unknown
  navigateToPage: (args: Record<string, any>) => Promise<unknown>
  updateProfile: (args: Record<string, any>) => Promise<unknown>
  listUploads: (args: Record<string, any>) => Promise<unknown>
  deleteUploads: (args: Record<string, any>) => Promise<unknown>
  listProjects: (args: Record<string, any>) => Promise<unknown>
  openProject: (args: Record<string, any>) => Promise<unknown>
  createProject: (args: Record<string, any>) => Promise<unknown>
  updateProject: (args: Record<string, any>) => Promise<unknown>
  deleteProjects: (args: Record<string, any>) => Promise<unknown>
  getProjectTree: (args: Record<string, any>) => Promise<unknown>
  createTreeNode: (args: Record<string, any>) => Promise<unknown>
  moveTreeNode: (args: Record<string, any>) => Promise<unknown>
  openTreeNode: (args: Record<string, any>) => Promise<unknown>
  readDocument: (args: Record<string, any>) => Promise<unknown>
  readEditorSnapshot: (args: Record<string, any>) => Promise<unknown>
  appendCurrentDocumentContent: (args: Record<string, any>) => Promise<unknown>
  replaceCurrentDocumentContent: (args: Record<string, any>) => Promise<unknown>
  rewriteDocumentSection: (args: Record<string, any>) => Promise<unknown>
  replaceDocumentBlock: (args: Record<string, any>) => Promise<unknown>
  replaceDocumentBlocks: (args: Record<string, any>) => Promise<unknown>
  swapDocumentSections: (args: Record<string, any>) => Promise<unknown>
  saveCurrentDocument: (args: Record<string, any>) => Promise<unknown>
  updateTreeNodeMeta: (args: Record<string, any>) => Promise<unknown>
  deleteTreeNodes: (args: Record<string, any>) => Promise<unknown>
  getMarkdownEditorRuntime: () => Promise<unknown> | unknown
  getBrowserRuntime: (args: Record<string, any>) => Promise<unknown> | unknown
}

let runtime: AgentToolRuntime | null = null

function syncAgentGlobals() {
  if (typeof window === 'undefined') return
  const target = window as typeof window & Record<string, unknown>

  if (!runtime) {
    delete target.markflow
    delete target.editor
    delete target.__MARKFLOW_AGENT__
    return
  }

  Object.defineProperty(target, 'markflow', {
    configurable: true,
    enumerable: false,
    get() {
      return createMarkflowJsHelper(runtime as AgentToolRuntime)
    },
  })

  Object.defineProperty(target, 'editor', {
    configurable: true,
    enumerable: false,
    get() {
      return createJsEditorAdapter()
    },
  })

  Object.defineProperty(target, '__MARKFLOW_AGENT__', {
    configurable: true,
    enumerable: false,
    get() {
      return {
        runtime: createMarkflowJsHelper(runtime as AgentToolRuntime),
        editor: createJsEditorAdapter(),
      }
    },
  })
}

export function registerAgentToolRuntime(nextRuntime: AgentToolRuntime) {
  runtime = nextRuntime
  syncAgentGlobals()
}

export function unregisterAgentToolRuntime() {
  runtime = null
  syncAgentGlobals()
}

function requireRuntime() {
  if (!runtime) {
    throw new Error('前端工具运行时尚未初始化')
  }
  return runtime
}

function safeSerialize(value: unknown, depth = 0): unknown {
  if (depth > 4) return '[MaxDepth]'
  if (value === null || value === undefined) return value
  if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
    return value
  }
  if (Array.isArray(value)) {
    return value.slice(0, 50).map((item) => safeSerialize(item, depth + 1))
  }
  if (value instanceof Error) {
    return { name: value.name, message: value.message }
  }
  if (typeof Element !== 'undefined' && value instanceof Element) {
    return {
      kind: 'Element',
      tagName: value.tagName,
      id: value.id || null,
      className: value.className || null,
      text: value.textContent?.slice(0, 200) || '',
    }
  }
  if (typeof Window !== 'undefined' && value instanceof Window) {
    return {
      kind: 'Window',
      location: value.location.href,
      title: value.document.title,
    }
  }
  if (typeof Document !== 'undefined' && value instanceof Document) {
    return {
      kind: 'Document',
      title: value.title,
      url: value.URL,
    }
  }
  if (typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>).slice(0, 50)
    return Object.fromEntries(entries.map(([key, item]) => [key, safeSerialize(item, depth + 1)]))
  }
  return String(value)
}

function parseArguments(raw: string) {
  const trimmed = raw.trim()
  if (!trimmed) return {}
  try {
    const parsed = JSON.parse(trimmed)
    return parsed && typeof parsed === 'object' ? parsed : {}
  } catch {
    throw new Error(`工具参数不是合法 JSON: ${trimmed}`)
  }
}

function createJsEditorAdapter() {
  const bridge = getAgentEditorBridge()
  if (!bridge) return null

  return {
    docId: bridge.docId,
    docName: bridge.docName,
    getValue: () => bridge.getValue(),
    setValue: (value: string) => bridge.setValue(value),
    insertValue: (value: string) => bridge.insertValue(value),
    appendValue: (value: string) => bridge.appendValue(value),
    replaceValue: (value: string) => bridge.replaceValue(value),
    focus: () => bridge.focus(),
    scrollToBottom: () => bridge.scrollToBottom(),
    save: () => bridge.save(),
  }
}

function createMarkflowJsHelper(toolRuntime: AgentToolRuntime) {
  return {
    getCurrentPageState: () => toolRuntime.getCurrentPageState(),
    listPageRoutes: () => toolRuntime.listPageRoutes(),
    navigateToPage: (args: Record<string, any>) => toolRuntime.navigateToPage(args),
    updateProfile: (args: Record<string, any>) => toolRuntime.updateProfile(args),
    listUploads: (args: Record<string, any> = {}) => toolRuntime.listUploads(args),
    deleteUploads: (args: Record<string, any>) => toolRuntime.deleteUploads(args),
    listProjects: (args: Record<string, any> = {}) => toolRuntime.listProjects(args),
    openProject: (args: Record<string, any>) => toolRuntime.openProject(args),
    createProject: (args: Record<string, any>) => toolRuntime.createProject(args),
    updateProject: (args: Record<string, any>) => toolRuntime.updateProject(args),
    deleteProjects: (args: Record<string, any>) => toolRuntime.deleteProjects(args),
    getProjectTree: (args: Record<string, any>) => toolRuntime.getProjectTree(args),
    createTreeNode: (args: Record<string, any>) => toolRuntime.createTreeNode(args),
    moveTreeNode: (args: Record<string, any>) => toolRuntime.moveTreeNode(args),
    openTreeNode: (args: Record<string, any>) => toolRuntime.openTreeNode(args),
    readDocument: (args: Record<string, any>) => toolRuntime.readDocument(args),
    readEditorSnapshot: (args: Record<string, any> = {}) => toolRuntime.readEditorSnapshot(args),
    appendCurrentDocumentContent: (args: Record<string, any>) => toolRuntime.appendCurrentDocumentContent(args),
    replaceCurrentDocumentContent: (args: Record<string, any>) => toolRuntime.replaceCurrentDocumentContent(args),
    rewriteDocumentSection: (args: Record<string, any>) => toolRuntime.rewriteDocumentSection(args),
    replaceDocumentBlock: (args: Record<string, any>) => toolRuntime.replaceDocumentBlock(args),
    replaceDocumentBlocks: (args: Record<string, any>) => toolRuntime.replaceDocumentBlocks(args),
    swapDocumentSections: (args: Record<string, any>) => toolRuntime.swapDocumentSections(args),
    saveCurrentDocument: (args: Record<string, any> = {}) => toolRuntime.saveCurrentDocument(args),
    updateTreeNodeMeta: (args: Record<string, any>) => toolRuntime.updateTreeNodeMeta(args),
    deleteTreeNodes: (args: Record<string, any>) => toolRuntime.deleteTreeNodes(args),
    getMarkdownEditorRuntime: () => toolRuntime.getMarkdownEditorRuntime(),
    getBrowserRuntime: (args: Record<string, any> = {}) => toolRuntime.getBrowserRuntime(args),
  }
}

async function executeBrowserJavascript(args: Record<string, any>) {
  const code = typeof args.code === 'string' ? args.code : ''
  if (!code.trim()) {
    throw new Error('execute_browser_javascript 缺少 code 参数')
  }

  const timeoutSecs = Math.max(1, Math.min(300, Number.isFinite(Number(args.timeout_secs)) ? Number(args.timeout_secs) : 30))
  const startedAt = Date.now()
  const stdout: string[] = []
  const stderr: string[] = []
  const stringifyConsoleValue = (value: unknown) => {
    if (typeof value === 'string') return value
    try {
      return JSON.stringify(safeSerialize(value))
    } catch {
      return String(value)
    }
  }
  const captureConsole = {
    ...console,
    log: (...items: unknown[]) => {
      stdout.push(items.map(stringifyConsoleValue).join(' '))
      console.log(...items)
    },
    info: (...items: unknown[]) => {
      stdout.push(items.map(stringifyConsoleValue).join(' '))
      console.info(...items)
    },
    warn: (...items: unknown[]) => {
      stderr.push(items.map(stringifyConsoleValue).join(' '))
      console.warn(...items)
    },
    error: (...items: unknown[]) => {
      stderr.push(items.map(stringifyConsoleValue).join(' '))
      console.error(...items)
    },
  }

  const toolRuntime = requireRuntime()
  syncAgentGlobals()
  const editor = createJsEditorAdapter()
  const markflow = createMarkflowJsHelper(toolRuntime)

  const executor = new Function(
    'window',
    'document',
    'location',
    'history',
    'navigator',
    'localStorage',
    'sessionStorage',
    'console',
    'editor',
    'markflow',
    `"use strict"; return (async () => { ${code}\n })();`,
  )

  let timeoutId: number | null = null
  const execution = Promise.resolve().then(() => executor(
    window,
    document,
    window.location,
    window.history,
    window.navigator,
    window.localStorage,
    window.sessionStorage,
    captureConsole,
    editor,
    markflow,
  ))
  const timeout = new Promise((_, reject) => {
    timeoutId = window.setTimeout(() => reject(new Error(`execute_browser_javascript timed out after ${timeoutSecs}s`)), timeoutSecs * 1000)
  })

  try {
    const result = await Promise.race([execution, timeout])
    return {
      exit_code: 0,
      success: true,
      stdout: stdout.join('\n'),
      stderr: stderr.join('\n'),
      result: safeSerialize(result),
      duration_ms: Date.now() - startedAt,
    }
  } finally {
    if (timeoutId !== null) window.clearTimeout(timeoutId)
  }
}

export async function executeAgentToolCalls(calls: AgentToolCall[]): Promise<AgentToolOutputPayload[]> {
  const outputs: AgentToolOutputPayload[] = []

  for (const call of calls) {
    let output: unknown
    try {
      const args = parseArguments(call.arguments)
      const toolRuntime = requireRuntime()

      switch (call.name) {
        case 'execute_browser_javascript':
          output = await executeBrowserJavascript(args)
          break
        default:
          throw new Error(`未知工具: ${call.name}`)
      }

      outputs.push({
        call_id: call.call_id,
        name: call.name,
        arguments: call.arguments,
        output: {
          ok: true,
          tool: call.name,
          result: safeSerialize(output),
        },
      })
    } catch (error: any) {
      console.error('agent tool execution failed', {
        callId: call.call_id,
        tool: call.name,
        arguments: call.arguments,
        error,
      })
      outputs.push({
        call_id: call.call_id,
        name: call.name,
        arguments: call.arguments,
        output: {
          ok: false,
          tool: call.name,
          error: error?.message || String(error),
          editor: describeAgentEditorBridge(),
        },
      })
    }
  }

  return outputs
}
