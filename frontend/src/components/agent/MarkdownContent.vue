<script setup lang="ts">
// 消息内容 Markdown 渲染：拆分 markdown / html 块，html 块走 HtmlPreviewBlock 预览，
// 其余走 marked + DOMPurify。代码块工具栏（复制/下载）和图片点击预览由 handleMarkdownClick 处理。

import { computed } from 'vue'

import { handleMarkdownClick, parseRenderedParts, renderMarkdownText } from './markdown'
import HtmlPreviewBlock from './HtmlPreviewBlock.vue'

const props = defineProps<{
  content: string
}>()

const emit = defineEmits<{
  (event: 'preview-image', url: string): void
}>()

const parts = computed(() => parseRenderedParts(props.content))

function onClick(event: MouseEvent) {
  handleMarkdownClick(event, (url) => emit('preview-image', url))
}
</script>

<template>
  <div class="markdown-body" @click="onClick">
    <template v-for="(part, index) in parts" :key="index">
      <HtmlPreviewBlock v-if="part.type === 'html'" :source="part.content" />
      <div v-else v-html="renderMarkdownText(part.content)" />
    </template>
  </div>
</template>

<style scoped>
.markdown-body {
  box-sizing: border-box;
  width: 100%;
  max-width: 100%;
  min-width: 0;
  white-space: normal;
  color: #24311f;
  font-size: 13px;
  line-height: 1.6;
}

.markdown-body :deep(:first-child) {
  margin-top: 0;
}
.markdown-body :deep(:last-child) {
  margin-bottom: 0;
}
.markdown-body :deep(> *) {
  box-sizing: border-box;
  max-width: 100%;
  min-width: 0;
  overflow-wrap: anywhere;
}

.markdown-body :deep(p),
.markdown-body :deep(ul),
.markdown-body :deep(ol),
.markdown-body :deep(pre),
.markdown-body :deep(blockquote),
.markdown-body :deep(table),
.markdown-body :deep(.rendered-code-block) {
  margin: 0 0 8px;
}

.markdown-body :deep(h1),
.markdown-body :deep(h2),
.markdown-body :deep(h3),
.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  margin: 14px 0 8px;
  color: #1a2416;
  font-weight: 750;
  line-height: 1.35;
}
.markdown-body :deep(h1) { font-size: 20px; }
.markdown-body :deep(h2) { font-size: 18px; }
.markdown-body :deep(h3) { font-size: 16px; }
.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) { font-size: 14px; }

.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  box-sizing: border-box;
  display: grid;
  gap: 4px;
  width: 100%;
  max-width: 100%;
  min-width: 0;
  padding-left: 20px;
}
.markdown-body :deep(li) {
  box-sizing: border-box;
  max-width: 100%;
  min-width: 0;
  margin: 0;
}

.markdown-body :deep(blockquote) {
  border-left: 3px solid #b9c7a6;
  color: #55684b;
  padding-left: 10px;
}

.markdown-body :deep(code) {
  border-radius: 4px;
  background: rgba(111, 154, 79, 0.16);
  padding: 1px 4px;
  font-family: "JetBrains Mono", Consolas, monospace;
  font-size: 12px;
}

.markdown-body :deep(pre) {
  box-sizing: border-box;
  min-height: 0;
  min-width: 0;
  width: 100%;
  max-width: 100%;
  max-height: none;
  border-radius: 0 0 6px 6px;
  background: #172033;
  color: #f8fafc;
  padding: 8px;
  overflow: auto;
  margin: 0;
}
.markdown-body :deep(pre code) {
  display: block;
  width: max-content;
  min-width: 100%;
  white-space: pre;
  background: transparent;
  color: inherit;
  padding: 0;
  border-radius: 0;
}

.markdown-body :deep(.rendered-code-block) {
  box-sizing: border-box;
  display: block;
  width: 100%;
  max-width: 100%;
  min-width: 0;
  overflow: hidden;
  border: 1px solid #26324a;
  border-radius: 7px;
  background: #172033;
}
.markdown-body :deep(.code-block-toolbar) {
  box-sizing: border-box;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  min-width: 0;
  max-width: 100%;
  min-height: 34px;
  padding: 0 10px;
  border-bottom: 1px solid #26324a;
  background: #111827;
  color: #cbd5e1;
  font-size: 12px;
}
.markdown-body :deep(.code-block-lang) {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.markdown-body :deep(.code-block-actions) {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex: 0 0 auto;
}
.markdown-body :deep(.code-block-actions button) {
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: #e5e7eb;
  font: inherit;
  line-height: 1;
  padding: 5px 7px;
  cursor: pointer;
}
.markdown-body :deep(.code-block-actions button:hover) {
  background: rgba(148, 163, 184, 0.18);
}

.markdown-body :deep(img),
.markdown-body :deep(video),
.markdown-body :deep(audio),
.markdown-body :deep(iframe) {
  display: block;
  max-width: 100%;
  min-width: 0;
  border-radius: 6px;
}
.markdown-body :deep(img) {
  width: 100%;
  height: auto;
  background: #f8fafc;
  cursor: zoom-in;
}
.markdown-body :deep(video) {
  width: 100%;
  height: auto;
  max-height: 520px;
  object-fit: contain;
  background: #f8fafc;
}
.markdown-body :deep(audio) {
  width: 100%;
}
.markdown-body :deep(iframe) {
  width: 100%;
  height: 400px;
  border: 0;
  background: #ffffff;
}

.markdown-body :deep(table) {
  width: 100%;
  border-collapse: collapse;
  overflow: auto;
}
.markdown-body :deep(th),
.markdown-body :deep(td) {
  border: 1px solid #d8e0ea;
  padding: 6px 8px;
  text-align: left;
  vertical-align: top;
}
.markdown-body :deep(th) {
  background: rgba(111, 154, 79, 0.1);
}
</style>
