// 悬浮面板的位置 / 折叠状态 composable。
//
// 从 AgentPanel 抽出：持有面板坐标、折叠态、拖拽逻辑与本地持久化。
// 会话恢复不在此处（属于会话职责），组件通过 loadPanelState() 拿到持久化的
// 原始快照后自行处理 currentSessionId 恢复。

import { computed, ref } from 'vue'

const PANEL_STATE_KEY = 'markflow.agent.panel.state'
const VIEWPORT_MARGIN = 24
const FAB_WIDTH = 96
const FAB_HEIGHT = 48
const PANEL_WIDTH = 440
const PANEL_HEIGHT = 760

interface PanelStateSnapshot {
  collapsed: boolean
  panelX: number
  panelY: number
  expandedPanelX: number
  expandedPanelY: number
  collapsedPanelX: number
  collapsedPanelY: number
  currentSessionId: string
}

function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw ? JSON.parse(raw) as T : fallback
  } catch {
    return fallback
  }
}

export function usePanelPosition(getSessionId: () => string = () => '') {
  const collapsed = ref(false)
  const panelX = ref(0)
  const panelY = ref(88)
  const expandedPanelX = ref(0)
  const expandedPanelY = ref(88)
  const collapsedPanelX = ref(0)
  const collapsedPanelY = ref(88)

  let dragOffsetX = 0
  let dragOffsetY = 0
  let dragging = false
  let didDrag = false

  const panelStyle = computed(() => ({
    transform: `translate(${panelX.value}px, ${panelY.value}px)`,
  }))

  function maxPanelXFor(nextCollapsed: boolean) {
    const width = nextCollapsed ? FAB_WIDTH : PANEL_WIDTH
    return Math.max(VIEWPORT_MARGIN, window.innerWidth - width - VIEWPORT_MARGIN)
  }

  function maxPanelYFor(nextCollapsed: boolean) {
    const estimatedHeight = nextCollapsed
      ? FAB_HEIGHT
      : Math.min(PANEL_HEIGHT, Math.max(420, window.innerHeight - 112))
    return Math.max(VIEWPORT_MARGIN, window.innerHeight - estimatedHeight - VIEWPORT_MARGIN)
  }

  function clampPanelPosition(nextX: number, nextY: number, nextCollapsed: boolean) {
    return {
      x: Math.max(VIEWPORT_MARGIN, Math.min(maxPanelXFor(nextCollapsed), nextX)),
      y: Math.max(VIEWPORT_MARGIN, Math.min(maxPanelYFor(nextCollapsed), nextY)),
    }
  }

  function defaultPanelState(nextCollapsed = true): PanelStateSnapshot {
    const expandedPosition = clampPanelPosition(
      window.innerWidth - PANEL_WIDTH - VIEWPORT_MARGIN,
      window.innerHeight - PANEL_HEIGHT - VIEWPORT_MARGIN,
      false,
    )
    const collapsedPosition = clampPanelPosition(
      window.innerWidth - FAB_WIDTH - VIEWPORT_MARGIN,
      window.innerHeight - FAB_HEIGHT - VIEWPORT_MARGIN,
      true,
    )
    return {
      collapsed: nextCollapsed,
      panelX: nextCollapsed ? collapsedPosition.x : expandedPosition.x,
      panelY: nextCollapsed ? collapsedPosition.y : expandedPosition.y,
      expandedPanelX: expandedPosition.x,
      expandedPanelY: expandedPosition.y,
      collapsedPanelX: collapsedPosition.x,
      collapsedPanelY: collapsedPosition.y,
      currentSessionId: '',
    }
  }

  function toggleCollapse(nextCollapsed: boolean) {
    if (collapsed.value === nextCollapsed) return
    if (collapsed.value) {
      collapsedPanelX.value = panelX.value
      collapsedPanelY.value = panelY.value
    } else {
      expandedPanelX.value = panelX.value
      expandedPanelY.value = panelY.value
    }

    const nextPosition = clampPanelPosition(
      nextCollapsed ? collapsedPanelX.value : expandedPanelX.value,
      nextCollapsed ? collapsedPanelY.value : expandedPanelY.value,
      nextCollapsed,
    )
    panelX.value = nextPosition.x
    panelY.value = nextPosition.y
    collapsed.value = nextCollapsed
  }

  function persistPanelState(currentSessionId: string) {
    localStorage.setItem(
      PANEL_STATE_KEY,
      JSON.stringify({
        collapsed: collapsed.value,
        panelX: panelX.value,
        panelY: panelY.value,
        expandedPanelX: expandedPanelX.value,
        expandedPanelY: expandedPanelY.value,
        collapsedPanelX: collapsedPanelX.value,
        collapsedPanelY: collapsedPanelY.value,
        currentSessionId,
      }),
    )
  }

  function startDrag(event: MouseEvent) {
    dragging = true
    didDrag = false
    dragOffsetX = event.clientX - panelX.value
    dragOffsetY = event.clientY - panelY.value
    window.addEventListener('mousemove', onDrag)
    window.addEventListener('mouseup', stopDrag)
  }

  function onDrag(event: MouseEvent) {
    if (!dragging) return
    didDrag = true
    const nextX = event.clientX - dragOffsetX
    const nextY = event.clientY - dragOffsetY
    const clamped = clampPanelPosition(nextX, nextY, collapsed.value)
    panelX.value = clamped.x
    panelY.value = clamped.y
    if (collapsed.value) {
      collapsedPanelX.value = clamped.x
      collapsedPanelY.value = clamped.y
    } else {
      expandedPanelX.value = clamped.x
      expandedPanelY.value = clamped.y
    }
  }

  function stopDrag() {
    dragging = false
    window.removeEventListener('mousemove', onDrag)
    window.removeEventListener('mouseup', stopDrag)
    persistPanelState(getSessionId())
  }

  function handleFabClick() {
    if (didDrag) {
      didDrag = false
      return
    }
    toggleCollapse(false)
  }

  /** 从本地恢复面板位置，返回持久化快照（含 currentSessionId 供组件恢复会话）。 */
  function loadPanelState(): PanelStateSnapshot {
    const panelState = loadJson(PANEL_STATE_KEY, defaultPanelState(true))
    const defaultExpanded = clampPanelPosition(
      Number.isFinite(panelState.expandedPanelX) ? panelState.expandedPanelX : defaultPanelState(false).panelX,
      Number.isFinite(panelState.expandedPanelY) ? panelState.expandedPanelY : defaultPanelState(false).panelY,
      false,
    )
    const defaultCollapsed = clampPanelPosition(
      Number.isFinite(panelState.collapsedPanelX) ? panelState.collapsedPanelX : defaultPanelState(true).panelX,
      Number.isFinite(panelState.collapsedPanelY) ? panelState.collapsedPanelY : defaultPanelState(true).panelY,
      true,
    )
    collapsed.value = Boolean(panelState.collapsed)
    expandedPanelX.value = defaultExpanded.x
    expandedPanelY.value = defaultExpanded.y
    collapsedPanelX.value = defaultCollapsed.x
    collapsedPanelY.value = defaultCollapsed.y
    const initialPosition = collapsed.value ? defaultCollapsed : defaultExpanded
    panelX.value = initialPosition.x
    panelY.value = initialPosition.y
    return panelState
  }

  /** 供 watch 使用：把当前坐标夹回可视区域并持久化。 */
  function reclampPanelPosition(currentSessionId: string) {
    const clamped = clampPanelPosition(panelX.value, panelY.value, collapsed.value)
    if (clamped.x !== panelX.value) panelX.value = clamped.x
    if (clamped.y !== panelY.value) panelY.value = clamped.y
    if (collapsed.value) {
      collapsedPanelX.value = panelX.value
      collapsedPanelY.value = panelY.value
    } else {
      expandedPanelX.value = panelX.value
      expandedPanelY.value = panelY.value
    }
    persistPanelState(currentSessionId)
  }

  return {
    collapsed,
    panelX,
    panelY,
    panelStyle,
    toggleCollapse,
    persistPanelState,
    startDrag,
    onDrag,
    stopDrag,
    handleFabClick,
    loadPanelState,
    reclampPanelPosition,
  }
}
