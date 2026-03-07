<template>
  <div class="doc-tree">
    <div class="tree-header">
      <span class="tree-label">{{ props.projectName ? `${props.projectName} / 文档树` : '文档树' }}</span>
      <div class="tree-actions">
        <button class="icon-btn" title="新建文档" @click="createNode('doc')">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M9.75 2.854a.75.75 0 0 0-1.5 0V7.5H3.604a.75.75 0 0 0 0 1.5H8.25v4.646a.75.75 0 0 0 1.5 0V9h4.646a.75.75 0 0 0 0-1.5H9.75V2.854Z"/></svg>
        </button>
        <button class="icon-btn" title="新建目录" @click="createNode('dir')">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M.513 1.513A1.75 1.75 0 0 1 1.75 1h3.5c.55 0 1.07.26 1.4.7l.9 1.2a.25.25 0 0 0 .2.1H13.5A1.75 1.75 0 0 1 15.25 4.75v8a1.75 1.75 0 0 1-1.75 1.75H1.75A1.75 1.75 0 0 1 0 12.75V2.75c0-.464.184-.91.513-1.237Z"/></svg>
        </button>
        <button class="icon-btn" title="刷新" @click="docs.fetchTree(props.projectId || undefined)">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M1.705 8.005a.75.75 0 0 1 .834.656 5.5 5.5 0 0 0 9.592 2.97l-1.204-1.204a.25.25 0 0 1 .177-.427h3.646a.25.25 0 0 1 .25.25v3.646a.25.25 0 0 1-.427.177l-1.38-1.38A7.002 7.002 0 0 1 1.05 8.84a.75.75 0 0 1 .656-.834ZM8 2.5a5.487 5.487 0 0 0-4.131 1.869l1.204 1.204A.25.25 0 0 1 4.896 6H1.25A.25.25 0 0 1 1 5.75V2.104a.25.25 0 0 1 .427-.177l1.38 1.38A7.002 7.002 0 0 1 14.95 7.16a.75.75 0 0 1-1.49.178A5.5 5.5 0 0 0 8 2.5Z"/></svg>
        </button>
      </div>
    </div>

    <div class="tree-search">
      <svg class="search-icon" width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M10.68 11.74a6 6 0 0 1-7.922-8.982 6 6 0 0 1 8.982 7.922l3.04 3.04a.749.749 0 0 1-.326 1.275.749.749 0 0 1-.734-.215ZM11.5 7a4.499 4.499 0 1 0-8.997 0A4.499 4.499 0 0 0 11.5 7Z"/></svg>
      <input v-model="searchQuery" class="search-input" placeholder="搜索文档..." />
      <button v-if="searchQuery" class="search-clear" @click="searchQuery = ''">×</button>
    </div>

    <div class="tree-scroll" @contextmenu="openTreeCtx">
      <template v-if="filteredTree.length">
        <TreeNode
          v-for="node in filteredTree"
          :key="node.id"
          :node="node"
          :depth="0"
          :selected-id="docs.currentNode?.id"
          :expanded-ids="expandedIds"
          :force-expand="Boolean(searchQuery)"
          :dragging-id="draggingNodeId"
          :drop-target-id="dropState?.targetId || null"
          :drop-mode="dropState?.mode || null"
          @select="selectNode"
          @toggle="toggleExpand"
          @create="createUnder"
          @rename="startRename"
          @delete="deleteNode"
          @share="(n: DocNode) => emit('share', n)"
          @contextmenu="openCtx"
          @dragstart-node="onDragStart"
          @dragover-row="onDragOverRow"
          @drop-row="onDropRow"
          @dragend-node="onDragEnd"
        />
        <div
          class="root-drop-zone"
          :class="{ active: dropState?.mode === 'root' }"
          @dragover="onRootDragOver"
          @drop="onRootDrop"
        >
          <span v-if="draggingNodeId">拖到此处移动到根目录</span>
          <span v-else> </span>
        </div>
      </template>
      <div
        v-else-if="searchQuery"
        class="tree-empty"
        @dragover.prevent
        @drop.prevent
      >
        <svg width="28" height="28" viewBox="0 0 16 16" fill="var(--text3)"><path d="M10.68 11.74a6 6 0 0 1-7.922-8.982 6 6 0 0 1 8.982 7.922l3.04 3.04a.749.749 0 0 1-.326 1.275.749.749 0 0 1-.734-.215ZM11.5 7a4.499 4.499 0 1 0-8.997 0A4.499 4.499 0 0 0 11.5 7Z"/></svg>
        <span>未找到 "{{ searchQuery }}"</span>
      </div>
      <div v-else class="tree-empty" @dragover="onRootDragOver" @drop="onRootDrop">
        <svg width="28" height="28" viewBox="0 0 16 16" fill="var(--text3)"><path d="M0 3.75C0 2.784.784 2 1.75 2h12.5c.966 0 1.75.784 1.75 1.75v8.5A1.75 1.75 0 0 1 14.25 14H1.75A1.75 1.75 0 0 1 0 12.25ZM14.5 8.5h-13v3.75c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25Zm0-5H1.75a.25.25 0 0 0-.25.25V7h13Z"/></svg>
        <span>暂无文档</span>
        <button class="tree-empty-btn" @click="createNode('doc')">新建文档</button>
      </div>
    </div>

    <Teleport to="body">
      <div
        v-if="ctxMenu.visible"
        class="ctx-menu"
        :style="{ top: ctxMenu.y + 'px', left: ctxMenu.x + 'px' }"
        @mouseleave="closeCtx"
      >
        <template v-if="ctxMenu.node">
          <template v-if="ctxMenu.node.node_type === 'dir'">
            <button class="ctx-item" @click="ctxAction('new-doc')">
              <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M9.75 2.854a.75.75 0 0 0-1.5 0V7.5H3.604a.75.75 0 0 0 0 1.5H8.25v4.646a.75.75 0 0 0 1.5 0V9h4.646a.75.75 0 0 0 0-1.5H9.75V2.854Z"/></svg>
              新建文档
            </button>
            <button class="ctx-item" @click="ctxAction('new-dir')">
              <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M.513 1.513A1.75 1.75 0 0 1 1.75 1h3.5c.55 0 1.07.26 1.4.7l.9 1.2a.25.25 0 0 0 .2.1H13.5A1.75 1.75 0 0 1 15.25 4.75v8a1.75 1.75 0 0 1-1.75 1.75H1.75A1.75 1.75 0 0 1 0 12.75V2.75c0-.464.184-.91.513-1.237Z"/></svg>
              新建子目录
            </button>
            <div class="ctx-divider"></div>
          </template>
          <button class="ctx-item" @click="ctxAction('open')">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M1.75 2.5a.25.25 0 0 0-.25.25v10.5c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25v-8.5a.25.25 0 0 0-.25-.25H7.5c-.55 0-1.07-.26-1.4-.7L4.2 2.3a.25.25 0 0 0-.2-.1z"/></svg>
            打开
          </button>
          <button class="ctx-item" @click="ctxAction('rename')">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M11.013 1.427a1.75 1.75 0 0 1 2.474 0l1.086 1.086a1.75 1.75 0 0 1 0 2.474l-8.61 8.61c-.21.21-.47.364-.756.445l-3.251.93a.75.75 0 0 1-.927-.928l.929-3.25c.081-.286.235-.547.445-.758l8.61-8.61Z"/></svg>
            重命名
          </button>
          <button class="ctx-item" @click="ctxAction('share')">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="m7.775 3.275 1.25-1.25a3.5 3.5 0 1 1 4.95 4.95l-2.5 2.5a3.5 3.5 0 0 1-4.95 0 .751.751 0 0 1 .018-1.042.751.751 0 0 1 1.042-.018 2 2 0 0 0 2.83 0l2.5-2.5a2 2 0 0 0-2.83-2.83l-1.25 1.25a.751.751 0 0 1-1.042-.018.751.751 0 0 1-.018-1.042Zm-4.69 9.64a2 2 0 0 0 2.83 0l1.25-1.25a.751.751 0 0 1 1.042.018.751.751 0 0 1 .018 1.042l-1.25 1.25a3.5 3.5 0 1 1-4.95-4.95l2.5-2.5a3.5 3.5 0 0 1 4.95 0 .751.751 0 0 1-.018 1.042.751.751 0 0 1-1.042.018 2 2 0 0 0-2.83 0l-2.5 2.5a2 2 0 0 0 0 2.83Z"/></svg>
            分享
          </button>
          <div class="ctx-divider"></div>
          <button class="ctx-item ctx-item-danger" @click="ctxAction('delete')">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M11 1.75V3h2.25a.75.75 0 0 1 0 1.5H2.75a.75.75 0 0 1 0-1.5H5V1.75C5 .784 5.784 0 6.75 0h2.5C10.216 0 11 .784 11 1.75ZM4.496 6.675l.66 6.6a.25.25 0 0 0 .249.225h5.19a.25.25 0 0 0 .249-.225l.66-6.6a.75.75 0 0 1 1.492.149l-.66 6.6A1.748 1.748 0 0 1 10.595 15h-5.19a1.75 1.75 0 0 1-1.741-1.575l-.66-6.6a.75.75 0 1 1 1.492-.15ZM6.5 1.75V3h3V1.75a.25.25 0 0 0-.25-.25h-2.5a.25.25 0 0 0-.25.25Z"/></svg>
            删除
          </button>
        </template>
        <template v-else>
          <button class="ctx-item" @click="ctxAction('new-root-doc')">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M9.75 2.854a.75.75 0 0 0-1.5 0V7.5H3.604a.75.75 0 0 0 0 1.5H8.25v4.646a.75.75 0 0 0 1.5 0V9h4.646a.75.75 0 0 0 0-1.5H9.75V2.854Z"/></svg>
            新建文档
          </button>
          <button class="ctx-item" @click="ctxAction('new-root-dir')">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M.513 1.513A1.75 1.75 0 0 1 1.75 1h3.5c.55 0 1.07.26 1.4.7l.9 1.2a.25.25 0 0 0 .2.1H13.5A1.75 1.75 0 0 1 15.25 4.75v8a1.75 1.75 0 0 1-1.75 1.75H1.75A1.75 1.75 0 0 1 0 12.75V2.75c0-.464.184-.91.513-1.237Z"/></svg>
            新建目录
          </button>
          <div class="ctx-divider"></div>
          <button class="ctx-item" @click="ctxAction('refresh')">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M1.705 8.005a.75.75 0 0 1 .834.656 5.5 5.5 0 0 0 9.592 2.97l-1.204-1.204a.25.25 0 0 1 .177-.427h3.646a.25.25 0 0 1 .25.25v3.646a.25.25 0 0 1-.427.177l-1.38-1.38A7.002 7.002 0 0 1 1.05 8.84a.75.75 0 0 1 .656-.834ZM8 2.5a5.487 5.487 0 0 0-4.131 1.869l1.204 1.204A.25.25 0 0 1 4.896 6H1.25A.25.25 0 0 1 1 5.75V2.104a.25.25 0 0 1 .427-.177l1.38 1.38A7.002 7.002 0 0 1 14.95 7.16a.75.75 0 0 1-1.49.178A5.5 5.5 0 0 0 8 2.5Z"/></svg>
            刷新
          </button>
        </template>
      </div>
    </Teleport>

    <el-dialog v-model="showRename" title="重命名" width="320px" append-to-body destroy-on-close>
      <el-input v-model="renameValue" ref="renameInput" @keydown.enter="confirmRename" clearable />
      <template #footer>
        <el-button @click="showRename = false">取消</el-button>
        <el-button type="primary" @click="confirmRename">确认</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showCreate" :title="createType === 'dir' ? '新建目录' : '新建文档'" width="320px" append-to-body destroy-on-close>
      <el-input
        v-model="createName"
        :placeholder="createType === 'dir' ? '目录名称' : '文档标题'"
        :disabled="creating"
        @keydown.enter="confirmCreate"
        clearable
      />
      <template #footer>
        <el-button :disabled="creating" @click="showCreate = false">取消</el-button>
        <el-button type="primary" :loading="creating" @click="confirmCreate">创建</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, defineComponent, h, nextTick, onMounted, onUnmounted, ref, watch, type VNodeChild } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { useDocsStore, type DocNode } from '@/stores/docs'
import TreeNodeGlyph from '@/components/TreeNodeGlyph.vue'

const docs = useDocsStore()
const props = defineProps<{
  projectId?: number | null
  projectName?: string
}>()
const emit = defineEmits<{ share: [node: DocNode] }>()

const searchQuery = ref('')
const showRename = ref(false)
const renameValue = ref('')
const renameTarget = ref<DocNode | null>(null)
const showCreate = ref(false)
const createName = ref('')
const createType = ref<'doc' | 'dir'>('doc')
const createParent = ref<number | null>(null)
const creating = ref(false)
const renameInput = ref()
const expandedIds = ref<Set<number>>(new Set())
const knownDirIds = ref<Set<number>>(new Set())
const expansionInitialized = ref(false)
const cachedExpandedIds = ref<Set<number>>(new Set())
const hasExpandedPreference = ref(false)
const draggingNodeId = ref<number | null>(null)
const dragMoving = ref(false)

type DropMode = 'before' | 'inside' | 'after' | 'root'
const dropState = ref<{ targetId: number | null; mode: DropMode } | null>(null)

const ctxMenu = ref<{ visible: boolean; x: number; y: number; node: DocNode | null }>({
  visible: false,
  x: 0,
  y: 0,
  node: null,
})

function collectDirIds(nodes: DocNode[], set: Set<number>) {
  for (const node of nodes) {
    if (node.node_type === 'dir') {
      set.add(node.id)
      if (node.children?.length) collectDirIds(node.children, set)
    }
  }
}

const TREE_EXPANDED_CACHE_PREFIX = 'markflow.doc_tree.expanded.'

function getExpandedCacheKey() {
  return `${TREE_EXPANDED_CACHE_PREFIX}${props.projectId || 'global'}`
}

function loadExpandedPreference() {
  hasExpandedPreference.value = false
  cachedExpandedIds.value = new Set()
  try {
    const raw = localStorage.getItem(getExpandedCacheKey())
    if (!raw) return
    const parsed = JSON.parse(raw)
    if (Array.isArray(parsed)) {
      cachedExpandedIds.value = new Set(parsed.filter((id) => typeof id === 'number'))
      hasExpandedPreference.value = true
    }
  } catch {
    // ignore invalid cache
  }
}

function persistExpandedPreference() {
  localStorage.setItem(getExpandedCacheKey(), JSON.stringify(Array.from(expandedIds.value)))
}

function resetExpansionStateFromCache() {
  loadExpandedPreference()
  expandedIds.value = new Set(cachedExpandedIds.value)
  knownDirIds.value = new Set()
  expansionInitialized.value = false
}

watch(
  () => props.projectId,
  () => {
    resetExpansionStateFromCache()
  },
  { immediate: true }
)

watch(
  () => docs.tree,
  (tree) => {
    const currentDirIds = new Set<number>()
    collectDirIds(tree, currentDirIds)

    // When restoring from cache, avoid treating the initial empty tree snapshot as
    // a real state update. Otherwise cached expanded ids would be cleared on refresh.
    if (!expansionInitialized.value && hasExpandedPreference.value && tree.length === 0) {
      knownDirIds.value = currentDirIds
      return
    }

    const next = new Set([...expandedIds.value].filter((id) => currentDirIds.has(id)))

    if (!expansionInitialized.value) {
      if (hasExpandedPreference.value) {
        cachedExpandedIds.value.forEach((id) => {
          if (currentDirIds.has(id)) next.add(id)
        })
      } else {
        currentDirIds.forEach((id) => next.add(id))
      }
      expansionInitialized.value = true
    } else {
      currentDirIds.forEach((id) => {
        if (!knownDirIds.value.has(id)) next.add(id)
      })
    }

    expandedIds.value = next
    knownDirIds.value = currentDirIds
  },
  { deep: true, immediate: true }
)

watch(expandedIds, () => {
  persistExpandedPreference()
})

function openCtx(e: MouseEvent, node: DocNode) {
  e.preventDefault()
  e.stopPropagation()
  const x = Math.min(e.clientX, window.innerWidth - 190)
  const y = Math.min(e.clientY, window.innerHeight - 230)
  ctxMenu.value = { visible: true, x, y, node }
}

function openTreeCtx(e: MouseEvent) {
  const target = e.target as HTMLElement | null
  if (target?.closest('.tree-node-row')) return
  e.preventDefault()
  e.stopPropagation()
  const x = Math.min(e.clientX, window.innerWidth - 190)
  const y = Math.min(e.clientY, window.innerHeight - 180)
  ctxMenu.value = { visible: true, x, y, node: null }
}

function closeCtx() {
  ctxMenu.value.visible = false
}

function ctxAction(
  action: 'open' | 'rename' | 'share' | 'delete' | 'new-doc' | 'new-dir' | 'new-root-doc' | 'new-root-dir' | 'refresh'
) {
  const node = ctxMenu.value.node
  closeCtx()

  if (action === 'new-root-doc') {
    createNode('doc', null)
    return
  }
  if (action === 'new-root-dir') {
    createNode('dir', null)
    return
  }
  if (action === 'refresh') {
    docs.fetchTree(props.projectId || undefined)
    return
  }

  if (!node) return

  if (action === 'open') selectNode(node)
  else if (action === 'rename') startRename(node)
  else if (action === 'share') emit('share', node)
  else if (action === 'delete') deleteNode(node)
  else if (action === 'new-doc') createNode('doc', node.id)
  else if (action === 'new-dir') createNode('dir', node.id)
}

onMounted(() => {
  document.addEventListener('click', closeCtx)
})

onUnmounted(() => {
  document.removeEventListener('click', closeCtx)
})

function filterNodes(nodes: DocNode[], q: string): DocNode[] {
  if (!q) return nodes
  const lq = q.toLowerCase()

  return nodes.flatMap((node) => {
    const match = node.name.toLowerCase().includes(lq)
    const children = filterNodes(node.children || [], q)

    if (match) return [{ ...node, children: node.children || [] }]
    if (children.length) return [{ ...node, children }]
    return []
  })
}

const filteredTree = computed(() => filterNodes(docs.tree, searchQuery.value))

function findNodeById(nodes: DocNode[], id: number): DocNode | null {
  for (const node of nodes) {
    if (node.id === id) return node
    const found = findNodeById(node.children || [], id)
    if (found) return found
  }
  return null
}

function getChildrenByParentId(parentId: number | null): DocNode[] {
  if (!parentId) return docs.tree
  const parent = findNodeById(docs.tree, parentId)
  return parent?.children || []
}

function calcDropModeByPointer(event: DragEvent, isDir: boolean): Exclude<DropMode, 'root'> {
  const el = event.currentTarget as HTMLElement
  const rect = el.getBoundingClientRect()
  const y = event.clientY - rect.top
  const top = rect.height * 0.28
  const bottom = rect.height * 0.72

  if (y < top) return 'before'
  if (y > bottom) return 'after'
  return isDir ? 'inside' : 'after'
}

function getInsertIndex(
  parentId: number | null,
  targetId: number,
  mode: Exclude<DropMode, 'root'>,
  draggedId: number
): number {
  const destinationIds = getChildrenByParentId(parentId).map((n) => n.id).filter((id) => id !== draggedId)
  if (mode === 'inside') {
    return destinationIds.length
  }

  const targetIndex = destinationIds.indexOf(targetId)
  if (targetIndex < 0) return destinationIds.length
  return mode === 'before' ? targetIndex : targetIndex + 1
}

async function applyTreeMove(parentId: number | null, insertIndex: number) {
  const draggedId = draggingNodeId.value
  if (!draggedId || dragMoving.value) return

  const draggedNode = findNodeById(docs.tree, draggedId)
  if (!draggedNode) return

  if (parentId === draggedId) return

  const sourceParentId = draggedNode.parent_id || null
  const originalDestinationIds = getChildrenByParentId(parentId).map((n) => n.id)
  const destinationIds = originalDestinationIds.filter((id) => id !== draggedId)
  const clampedIndex = Math.max(0, Math.min(insertIndex, destinationIds.length))
  destinationIds.splice(clampedIndex, 0, draggedId)

  if (
    sourceParentId === parentId
    && destinationIds.length === originalDestinationIds.length
    && destinationIds.every((id, idx) => id === originalDestinationIds[idx])
  ) {
    onDragEnd()
    return
  }

  const sourceRemainderIds =
    sourceParentId !== parentId
      ? getChildrenByParentId(sourceParentId).map((n) => n.id).filter((id) => id !== draggedId)
      : []

  dragMoving.value = true
  try {
    for (let i = 0; i < destinationIds.length; i += 1) {
      await docs.moveNode(destinationIds[i], parentId, i, props.projectId, false)
    }

    if (sourceParentId !== parentId) {
      for (let i = 0; i < sourceRemainderIds.length; i += 1) {
        await docs.moveNode(sourceRemainderIds[i], sourceParentId, i, props.projectId, false)
      }
    }

    await docs.fetchTree(props.projectId)
    if (docs.currentNode?.id === draggedId) {
      await docs.fetchNode(draggedId)
    }
    ElMessage.success('文档已移动')
  } catch {
    ElMessage.error('移动失败')
  } finally {
    dragMoving.value = false
    onDragEnd()
  }
}

function onDragStart(event: DragEvent, node: DocNode) {
  if (searchQuery.value.trim()) {
    event.preventDefault()
    ElMessage.warning('搜索模式下不可拖拽，请先清空搜索')
    return
  }
  event.dataTransfer?.setData('text/plain', String(node.id))
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = 'move'
  }
  draggingNodeId.value = node.id
}

function onDragEnd() {
  draggingNodeId.value = null
  dropState.value = null
}

function onDragOverRow(event: DragEvent, node: DocNode, isDir: boolean) {
  if (!draggingNodeId.value || draggingNodeId.value === node.id) return
  event.preventDefault()
  event.stopPropagation()
  const mode = calcDropModeByPointer(event, isDir)
  dropState.value = { targetId: node.id, mode }
}

async function onDropRow(event: DragEvent, node: DocNode, isDir: boolean) {
  if (!draggingNodeId.value || draggingNodeId.value === node.id) return
  event.preventDefault()
  event.stopPropagation()

  const mode = calcDropModeByPointer(event, isDir)
  const effectiveMode = mode === 'inside' && !isDir ? 'after' : mode

  const parentId = effectiveMode === 'inside' ? node.id : (node.parent_id || null)
  const insertIndex = getInsertIndex(parentId, node.id, effectiveMode, draggingNodeId.value)
  await applyTreeMove(parentId, insertIndex)
}

function onRootDragOver(event: DragEvent) {
  if (!draggingNodeId.value) return
  event.preventDefault()
  dropState.value = { targetId: null, mode: 'root' }
}

async function onRootDrop(event: DragEvent) {
  if (!draggingNodeId.value) return
  event.preventDefault()
  const rootChildren = getChildrenByParentId(null).map((n) => n.id).filter((id) => id !== draggingNodeId.value)
  await applyTreeMove(null, rootChildren.length)
}

function selectNode(node: DocNode) {
  docs.fetchNode(node.id)
}

function toggleExpand(nodeId: number) {
  const next = new Set(expandedIds.value)
  if (next.has(nodeId)) next.delete(nodeId)
  else next.add(nodeId)
  expandedIds.value = next
}

function createNode(type: 'doc' | 'dir', parentId: number | null = null) {
  createType.value = type
  createParent.value = parentId
  createName.value = ''
  showCreate.value = true
}

function createUnder(parentId: number, type: 'doc' | 'dir') {
  createNode(type, parentId)
}

async function confirmCreate() {
  if (creating.value) return
  if (!createName.value.trim()) {
    ElMessage.warning('名称不能为空')
    return
  }

  creating.value = true
  try {
    const node = await docs.createNode({
      name: createName.value.trim(),
      node_type: createType.value,
      project_id: createParent.value ? undefined : props.projectId || undefined,
      parent_id: createParent.value || undefined,
    }, props.projectId)
    showCreate.value = false
    ElMessage({ message: '创建成功', type: 'success', duration: 1500 })

    if (createParent.value) {
      const next = new Set(expandedIds.value)
      next.add(createParent.value)
      expandedIds.value = next
    }

    selectNode(node)
  } catch {
    ElMessage.error('创建失败')
  } finally {
    creating.value = false
  }
}

function startRename(node: DocNode) {
  renameTarget.value = node
  renameValue.value = node.name
  showRename.value = true
  nextTick(() => renameInput.value?.focus())
}

async function confirmRename() {
  if (!renameValue.value.trim() || !renameTarget.value) return

  try {
    await docs.updateNode(renameTarget.value.id, { name: renameValue.value.trim() }, props.projectId)
    showRename.value = false
    ElMessage({ message: '重命名成功', type: 'success', duration: 1200 })
  } catch {
    ElMessage.error('重命名失败')
  }
}

async function deleteNode(node: DocNode) {
  try {
    await ElMessageBox.confirm(
      `确认删除「${node.name}」${node.node_type === 'dir' ? '及其所有内容' : ''}？`,
      '删除确认',
      {
        type: 'warning',
        confirmButtonText: '删除',
        cancelButtonText: '取消',
        confirmButtonClass: 'el-button--danger',
      }
    )

    await docs.deleteNode(node.id, props.projectId)
    ElMessage({ message: '已删除', type: 'success', duration: 1500 })
  } catch {
    // ignore cancel
  }
}

const TreeNode: any = defineComponent({
  name: 'TreeNode',
  props: {
    node: { type: Object as () => DocNode, required: true },
    depth: { type: Number, default: 0 },
    selectedId: Number,
    expandedIds: { type: Object as () => Set<number>, required: true },
    forceExpand: { type: Boolean, default: false },
    draggingId: { type: Number, default: null },
    dropTargetId: { type: Number, default: null },
    dropMode: { type: String as () => DropMode | null, default: null },
  },
  emits: [
    'select',
    'toggle',
    'create',
    'rename',
    'delete',
    'share',
    'contextmenu',
    'dragstart-node',
    'dragover-row',
    'drop-row',
    'dragend-node',
  ],
  setup(props, { emit }) {
    const hovered = ref(false)
    const isDir = computed(() => props.node.node_type === 'dir')
    const hasChildren = computed(() => (props.node.children?.length ?? 0) > 0)
    const expanded = computed(() => props.forceExpand || props.expandedIds.has(props.node.id))

    return (): VNodeChild => {
      const node = props.node
      const isSelected = props.selectedId === node.id
      const indent = props.depth * 14
      const isDraggingSelf = props.draggingId === node.id
      const isDropBefore = props.dropTargetId === node.id && props.dropMode === 'before'
      const isDropInside = props.dropTargetId === node.id && props.dropMode === 'inside'
      const isDropAfter = props.dropTargetId === node.id && props.dropMode === 'after'

      const rowEl: VNodeChild = h(
        'div',
        {
          class: [
            'tree-node-row',
            {
              'is-selected': isSelected,
              'is-dir': isDir.value,
              'is-dragging': isDraggingSelf,
              'drop-before': isDropBefore,
              'drop-inside': isDropInside,
              'drop-after': isDropAfter,
            },
          ],
          style: { paddingLeft: `${8 + indent}px` },
          draggable: true,
          onClick: () => emit('select', node),
          onDblclick: () => {
            if (isDir.value) emit('toggle', node.id)
          },
          onContextmenu: (e: MouseEvent) => emit('contextmenu', e, node),
          onDragstart: (e: DragEvent) => emit('dragstart-node', e, node),
          onDragover: (e: DragEvent) => emit('dragover-row', e, node, isDir.value),
          onDrop: (e: DragEvent) => emit('drop-row', e, node, isDir.value),
          onDragend: () => emit('dragend-node'),
          onMouseenter: () => (hovered.value = true),
          onMouseleave: () => (hovered.value = false),
        },
        [
          isDir.value
            ? h(
                'span',
                {
                  class: ['node-expand', { expanded: expanded.value }],
                  onClick: (e: Event) => {
                    e.stopPropagation()
                    emit('toggle', node.id)
                  },
                },
                [
                  h('svg', { width: '10', height: '10', viewBox: '0 0 16 16', fill: 'currentColor' }, [
                    h('path', {
                      d: 'M6.22 3.22a.75.75 0 0 1 1.06 0l4.25 4.25a.75.75 0 0 1 0 1.06l-4.25 4.25a.751.751 0 0 1-1.042-.018.751.751 0 0 1-.018-1.042L9.94 8 6.22 4.28a.75.75 0 0 1 0-1.06Z',
                    }),
                  ]),
                ]
              )
            : h('span', { style: { width: '14px', flexShrink: 0 } }),

          h(
            'span',
            { class: 'node-icon' },
            h(TreeNodeGlyph, {
              kind: isDir.value ? 'dir' : 'doc',
              expanded: expanded.value,
              active: isSelected,
              size: 18,
            })
          ),

          h('span', { class: 'node-label' }, node.name),

          hovered.value &&
            h(
              'div',
              {
                class: 'node-actions',
                onClick: (e: Event) => e.stopPropagation(),
              },
              [
                isDir.value &&
                  h(
                    'button',
                    {
                      class: 'na-btn',
                      title: '新建文档',
                      onClick: (e: Event) => {
                        e.stopPropagation()
                        emit('create', node.id, 'doc')
                      },
                    },
                    [
                      h('svg', { width: '12', height: '12', viewBox: '0 0 16 16', fill: 'currentColor' }, [
                        h('path', { d: 'M9.75 2.854a.75.75 0 0 0-1.5 0V7.5H3.604a.75.75 0 0 0 0 1.5H8.25v4.646a.75.75 0 0 0 1.5 0V9h4.646a.75.75 0 0 0 0-1.5H9.75V2.854Z' }),
                      ]),
                    ]
                  ),
                h(
                  'button',
                  {
                    class: 'na-btn',
                    title: '重命名',
                    onClick: (e: Event) => {
                      e.stopPropagation()
                      emit('rename', node)
                    },
                  },
                  [
                    h('svg', { width: '12', height: '12', viewBox: '0 0 16 16', fill: 'currentColor' }, [
                      h('path', { d: 'M11.013 1.427a1.75 1.75 0 0 1 2.474 0l1.086 1.086a1.75 1.75 0 0 1 0 2.474l-8.61 8.61c-.21.21-.47.364-.756.445l-3.251.93a.75.75 0 0 1-.927-.928l.929-3.25c.081-.286.235-.547.445-.758l8.61-8.61Z' }),
                    ]),
                  ]
                ),
                h(
                  'button',
                  {
                    class: 'na-btn',
                    title: '分享',
                    onClick: (e: Event) => {
                      e.stopPropagation()
                      emit('share', node)
                    },
                  },
                  [
                    h('svg', { width: '12', height: '12', viewBox: '0 0 16 16', fill: 'currentColor' }, [
                      h('path', { d: 'M4 6.997a.75.75 0 0 1 .75-.75h6.5a.75.75 0 1 1 0 1.5h-6.5A.75.75 0 0 1 4 6.997Zm0 2.5a.75.75 0 0 1 .75-.75h6.5a.75.75 0 1 1 0 1.5h-6.5a.75.75 0 0 1-.75-.75Z' }),
                    ]),
                  ]
                ),
                h(
                  'button',
                  {
                    class: 'na-btn na-btn-danger',
                    title: '删除',
                    onClick: (e: Event) => {
                      e.stopPropagation()
                      emit('delete', node)
                    },
                  },
                  [
                    h('svg', { width: '12', height: '12', viewBox: '0 0 16 16', fill: 'currentColor' }, [
                      h('path', { d: 'M11 1.75V3h2.25a.75.75 0 0 1 0 1.5H2.75a.75.75 0 0 1 0-1.5H5V1.75C5 .784 5.784 0 6.75 0h2.5C10.216 0 11 .784 11 1.75ZM4.496 6.675l.66 6.6a.25.25 0 0 0 .249.225h5.19a.25.25 0 0 0 .249-.225l.66-6.6a.75.75 0 0 1 1.492.149l-.66 6.6A1.748 1.748 0 0 1 10.595 15h-5.19a1.75 1.75 0 0 1-1.741-1.575l-.66-6.6a.75.75 0 1 1 1.492-.15ZM6.5 1.75V3h3V1.75a.25.25 0 0 0-.25-.25h-2.5a.25.25 0 0 0-.25.25Z' }),
                    ]),
                  ]
                ),
              ]
            ),
        ]
      )

      const children: VNodeChild = isDir.value && expanded.value && hasChildren.value
        ? h(
            'div',
            { class: 'tree-children' },
            (node.children || []).map((child) =>
              h(TreeNode, {
                key: child.id,
                node: child,
                depth: props.depth + 1,
                selectedId: props.selectedId,
                expandedIds: props.expandedIds,
                forceExpand: props.forceExpand,
                draggingId: props.draggingId,
                dropTargetId: props.dropTargetId,
                dropMode: props.dropMode,
                onSelect: (n: DocNode) => emit('select', n),
                onToggle: (id: number) => emit('toggle', id),
                onCreate: (pid: number, t: 'doc' | 'dir') => emit('create', pid, t),
                onRename: (n: DocNode) => emit('rename', n),
                onDelete: (n: DocNode) => emit('delete', n),
                onShare: (n: DocNode) => emit('share', n),
                onContextmenu: (e: MouseEvent, n: DocNode) => emit('contextmenu', e, n),
                onDragstartNode: (e: DragEvent, n: DocNode) => emit('dragstart-node', e, n),
                onDragoverRow: (e: DragEvent, n: DocNode, dir: boolean) => emit('dragover-row', e, n, dir),
                onDropRow: (e: DragEvent, n: DocNode, dir: boolean) => emit('drop-row', e, n, dir),
                onDragendNode: () => emit('dragend-node'),
              })
            )
          )
        : null

      return h('div', { class: 'tree-node' }, [rowEl, children])
    }
  },
})
</script>

<style scoped>
.doc-tree {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.tree-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px 6px;
  flex-shrink: 0;
}

.tree-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.7px;
  text-transform: uppercase;
  color: var(--text3);
}

.tree-actions {
  display: flex;
  gap: 2px;
}

.icon-btn {
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--text3);
  border-radius: var(--r-sm);
  cursor: pointer;
  transition: all 0.12s;
}

.icon-btn:hover {
  background: color-mix(in srgb, var(--blue) 14%, transparent);
  color: var(--text);
}

.tree-search {
  position: relative;
  padding: 0 8px 8px;
  flex-shrink: 0;
}

.search-icon {
  position: absolute;
  left: 18px;
  top: 50%;
  transform: translateY(-60%);
  color: var(--text3);
  pointer-events: none;
}

.search-input {
  width: 100%;
  padding: 5px 24px 5px 26px;
  background: color-mix(in srgb, var(--bg) 82%, var(--bg3));
  border: 1px solid transparent;
  border-radius: var(--r-sm);
  color: var(--text2);
  font-size: 12px;
  font-family: var(--font);
  outline: none;
  transition: all 0.15s;
}

.search-input:focus {
  border-color: var(--border);
  background: var(--bg3);
  color: var(--text);
}

.search-input::placeholder {
  color: var(--text3);
}

.search-clear {
  position: absolute;
  right: 14px;
  top: 50%;
  transform: translateY(-60%);
  border: none;
  background: transparent;
  color: var(--text3);
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
  padding: 0 2px;
}

.tree-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 0 6px 8px;
}

.root-drop-zone {
  margin-top: 8px;
  height: 26px;
  border: 1px dashed transparent;
  border-radius: var(--r-sm);
  display: flex;
  align-items: center;
  justify-content: center;
  color: transparent;
  font-size: 11px;
  transition: all 0.12s;
}

.root-drop-zone.active {
  border-color: var(--blue2);
  background: color-mix(in srgb, var(--blue) 14%, transparent);
  color: var(--blue2);
}

:deep(.tree-node) {
  margin-bottom: 1px;
}

:deep(.tree-node-row) {
  display: flex;
  align-items: center;
  gap: 5px;
  height: 30px;
  border-radius: var(--r-sm);
  cursor: pointer;
  padding-right: 6px;
  transition: background 0.1s;
  position: relative;
  overflow: hidden;
}

:deep(.tree-node-row:hover) {
  background: color-mix(in srgb, var(--blue) 11%, transparent);
}

:deep(.tree-node-row.is-dragging) {
  opacity: 0.55;
}

:deep(.tree-node-row.drop-before) {
  box-shadow: inset 0 2px 0 var(--blue2);
}

:deep(.tree-node-row.drop-after) {
  box-shadow: inset 0 -2px 0 var(--blue2);
}

:deep(.tree-node-row.drop-inside) {
  background: color-mix(in srgb, var(--blue) 18%, transparent);
}

:deep(.tree-node-row.is-selected) {
  background: var(--green-dim);
}

:deep(.tree-node-row.is-selected .node-label) {
  color: var(--green3);
  font-weight: 500;
}

:deep(.node-expand) {
  width: 14px;
  height: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text3);
  flex-shrink: 0;
  transition: transform 0.15s;
}

:deep(.node-expand.expanded) {
  transform: rotate(90deg);
}

:deep(.node-icon) {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
}

:deep(.node-label) {
  flex: 1;
  font-size: 13px;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

:deep(.node-actions) {
  display: flex;
  align-items: center;
  gap: 1px;
  flex-shrink: 0;
}

:deep(.na-btn) {
  width: 22px;
  height: 22px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--text3);
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.1s;
}

:deep(.na-btn:hover) {
  background: color-mix(in srgb, var(--blue) 18%, transparent);
  color: var(--text);
}

:deep(.na-btn-danger:hover) {
  background: rgba(248, 81, 73, 0.15);
  color: var(--red);
}

:deep(.tree-children) {
  margin-top: 1px;
}

.tree-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 40px 16px;
  color: var(--text3);
  font-size: 12px;
  text-align: center;
}

.tree-empty-btn {
  margin-top: 4px;
  padding: 5px 14px;
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text2);
  border-radius: var(--r-sm);
  font-size: 12px;
  cursor: pointer;
  font-family: var(--font);
  transition: all 0.12s;
}

.tree-empty-btn:hover {
  border-color: var(--green2);
  color: var(--green3);
  background: var(--green-dim);
}

.ctx-menu {
  position: fixed;
  z-index: 9999;
  min-width: 168px;
  background: var(--bg2);
  border: 1px solid var(--border);
  border-radius: var(--r-md);
  padding: 4px;
  box-shadow: var(--shadow-lg);
  animation: ctx-in 0.1s ease;
}

@keyframes ctx-in {
  from {
    opacity: 0;
    transform: scale(0.96) translateY(-4px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}

.ctx-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 10px;
  border: none;
  background: transparent;
  color: var(--text);
  font-size: 13px;
  font-family: var(--font);
  border-radius: var(--r-sm);
  cursor: pointer;
  text-align: left;
  transition: background 0.1s;
}

.ctx-item:hover {
  background: color-mix(in srgb, var(--blue) 12%, transparent);
}

.ctx-item svg {
  color: var(--text3);
  flex-shrink: 0;
}

.ctx-item-danger {
  color: var(--red);
}

.ctx-item-danger svg {
  color: var(--red);
}

.ctx-item-danger:hover {
  background: rgba(248, 81, 73, 0.1);
}

.ctx-divider {
  height: 1px;
  background: var(--border);
  margin: 3px 0;
}
</style>
