# 文档读取、局部编辑与保存 Reference

本文件描述文档内容相关的 `markflow` 方法。全部通过 `execute_browser_javascript` 调用。

重要分流：

- **空文档首稿、文末追加**：使用 `appendCurrentDocumentContent`。
- **整篇重写/覆盖**：使用 `replaceCurrentDocumentContent`，这是覆盖性操作，用户未明确要求时先确认。
- **局部结构化修改**：改某个章节、替换某段、批量替换、交换章节，使用本文局部编辑方法。
- 写入/局部编辑前先读最新内容，尤其当前文档可能有未保存草稿时，优先 `readEditorSnapshot`。

通用定位参数：多数文档方法支持 `doc_id`/`node_id`，或 `project_id + doc_path/doc_name`。优先 ID。

## 1. readDocument

**应用场景**：读取后端已保存正文；确认文档内容；做基于已保存内容的分析。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `doc_id` / `node_id` | number | 否 | 文档节点 ID。省略定位时读取当前打开文档。 |
| `doc_path` / `node_path` | string | 否 | 文档路径，通常配合 `project_id`。 |
| `doc_name` / `node_name` | string | 否 | 文档名，兜底使用。 |
| `project_id` / `project_name` | number/string | 否 | 项目定位。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `project` | 所属项目。 |
| `document` | 文档节点摘要。 |
| `content` | 已保存正文。 |
| `content_length` | 正文长度。 |

**Example**

```javascript
return await markflow.readDocument({ doc_id: 78 })
```

## 2. readEditorSnapshot

**应用场景**：读取编辑器实时内容，包含未保存草稿；局部编辑前确认最新正文。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `doc_id` / `node_id` | number | 否 | 文档 ID；省略时读当前文档。 |
| `doc_path` / `doc_name` | string | 否 | 文档路径/名称。 |
| `project_id` / `project_name` | number/string | 否 | 项目定位。 |
| `max_chars` | number | 否 | 只需要摘要时限制返回正文长度。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `content` | 实时正文或截断后的正文。 |
| `content_length` | 完整正文长度。 |
| `truncated` | 是否因 `max_chars` 截断。 |
| `source` | 内容来源，如 editor/draft/saved。 |
| `unsaved` / `has_unsaved_changes` | 是否存在未保存修改。 |

**Example**

```javascript
return await markflow.readEditorSnapshot({ doc_id: 78, max_chars: 4000 })
```

## 3. appendCurrentDocumentContent

**应用场景**：向空文档写首稿，或在文档末尾追加 Markdown 正文。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `content` / `markdown` / `value` | string | 是 | 要追加的 Markdown 正文。 |
| `doc_id` / `node_id` | number | 否 | 文档 ID；省略时默认当前文档。 |
| `doc_path` / `doc_name` | string | 否 | 文档路径/名称。 |
| `project_id` / `project_name` | number/string | 否 | 项目定位。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `applied` | 是否已应用到编辑器。 |
| `mode` | `append_current_document_content`。 |
| `appended_length` | 本次追加长度。 |
| `content_length_before/after` | 追加前后长度。 |
| `unsaved_changes` | 写入后通常为 `true`。 |
| `target_document` | 目标文档摘要。 |

**Example**

```javascript
return await markflow.appendCurrentDocumentContent({
  doc_id: 78,
  content: '# 标题\n\n这里是首稿正文。'
})
```

## 4. replaceCurrentDocumentContent

**应用场景**：用一份新的完整 Markdown 正文覆盖当前整篇文档。属于覆盖性操作，用户没有明确要求时先确认。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `content` / `markdown` / `value` | string | 是 | 新的完整 Markdown 正文。 |
| `doc_id` / `node_id` | number | 否 | 文档 ID；省略时默认当前文档。 |
| `doc_path` / `doc_name` | string | 否 | 文档路径/名称。 |
| `project_id` / `project_name` | number/string | 否 | 项目定位。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `applied` | 是否已应用到编辑器。 |
| `mode` | `replace_current_document_content`。 |
| `content_length_before/after` | 替换前后长度。 |
| `unsaved_changes` | 写入后通常为 `true`。 |
| `target_document` | 目标文档摘要。 |

**Example**

```javascript
return await markflow.replaceCurrentDocumentContent({
  doc_id: 78,
  content: '# 新标题\n\n新的完整正文。'
})
```

## 5. rewriteDocumentSection

**应用场景**：替换某个 Markdown 标题下的整节内容。适合“把结论章节改成……”这类局部修改。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `doc_id` / `node_id` | number | 否 | 文档 ID；省略时默认当前文档。 |
| `target_heading` | string | 是 | 目标标题，可写 `## 结论` 或 `结论`。必须真实存在。 |
| `content` | string | 是 | 新整节 Markdown。通常包含标题本身。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `updated` / `ok` | 是否修改成功。 |
| `target_heading` | 命中的标题。 |
| `content_length_before/after` | 修改前后长度。 |
| `unsaved` | 修改后通常为未保存状态。 |

**Example**

```javascript
return await markflow.rewriteDocumentSection({
  doc_id: 78,
  target_heading: '## 结论',
  content: '## 结论\n\n新的结论内容。'
})
```

## 6. replaceDocumentBlock

**应用场景**：精确替换一段已知原文。适合小范围、可逐字匹配的修改。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `doc_id` / `node_id` | number | 否 | 文档 ID；省略时默认当前文档。 |
| `find` | string | 是 | 原文片段，必须与当前正文逐字匹配。 |
| `replace` | string | 是 | 替换后的文本。 |

**出参**：是否匹配/替换成功、替换次数、修改后长度、未保存状态。

**Example**

```javascript
return await markflow.replaceDocumentBlock({
  doc_id: 78,
  find: '原文片段，必须逐字匹配',
  replace: '替换后的片段'
})
```

## 7. replaceDocumentBlocks

**应用场景**：一次合并多段精确替换，避免多轮来回写入。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `doc_id` / `node_id` | number | 否 | 文档 ID；省略时默认当前文档。 |
| `replacements` | array | 是 | 每项 `{ find, replace }`。后一项基于前一项替换后的正文继续匹配。 |

**出参**：每段替换结果、失败项、最终长度、未保存状态。

**Example**

```javascript
return await markflow.replaceDocumentBlocks({
  doc_id: 78,
  replacements: [
    { find: '旧术语 A', replace: '新术语 A' },
    { find: '旧术语 B', replace: '新术语 B' }
  ]
})
```

## 8. swapDocumentSections

**应用场景**：交换两个 Markdown 章节顺序，保留各自标题层级和内容。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `doc_id` / `node_id` | number | 否 | 文档 ID；省略时默认当前文档。 |
| `first_heading` | string | 是 | 第一个章节标题。 |
| `second_heading` | string | 是 | 第二个章节标题。 |

**出参**：是否交换成功、两个章节命中信息、修改后未保存状态。

**Example**

```javascript
return await markflow.swapDocumentSections({
  doc_id: 78,
  first_heading: '结论',
  second_heading: '重要发现'
})
```

## 9. saveCurrentDocument

**应用场景**：用户要求保存/提交/应用修改时保存当前文档或指定文档。仅生成草稿时不要保存。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `doc_id` / `node_id` | number | 否 | 文档 ID；省略时保存当前文档。 |
| `doc_path` / `doc_name` | string | 否 | 文档路径/名称。 |
| `project_id` / `project_name` | number/string | 否 | 项目定位。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `saved` | 是否真正执行保存。 |
| `already_saved` | 是否本来已保存。 |
| `unsaved_changes_before_save` | 保存前是否有未保存修改。 |
| `document` | 保存的文档。 |

不要把 `already_saved=true` 说成“刚保存成功”，应说明“文档本来就是已保存状态”。

**Example**

```javascript
return await markflow.saveCurrentDocument({ doc_id: 78 })
```
