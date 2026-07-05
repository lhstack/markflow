---
name: markflow-manual
description: MarkFlow 工作区操作手册。用于通过 execute_browser_javascript 在当前浏览器页面执行 JS，实时操作项目、文档树、文档编写/追加/替换、文档读取、局部编辑、保存、页面导航、附件和用户资料。
---

# MarkFlow 工作区操作手册

本技能用于操作 **MarkFlow 当前浏览器页面里的真实工作区状态**。不要凭记忆假设页面、项目、文档、节点 ID 或编辑器状态；需要状态就实时调用工具读取。

## 1. 执行工具：execute_browser_javascript

`execute_browser_javascript` 参考 shell/bash 工具风格设计：给它一段 JS，它在当前浏览器页面执行，并返回类命令执行结果。

### 入参

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `code` | string | 是 | 在当前浏览器页面执行的 JavaScript。代码运行在 async function body 内，建议显式 `return` 可序列化结果。 |
| `timeout_secs` | integer | 否 | 超时秒数。默认 `30`，硬上限 `300`。浏览器同步死循环无法被抢占，异步/await 场景可超时返回。 |

### 出参

外层工具结果一般是 `{ ok, tool, result }`。`result` 内部结构类似 bash：

| 字段 | 说明 |
| --- | --- |
| `exit_code` | 成功为 `0`。 |
| `success` | JS 是否成功执行。 |
| `stdout` | 捕获 `console.log/info` 的文本。 |
| `stderr` | 捕获 `console.warn/error` 的文本。 |
| `result` | JS `return` 的可序列化值。 |
| `duration_ms` | 执行耗时。 |

### 运行环境

`code` 里可直接使用：

- 浏览器对象：`window`、`document`、`location`、`history`、`navigator`、`localStorage`、`sessionStorage`、`console`
- `markflow`：MarkFlow 工作区助手，封装项目、文档树、文档编写/追加/替换、文档读取/局部编辑、页面、附件、用户资料等操作。
- `editor`：当前 Markdown 编辑器桥接对象；没有激活文档时可能为 `null`。优先使用 `markflow.*` 方法，不要直接调用 `editor.setValue/appendValue/replaceValue` 绕过手册流程。

### Example

```json
{
  "code": "console.log('path', location.pathname); return await markflow.getCurrentPageState()",
  "timeout_secs": 10
}
```

## 2. 文档正文写入方式

凡是涉及 **空文档写首稿、文末追加、整篇重写/覆盖**，都通过 `execute_browser_javascript` 调用 `markflow` 方法完成：

| 场景 | 方法 |
| --- | --- |
| 空文档写首稿 | `markflow.appendCurrentDocumentContent({ doc_id, content })` |
| 在当前文档末尾追加 | `markflow.appendCurrentDocumentContent({ doc_id, content })` |
| 用一份新完整正文覆盖当前整篇文档 | `markflow.replaceCurrentDocumentContent({ doc_id, content })` |
| 只改某个章节/某段/交换章节 | 读取 `references/document.md` 使用局部编辑方法。 |

正文内容放在 JS 字符串参数 `content` 中。写入前必须先确认目标文档，优先使用 `doc_id`。写入后继续按用户目标执行下一步；需要说明进度时直接在最终回复中简洁说明。

## 3. 铁律

1. **事实来自工具**：先读真实项目/树/文档/页面状态，再操作。禁止猜项目名、节点 ID、路径、当前文档内容。
2. **定位优先级**：ID > 路径 > 名称。拿得到 ID 就用 ID。
3. **破坏性操作需用户明确确认**：删除项目、删除节点、批量删附件、覆盖整篇正文等必须有用户明确授权。
4. **JS 一次只做一类操作**：读状态、创建节点、移动节点、正文写入、局部编辑、保存等尽量分开，读返回值后再决定下一步。
5. **正文写入分流**：首稿/追加/整篇覆盖走 `appendCurrentDocumentContent` / `replaceCurrentDocumentContent`；局部结构化修改走 `references/document.md` 的局部编辑方法。

## 4. References 使用说明

按任务读取对应 reference，不要凭记忆猜方法名、参数、返回结构：

| Reference | 内容 | 适用场景 |
| --- | --- | --- |
| `references/workspace.md` | 项目与文档树：列项目、打开/创建/更新/删除项目，读取树、创建/移动/打开/重命名/删除节点。 | 项目管理、目录/文档节点结构操作。 |
| `references/document.md` | 文档编写/追加/替换、文档读取、实时快照、局部结构化编辑、保存。 | 写正文、读取正文、改某节/某段/交换章节、保存当前文档。 |
| `references/page.md` | 页面状态、路由导航、附件、用户资料、运行时探测。 | 判断当前页面、导航、附件清理、头像资料更新、环境探测。 |

## 5. 常用流程

### 新建文档并写首稿

1. `markflow.listProjects()` 确认项目。
2. `markflow.getProjectTree({ project_id, refresh: true })` 确认父目录。
3. `markflow.createTreeNode({ project_id, parent_id, name, node_type: 'doc', open_after_create: true })` 创建并打开空文档。
4. `markflow.appendCurrentDocumentContent({ doc_id, content })` 写首稿。
5. 若用户要求保存，调用 `markflow.saveCurrentDocument({ doc_id })`。

### 修改文档某一节

1. 读取 `references/document.md`。
2. `markflow.readEditorSnapshot({ doc_id })` 获取最新正文。
3. 使用 `rewriteDocumentSection` / `replaceDocumentBlock(s)` / `swapDocumentSections` 做局部修改。
4. 用户要求保存时再 `markflow.saveCurrentDocument({ doc_id })`。
