# 项目与文档树操作 Reference

本文件描述 `markflow` 中的项目和文档树方法。全部通过 `execute_browser_javascript` 调用：

```javascript
return await markflow.methodName({ /* args */ })
```

通用规则：

- 定位优先级：`*_id` > `*_path` > `*_name`。
- 创建、移动、删除、重命名前，先 `listProjects` / `getProjectTree` 确认真实结构。
- 删除是破坏性操作，必须有用户明确确认。
- 创建文档只创建空文档；正文首稿/追加/整篇覆盖通过 document reference 中的 markflow.appendCurrentDocumentContent / markflow.replaceCurrentDocumentContent 完成。

## 1. listProjects

**应用场景**：需要知道有哪些项目、当前项目 ID、按名称定位项目时。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `refresh` | boolean | 否 | 是否强制刷新项目列表。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `projects` | 项目数组，元素含 `id/name/description/...`。 |
| `total` | 项目数量。 |
| `current_project_id` | 当前打开项目 ID，可能为 `null`。 |

**Example**

```javascript
return await markflow.listProjects({ refresh: true })
```

## 2. openProject

**应用场景**：切换到某个项目工作区。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `project_id` | number | 条件 | 项目 ID，优先使用。 |
| `project_name` | string | 条件 | 项目名，拿不到 ID 时使用。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `project` | 打开的项目。 |
| `current_project_id` | 当前项目 ID。 |
| `route`/`view` | 页面状态摘要。 |

**Example**

```javascript
return await markflow.openProject({ project_id: 12 })
```

## 3. createProject

**应用场景**：用户明确要求新建项目。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `name` | string | 是 | 项目名称。 |
| `description` | string | 否 | 项目描述。 |
| `open_after_create` | boolean | 否 | 创建后是否打开，默认 `true`。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `project` | 新建项目。 |
| `opened` | 是否已切换打开。 |
| `current_project_id` | 当前项目 ID。 |

**Example**

```javascript
return await markflow.createProject({
  name: '产品知识库',
  description: 'PRD、接口与设计资料',
  open_after_create: true
})
```

## 4. updateProject

**应用场景**：重命名项目、修改项目描述或背景等项目元信息。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `project_id` | number | 条件 | 项目 ID。 |
| `project_name` | string | 条件 | 项目名。 |
| `name` | string | 否 | 新项目名称。 |
| `description` | string | 否 | 新描述。 |
| `background`/`background_url` | string | 否 | 背景图 URL（如当前实现支持）。 |

**出参**：更新后的 `project` 与当前项目状态。

**Example**

```javascript
return await markflow.updateProject({
  project_id: 12,
  name: '产品知识库 v2',
  description: '新版说明'
})
```

## 5. deleteProjects

**应用场景**：用户明确确认删除项目。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `project_ids` | number[] | 条件 | 要删除的项目 ID 列表。 |
| `project_names` | string[] | 条件 | 要删除的项目名列表。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `deleted` | 成功删除的项目。 |
| `failed` | 删除失败项目及原因。 |
| `remaining_project_count` | 剩余项目数量。 |

**Example**

```javascript
return await markflow.deleteProjects({ project_ids: [12, 15] })
```

## 6. getProjectTree

**应用场景**：任何文档树结构操作前必读；按路径/名称定位节点时也要先读树。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `project_id` | number | 条件 | 项目 ID。省略时使用当前项目。 |
| `project_name` | string | 条件 | 项目名。 |
| `refresh` | boolean | 否 | 节点刚增删移后建议 `true`。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `project` | 项目摘要。 |
| `tree` | 树结构。 |
| `nodes`/`flat_nodes` | 扁平节点列表，含 ID、名称、路径、类型、父节点。 |
| `stats` | 文档/目录数量等统计。 |

**Example**

```javascript
return await markflow.getProjectTree({ project_id: 12, refresh: true })
```

## 7. createTreeNode

**应用场景**：创建目录或空文档节点。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `project_id` / `project_name` | number/string | 条件 | 目标项目。 |
| `parent_id` | number | 否 | 父目录 ID。根目录不要传 `null/0/''`，直接省略。 |
| `parent_path` | string | 否 | 父目录路径，拿不到 ID 时用。 |
| `parent_name` | string | 否 | 父目录名，最后兜底。 |
| `name` | string | 是 | 新节点名称。 |
| `node_type` | `'doc' | 'dir'` | 是 | 文档或目录。 |
| `open_after_create` | boolean | 否 | 默认 `true`。创建文档后通常打开，方便后续调用正文写入方法。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `node` | 新建节点。 |
| `project` | 所属项目。 |
| `opened` | 是否打开。 |
| `parent_id`/`path` | 位置摘要。 |

**Example**

```javascript
return await markflow.createTreeNode({
  project_id: 12,
  parent_id: 34,
  name: 'API 说明',
  node_type: 'doc',
  open_after_create: true
})
```

## 8. moveTreeNode

**应用场景**：移动文档或目录到另一个目录/根目录。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `project_id` / `project_name` | number/string | 条件 | 所属项目。不支持跨项目移动。 |
| `node_id` / `node_path` / `node_name` | number/string | 是 | 源节点定位。 |
| `target_parent_id` | number | 条件 | 目标父目录 ID。 |
| `target_parent_path` | string | 条件 | 目标父目录路径。 |
| `target_parent_name` | string | 条件 | 目标父目录名称。 |
| `to_root` | boolean | 条件 | 移动到项目根目录时传 `true`，不要再传 target_parent。 |
| `sort_order` | number | 否 | 排序位置，默认追加。 |

**出参**：移动后的节点、旧父节点、新父节点、路径摘要。

**Example**

```javascript
return await markflow.moveTreeNode({
  project_id: 12,
  node_id: 34,
  target_parent_id: 56
})
```

## 9. openTreeNode

**应用场景**：打开文档或目录；打开文档时会等待编辑器初始化。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `node_id` | number | 条件 | 节点 ID。 |
| `node_path` | string | 条件 | 节点路径。 |
| `node_name` | string | 条件 | 节点名。 |
| `project_id`/`project_name` | number/string | 否 | 按路径/名称定位时建议传项目。 |

**出参**：打开的节点、项目、是否编辑器可用、当前路由状态。

**Example**

```javascript
return await markflow.openTreeNode({ node_id: 34 })
```

## 10. updateTreeNodeMeta

**应用场景**：重命名文档或目录。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `node_id` / `node_path` / `node_name` | number/string | 是 | 目标节点。 |
| `project_id` / `project_name` | number/string | 否 | 按路径/名称定位时建议传。 |
| `new_name` | string | 是 | 新名称。也兼容 `name`。 |

**出参**：更新后的节点、旧名称、新名称、路径摘要。

**Example**

```javascript
return await markflow.updateTreeNodeMeta({ node_id: 34, new_name: '接口说明' })
```

## 11. deleteTreeNodes

**应用场景**：用户明确确认删除文档/目录。

**入参**

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `project_id` / `project_name` | number/string | 条件 | 所属项目。 |
| `node_ids` | number[] | 条件 | 删除目标 ID 列表。 |
| `node_paths` | string[] | 条件 | 删除目标路径列表。 |
| `node_names` | string[] | 条件 | 删除目标名称列表。 |

**出参**

| 字段 | 说明 |
| --- | --- |
| `deleted` | 删除成功节点。 |
| `failed` | 删除失败节点及原因。 |
| `remaining_node_count` | 剩余节点数。 |

**Example**

```javascript
return await markflow.deleteTreeNodes({ project_id: 12, node_ids: [34, 35] })
```
