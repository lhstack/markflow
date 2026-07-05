<script setup lang="ts">
// 工具调用卡片：展示一条 assistant 消息在本轮里发起的所有工具调用。
//
// 折叠态默认只占约一卡高度，超过阈值后列表内部滚动；每个工具项可点击展开，
// 查看入参 / 出参。数据来源于 message.toolEvents（后端 tool_event 归一化后）。

import { ref } from 'vue'

import type { AgentToolEvent } from '@/components/agent/types'

defineProps<{ events: AgentToolEvent[] }>()

const expanded = ref<Set<string>>(new Set())

function toggle(id: string) {
  const next = new Set(expanded.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  expanded.value = next
}

function statusLabel(status: string): string {
  switch (status) {
    case 'requested': return '请求中'
    case 'running': return '执行中'
    case 'completed': return '完成'
    case 'failed': return '失败'
    case 'noop': return '无变更'
    default: return status
  }
}

function statusClass(status: string): string {
  if (status === 'failed') return 'is-failed'
  if (status === 'completed') return 'is-ok'
  if (status === 'noop') return 'is-noop'
  return 'is-pending'
}
</script>

<template>
  <details class="tool-events" open>
    <summary>
      <span class="tool-events-title">工具调用</span>
      <span class="tool-events-count">{{ events.length }}</span>
    </summary>

    <div class="tool-events-scroll">
      <div
        v-for="event in events"
        :key="event.id"
        class="tool-event"
        :class="{ open: expanded.has(event.id) }"
      >
        <button type="button" class="tool-event-head" @click="toggle(event.id)">
          <span class="tool-event-caret">▸</span>
          <span class="tool-event-name">{{ event.tool }}</span>
          <span v-if="event.source" class="tool-event-source">{{ event.source }}</span>
          <span class="tool-event-status" :class="statusClass(event.status)">{{ statusLabel(event.status) }}</span>
        </button>

        <div class="tool-event-summary">{{ event.summary }}</div>

        <div v-if="expanded.has(event.id)" class="tool-event-detail">
          <div v-if="event.arguments" class="tool-event-block">
            <div class="tool-event-block-label">入参</div>
            <pre class="tool-event-code">{{ event.arguments }}</pre>
          </div>
          <div v-if="event.output" class="tool-event-block">
            <div class="tool-event-block-label">出参</div>
            <pre class="tool-event-code">{{ event.output }}</pre>
          </div>
          <div v-if="!event.arguments && !event.output" class="tool-event-empty">无入参 / 出参记录</div>
        </div>
      </div>
    </div>
  </details>
</template>

<style scoped>
.tool-events {
  width: 100%;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.84);
  border: 1px solid rgba(122, 147, 91, 0.14);
  overflow: hidden;
}

.tool-events > summary {
  list-style: none;
  cursor: pointer;
  user-select: none;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  font-size: 12px;
  font-weight: 700;
  color: #607057;
}

.tool-events > summary::-webkit-details-marker {
  display: none;
}

.tool-events > summary::before {
  content: '▸';
  color: #6f9a4f;
}

.tool-events[open] > summary::before {
  content: '▾';
}

.tool-events-count {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  border-radius: 999px;
  background: rgba(111, 154, 79, 0.16);
  color: #557036;
  font-size: 11px;
  font-weight: 700;
}

/* 默认一卡高度，最多约三卡后内部滚动。单卡折叠态约 62px。 */
.tool-events-scroll {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-height: 210px;
  overflow-y: auto;
  padding: 0 10px 10px;
  scrollbar-width: thin;
  scrollbar-color: rgba(111, 154, 79, 0.5) transparent;
}

.tool-events-scroll::-webkit-scrollbar {
  width: 8px;
}

.tool-events-scroll::-webkit-scrollbar-thumb {
  background: rgba(111, 154, 79, 0.45);
  border-radius: 999px;
}

.tool-event {
  border-radius: 10px;
  background: rgba(248, 250, 243, 0.92);
  border: 1px solid rgba(122, 147, 91, 0.12);
  padding: 8px 10px;
}

.tool-event-head {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  border: none;
  background: transparent;
  padding: 0;
  cursor: pointer;
  text-align: left;
}

.tool-event-caret {
  color: #6f9a4f;
  font-size: 11px;
  transition: transform 0.15s ease;
}

.tool-event.open .tool-event-caret {
  transform: rotate(90deg);
}

.tool-event-name {
  min-width: 0;
  flex: 1;
  font-size: 12px;
  font-weight: 700;
  color: #31402b;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tool-event-source {
  flex-shrink: 0;
  font-size: 10px;
  color: #8a9a7d;
  text-transform: uppercase;
}

.tool-event-status {
  flex-shrink: 0;
  font-size: 11px;
  font-weight: 600;
}

.tool-event-status.is-ok { color: #4f8a3d; }
.tool-event-status.is-failed { color: #c0563f; }
.tool-event-status.is-noop { color: #8a9a7d; }
.tool-event-status.is-pending { color: #c08a3d; }

.tool-event-summary {
  margin-top: 4px;
  font-size: 11px;
  line-height: 1.55;
  color: #4d5b44;
}

.tool-event-detail {
  margin-top: 8px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.tool-event-block-label {
  font-size: 11px;
  font-weight: 700;
  color: #6f7e65;
  margin-bottom: 3px;
}

.tool-event-code {
  margin: 0;
  padding: 8px;
  border-radius: 8px;
  background: rgba(240, 244, 234, 0.92);
  border: 1px solid rgba(122, 147, 91, 0.1);
  white-space: pre-wrap;
  word-break: break-word;
  font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
  font-size: 11px;
  line-height: 1.55;
  color: #3c4a34;
  max-height: 220px;
  overflow-y: auto;
}

.tool-event-empty {
  font-size: 11px;
  color: #8a9a7d;
}
</style>
