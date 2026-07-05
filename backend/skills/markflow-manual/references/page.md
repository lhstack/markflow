# 页面导航、状态、附件与用户资料 Reference

本文件描述页面状态、路由、附件和用户资料方法。全部通过 `execute_browser_javascript` 调用。

## 1. getCurrentPageState

**应用场景**：任何依赖当前页面/当前项目/当前文档/编辑器状态的操作前，先读取真实状态。

**入参**：无。

**出参**

| 字段 | 说明 |
| --- | --- |
| `route` | 当前路由 path/query/hash。 |
| `scope` / `view` | 当前页面/工作区视图。 |
| `current_project` | 当前项目摘要，可能为 `null`。 |
| `current_node` | 当前文档树节点摘要，可能为 `null`。 |
| `editor` | 编辑器可用性、文档 ID、未保存状态、内容长度等。 |
| `visible_projects` / `visible_nodes` | 当前页面可见项目/节点摘要。 |
| `capabilities` | 当前可用能力。 |

**Example**

```javascript
return await markflow.getCurrentPageState()
```

## 2. listPageRoutes

**应用场景**：不知道可跳转页面 route 时查询路由表。

**入参**：无。

**出参**：路由数组，包含 route、用途、参数要求。

**Example**

```javascript
return await markflow.listPageRoutes()
```

## 3. navigateToPage

**应用场景**：需要跳转到非项目/非节点的通用页面；打开项目优先用 `openProject`，打开文档/目录优先用 `openTreeNode`。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `route` | string | 是 | 目标路由名或路径，先用 `listPageRoutes` 确认。 |
| 其他定位参数 | any | 否 | 根据 route 要求传入，如项目/文档定位。 |

**出参**：目标 route、当前页面状态摘要、导航是否完成。

**Example**

```javascript
return await markflow.navigateToPage({ route: 'home' })
```

## 4. listUploads

**应用场景**：查询附件、图片、未引用文件；删除附件或设置头像前先用它确认目标。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `upload_ids` | number[] | 否 | 指定附件 ID。 |
| `kind` / `kinds` | string/string[] | 否 | 类型筛选，如 `image`、`avatar`、`doc-image`。 |
| `name_query` / `keyword` | string | 否 | 文件名关键字。 |
| `unused_only` / `unreferenced_only` | boolean | 否 | 仅返回未引用附件。 |
| `limit` | number | 否 | 限制返回数量。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `filters` | 实际使用的过滤条件。 |
| `total_upload_count` | 全部附件数。 |
| `matched_upload_count` | 匹配数量。 |
| `returned_upload_count` | 返回数量。 |
| `uploads` | 附件数组，含 ID、文件名、URL、类型、大小等。 |

**Example**

```javascript
return await markflow.listUploads({ kind: 'image', unused_only: true, limit: 20 })
```

## 5. deleteUploads

**应用场景**：用户明确确认删除附件。删除已引用附件可能导致正文图片/链接失效。

**入参**

至少传一个过滤条件：

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `upload_ids` | number[] | 条件 | 按 ID 删除。 |
| `kind` / `kinds` | string/string[] | 条件 | 按类型删除。 |
| `name_query` / `keyword` | string | 条件 | 按文件名关键字删除。 |
| `unused_only` | boolean | 条件 | 删除未引用附件。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `deleted` | 删除成功的附件。 |
| `failed` | 删除失败项及原因。 |
| `matched_upload_count` | 匹配数量。 |
| `remaining_upload_count` | 剩余附件数量。 |

**Example**

```javascript
return await markflow.deleteUploads({ upload_ids: [12, 15] })
```

## 6. updateProfile

**应用场景**：用户明确要求修改或清空头像。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `avatar` | string | 条件 | 直接设置头像 URL。 |
| `upload_id` | number | 条件 | 用已有图片附件作为头像。会校验附件是否为图片。 |
| `clear_avatar` | boolean | 条件 | 清空头像。不能与 `avatar/upload_id` 同传。 |

**出参**：更新后的用户资料。

**Example**

```javascript
return await markflow.updateProfile({ upload_id: 12 })
// 或
return await markflow.updateProfile({ clear_avatar: true })
```

## 7. getMarkdownEditorRuntime

**应用场景**：探测当前编辑器桥接对象是否可用、当前文档、可用方法。不要用它写整篇正文。

**入参**：无。

**出参**：编辑器桥接描述、当前文档摘要、全局对象说明、使用注意事项。

**Example**

```javascript
return await markflow.getMarkdownEditorRuntime()
```

## 8. getBrowserRuntime

**应用场景**：探测浏览器环境、URL、视口、存储 key 摘要。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `include_storage` | boolean | 否 | 是否返回 localStorage/sessionStorage key 摘要。默认 false。 |

**出参**：location、document、history、navigator、viewport、globals、storage 摘要。

**Example**

```javascript
return await markflow.getBrowserRuntime({ include_storage: false })
```
