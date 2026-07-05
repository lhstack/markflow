<script setup>
import { computed, ref } from 'vue'
import { ElMessage } from 'element-plus'

const props = defineProps({
  source: { type: String, required: true }
})

const showPreview = ref(true)

const previewSource = computed(() => {
  const source = props.source.trim()
  const previewStyle = `<style>
html,
body {
  width: 100% !important;
  height: 100% !important;
  margin: 0 !important;
  overflow: auto;
}
* {
  box-sizing: border-box;
}
</style>`
  if (/<\/head>/i.test(source)) return source.replace(/<\/head>/i, `${previewStyle}</head>`)
  if (/<html\b/i.test(source)) return source.replace(/<html\b([^>]*)>/i, `<html$1><head>${previewStyle}</head>`)
  return `${previewStyle}${source}`
})

async function copySource() {
  try {
    await navigator.clipboard.writeText(props.source)
    ElMessage.success('复制成功')
  } catch (err) {
    ElMessage.error(`复制失败：${err?.message || String(err)}`)
  }
}

function downloadSource() {
  const blob = new Blob([props.source], { type: 'text/html;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = 'code.html'
  document.body.appendChild(link)
  link.click()
  link.remove()
  URL.revokeObjectURL(url)
}
</script>

<template>
  <div class="html-preview-block">
    <div class="html-preview-toolbar">
      <span class="html-preview-lang">html</span>
      <span class="html-preview-actions">
        <button type="button" @click="copySource">复制</button>
        <button type="button" @click="downloadSource">下载</button>
        <button type="button" @click="showPreview = !showPreview">{{ showPreview ? '源码' : '预览' }}</button>
      </span>
    </div>
    <iframe
      v-show="showPreview"
      class="html-preview-frame"
      title="HTML preview"
      sandbox="allow-scripts allow-forms allow-popups allow-downloads"
      loading="lazy"
      referrerpolicy="no-referrer"
      scrolling="no"
      :srcdoc="previewSource"
    />
    <pre v-show="!showPreview" class="html-preview-source"><code>{{ source }}</code></pre>
  </div>
</template>

<style scoped>
.html-preview-block {
  overflow: hidden;
  margin: 0 0 8px;
  border: 1px solid #d8e0ea;
  border-radius: 7px;
  background: #ffffff;
}

.html-preview-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  min-height: 34px;
  padding: 0 10px;
  border-bottom: 1px solid #26324a;
  background: #111827;
  color: #cbd5e1;
  font-size: 12px;
}

.html-preview-lang {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.html-preview-actions {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex: 0 0 auto;
}

.html-preview-actions button {
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: #e5e7eb;
  font: inherit;
  line-height: 1;
  padding: 5px 7px;
  cursor: pointer;
}

.html-preview-actions button:hover {
  background: rgba(148, 163, 184, 0.18);
}

.html-preview-frame {
  width: 100%;
  min-height: 451px;
}
.html-preview-source {
  width: 100%;
}

.html-preview-frame {
  display: block;
  border: 0;
  background: #ffffff;
}

.html-preview-source {
  margin: 0;
  overflow: auto;
  border-radius: 0;
  background: #172033;
  color: #f8fafc;
  padding: 8px;
}

.html-preview-source code {
  display: block;
  min-width: max-content;
  white-space: pre;
  font-family: "JetBrains Mono", Consolas, monospace;
  font-size: 12px;
}
</style>
