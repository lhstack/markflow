<template>
  <div class="vditor-preview-shell">
    <div ref="previewRef" class="vditor-preview-host"></div>
    <pre v-if="renderError" class="vditor-preview-fallback">{{ markdown }}</pre>
  </div>
</template>

<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import Vditor from 'vditor'
import 'vditor/dist/index.css'
import { VDITOR_CDN } from '@/utils/vditor'

const props = defineProps<{
  markdown: string
  renderKey?: string | number
  clearBeforeRender?: boolean
}>()

interface PreviewHeading {
  text: string
  level: number
}

const emit = defineEmits<{
  rendered: [{ key?: string | number; headings: PreviewHeading[] }]
}>()

const previewRef = ref<HTMLDivElement | null>(null)
const renderError = ref(false)
let renderTimer: number | null = null

function handlePreviewLinkClick(event: MouseEvent) {
  const host = previewRef.value
  const target = event.target
  if (!host || !(target instanceof HTMLElement)) return

  const link = target.closest('a') as HTMLAnchorElement | null
  if (!link || !host.contains(link)) return

  const href = link.getAttribute('href')?.trim()
  if (!href) return

  if (href.startsWith('#')) {
    event.preventDefault()
    const targetId = decodeURIComponent(href.slice(1))
    const heading = targetId ? host.querySelector<HTMLElement>(`#${CSS.escape(targetId)}`) : null
    heading?.scrollIntoView({ block: 'start', behavior: 'smooth' })
    return
  }

  event.preventDefault()
  window.open(link.href || href, '_blank', 'noopener,noreferrer')
}

function normalizePreviewLinks() {
  if (!previewRef.value) return

  previewRef.value.querySelectorAll<HTMLAnchorElement>('a[href]').forEach((link) => {
    const href = link.getAttribute('href')?.trim()
    if (!href || href.startsWith('#')) return
    link.target = '_blank'
    link.rel = 'noopener noreferrer'
  })
}

async function renderPreview() {
  await nextTick()
  if (!previewRef.value) return
  if (props.clearBeforeRender) {
    previewRef.value.innerHTML = ''
  }
  renderError.value = false

  try {
    Vditor.preview(previewRef.value, props.markdown || '', {
      cdn: VDITOR_CDN,
      mode: 'light',
      lang: 'zh_CN',
      icon: 'material',
      theme: {
        current: 'light',
      },
      markdown: {
        toc: true,
        mark: true,
        footnotes: true,
        autoSpace: true,
        codeBlockPreview: true,
        mathBlockPreview: true,
      },
      hljs: {
        style: 'github',
        lineNumber: false,
      },
    })

    normalizePreviewLinks()

    const headings = Array.from(
      previewRef.value.querySelectorAll<HTMLElement>('.vditor-reset h1, .vditor-reset h2, .vditor-reset h3, .vditor-reset h4, .vditor-reset h5, .vditor-reset h6')
    ).map((heading) => ({
      text: heading.textContent?.trim() || '',
      level: Number(heading.tagName.slice(1)) || 1,
    }))

    emit('rendered', {
      key: props.renderKey,
      headings,
    })
  } catch (error) {
    console.error('Vditor preview render failed', error)
    renderError.value = true
    previewRef.value.innerHTML = ''
    emit('rendered', {
      key: props.renderKey,
      headings: [],
    })
  }
}

function queueRenderPreview() {
  if (renderTimer) {
    window.clearTimeout(renderTimer)
  }
  renderTimer = window.setTimeout(() => {
    renderTimer = null
    void renderPreview()
  }, 80)
}

watch(() => props.markdown, () => {
  queueRenderPreview()
}, { immediate: true })

onMounted(() => {
  previewRef.value?.addEventListener('click', handlePreviewLinkClick)
  queueRenderPreview()
})

onBeforeUnmount(() => {
  if (renderTimer) {
    window.clearTimeout(renderTimer)
  }
  previewRef.value?.removeEventListener('click', handlePreviewLinkClick)
})
</script>

<style scoped>
.vditor-preview-shell {
  width: 100%;
}

.vditor-preview-host {
  min-height: 120px;
}

.vditor-preview-fallback {
  margin: 0;
  padding: 16px 18px;
  border-radius: 16px;
  border: 1px solid rgba(15, 23, 42, 0.08);
  background: #f8fbf5;
  color: #334155;
  font-size: 13px;
  line-height: 1.65;
  white-space: pre-wrap;
  word-break: break-word;
}

:deep(.vditor-reset) {
  font-family: var(--font);
  color: #23262f;
  line-height: 1.8;
  font-size: 15px;
}

:deep(.vditor-reset h1),
:deep(.vditor-reset h2),
:deep(.vditor-reset h3),
:deep(.vditor-reset h4),
:deep(.vditor-reset h5),
:deep(.vditor-reset h6) {
  color: #13161d;
  letter-spacing: -0.02em;
  scroll-margin-top: 28px;
}

:deep(.vditor-reset h1) {
  font-size: 2rem;
  border-bottom: 0;
}

:deep(.vditor-reset h2) {
  margin-top: 2.2rem;
  padding-bottom: 0.3rem;
  border-bottom: 1px solid rgba(30, 41, 59, 0.08);
}

:deep(.vditor-reset p),
:deep(.vditor-reset li) {
  color: #3b4252;
}

:deep(.vditor-reset blockquote) {
  margin: 1.2rem 0;
  padding: 0.9rem 1rem;
  border-left: 4px solid #78a55a;
  background: linear-gradient(135deg, rgba(120, 165, 90, 0.08), rgba(241, 245, 233, 0.9));
  color: #44503a;
  border-radius: 0 14px 14px 0;
}

:deep(.vditor-reset code:not(.hljs)) {
  border-radius: 6px;
  padding: 0.18rem 0.38rem;
  background: #f0f4e8;
  color: #486032;
  font-family: var(--mono);
}

:deep(.vditor-reset pre) {
  padding: 0 !important;
  overflow: hidden;
  border-radius: 18px;
  border: 1px solid rgba(22, 34, 21, 0.08);
  background: #f8fbf5;
  box-shadow: 0 18px 40px rgba(44, 58, 35, 0.08);
}

:deep(.vditor-reset pre code) {
  display: block;
  padding: 1rem 1.1rem !important;
  font-family: var(--mono);
}

:deep(.vditor-reset table) {
  border-collapse: separate;
  border-spacing: 0;
  overflow: hidden;
  border-radius: 14px;
  border: 1px solid rgba(15, 23, 42, 0.08);
}

:deep(.vditor-reset th) {
  background: #f3f7ee;
}

:deep(.vditor-reset img) {
  border-radius: 16px;
  box-shadow: 0 18px 36px rgba(36, 47, 28, 0.12);
}

:deep(.vditor-reset a) {
  color: #3e7a2d;
}
</style>
