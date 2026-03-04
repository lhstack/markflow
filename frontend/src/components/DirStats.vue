<template>
  <div class="dir-view">
    <div class="dir-header">
      <el-icon class="dir-icon"><Folder /></el-icon>
      <div class="dir-info">
        <h1 class="dir-title">{{ node.name }}</h1>
        <p class="dir-path">创建于 {{ formatDate(node.created_at) }}</p>
      </div>
      <div class="dir-actions">
        <el-button :icon="Share" @click="emit('share', node)">分享</el-button>
      </div>
    </div>

    <div class="dir-stats">
      <div class="stat-card">
        <div class="stat-value">{{ safeStats.doc_count }}</div>
        <div class="stat-label">文档</div>
      </div>
      <div class="stat-card">
        <div class="stat-value">{{ safeStats.dir_count }}</div>
        <div class="stat-label">子目录</div>
      </div>
      <div class="stat-card">
        <div class="stat-value">{{ node.updated_at ? formatDate(node.updated_at) : '-' }}</div>
        <div class="stat-label">最后修改</div>
      </div>
    </div>

    <div v-if="node.children?.length" class="dir-children">
      <h3 class="section-title">内容</h3>
      <div class="children-grid">
        <div v-for="child in node.children" :key="child.id" class="child-card" @click="emit('select', child)">
          <el-icon class="child-icon">
            <Folder v-if="child.node_type === 'dir'" />
            <Document v-else />
          </el-icon>
          <span class="child-name">{{ child.name }}</span>
          <span class="child-meta">{{ child.node_type === 'dir' ? '目录' : '文档' }}</span>
        </div>
      </div>
    </div>

    <div v-else class="dir-empty">
      <el-icon><FolderOpened /></el-icon>
      <p>当前目录下无直接子项</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Share } from '@element-plus/icons-vue'
import type { DocNode } from '@/stores/docs'

const props = defineProps<{
  node: DocNode
  stats?: { doc_count: number; dir_count: number }
}>()

const emit = defineEmits<{
  share: [node: DocNode]
  select: [node: DocNode]
}>()

const safeStats = computed(() => props.stats ?? { doc_count: 0, dir_count: 0 })

function formatDate(dateStr: string) {
  const d = new Date(dateStr.endsWith('Z') ? dateStr : `${dateStr}Z`)
  if (Number.isNaN(d.getTime())) return '-'
  return d.toLocaleDateString('zh-CN')
}
</script>

<style scoped>
.dir-view {
  padding: 32px 40px;
  max-width: 900px;
  margin: 0 auto;
}

.dir-header {
  display: flex;
  align-items: flex-start;
  gap: 16px;
  margin-bottom: 28px;
}

.dir-icon {
  font-size: 44px;
  color: #d4a72c;
  flex-shrink: 0;
  margin-top: 2px;
}

.dir-info {
  flex: 1;
  min-width: 0;
}

.dir-title {
  font-size: 26px;
  font-weight: 700;
  color: var(--text);
  margin: 0 0 6px;
}

.dir-path {
  font-size: 13px;
  color: var(--text3);
  margin: 0;
}

.dir-stats {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 12px;
  margin-bottom: 32px;
}

.stat-card {
  background: var(--bg2);
  border: 1px solid var(--border);
  border-radius: var(--r-md);
  padding: 16px 18px;
}

.stat-value {
  font-size: 24px;
  font-weight: 700;
  color: var(--text);
  font-family: var(--mono);
  line-height: 1.1;
}

.stat-label {
  margin-top: 6px;
  font-size: 12px;
  color: var(--text3);
}

.section-title {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.6px;
  text-transform: uppercase;
  color: var(--text3);
  margin: 0 0 14px;
}

.children-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 12px;
}

.child-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 18px 14px;
  background: var(--bg2);
  border: 1px solid var(--border);
  border-radius: var(--r-md);
  cursor: pointer;
  transition: all 0.15s ease;
  text-align: center;
}

.child-card:hover {
  border-color: var(--border2);
  background: color-mix(in srgb, var(--blue) 6%, var(--bg2));
  transform: translateY(-1px);
}

.child-icon {
  font-size: 30px;
  color: #d4a72c;
}

.child-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}

.child-meta {
  font-size: 11px;
  color: var(--text3);
}

.dir-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 60px;
  color: var(--text3);
}

.dir-empty .el-icon {
  font-size: 44px;
}
</style>
