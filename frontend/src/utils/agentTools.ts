import { describeAgentEditorBridge, getAgentEditorBridge } from '@/utils/agentEditorBridge'

export interface AgentToolCall {
  call_id: string
  name: string
  arguments: string
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

  const result = await executor(
    window,
    document,
    window.location,
    window.history,
    window.navigator,
    window.localStorage,
    window.sessionStorage,
    console,
    editor,
    markflow,
  )

  return {
    executed: true,
    result: safeSerialize(result),
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
        case 'get_current_page_state':
          output = await toolRuntime.getCurrentPageState()
          break
        case 'list_page_routes':
          output = await toolRuntime.listPageRoutes()
          break
        case 'navigate_to_page':
          output = await toolRuntime.navigateToPage(args)
          break
        case 'list_projects':
          output = await toolRuntime.listProjects(args)
          break
        case 'open_project':
          output = await toolRuntime.openProject(args)
          break
        case 'create_project':
          output = await toolRuntime.createProject(args)
          break
        case 'update_project':
          output = await toolRuntime.updateProject(args)
          break
        case 'delete_projects':
          output = await toolRuntime.deleteProjects(args)
          break
        case 'execute_browser_javascript':
          output = await executeBrowserJavascript(args)
          break
        case 'get_project_tree':
          output = await toolRuntime.getProjectTree(args)
          break
        case 'create_tree_node':
          output = await toolRuntime.createTreeNode(args)
          break
        case 'move_tree_node':
          output = await toolRuntime.moveTreeNode(args)
          break
        case 'open_tree_node':
          output = await toolRuntime.openTreeNode(args)
          break
        case 'read_document':
          output = await toolRuntime.readDocument(args)
          break
        case 'read_editor_snapshot':
          output = await toolRuntime.readEditorSnapshot(args)
          break
        case 'save_current_document':
          output = await toolRuntime.saveCurrentDocument(args)
          break
        case 'update_tree_node_meta':
          output = await toolRuntime.updateTreeNodeMeta(args)
          break
        case 'delete_tree_nodes':
          output = await toolRuntime.deleteTreeNodes(args)
          break
        case 'get_markdown_editor_runtime':
          output = await toolRuntime.getMarkdownEditorRuntime()
          break
        case 'get_browser_runtime':
          output = await toolRuntime.getBrowserRuntime(args)
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
