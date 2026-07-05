// Agent 消息 Markdown 渲染：marked + DOMPurify。
//
// 从 web/src/components/chat/ChatMessageItem.vue 的实现移植而来，抽成可复用模块：
// - renderMarkdownText：把 markdown 文本渲染成安全 HTML（含代码块工具栏）。
// - parseRenderedParts：拆分出 html 代码块（交给 HtmlPreviewBlock 预览）与普通 markdown 段。
// - handleMarkdownClick：处理代码块复制/下载按钮与图片点击预览。

import DOMPurify from 'dompurify'
import { marked } from 'marked'
import { ElMessage } from 'element-plus'

marked.setOptions({ async: false, breaks: true, gfm: true })

export interface RenderedPart {
  type: 'markdown' | 'html'
  content: string
}

interface FenceLine {
  marker: string
  length: number
  info: string
  closing: boolean
}

function htmlSanitizeOptions() {
  return {
    ADD_TAGS: ['audio', 'video', 'source', 'figure', 'figcaption', 'iframe', 'button', 'textarea'],
    ADD_ATTR: [
      'controls', 'src', 'srcdoc', 'type', 'alt', 'title', 'sandbox', 'scrolling',
      'loading', 'referrerpolicy', 'allow', 'class', 'style', 'hidden', 'readonly',
      'data-code-action', 'data-code-lang',
    ],
  }
}

function escapeHtml(value: string) {
  return String(value || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function escapeHtmlAttr(value: string) {
  return String(value || '')
    .replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

export function codeLanguage(lang: string) {
  return String(lang || '').trim().split(/\s+/)[0] || 'text'
}

function codeBlockToolbar(lang: string) {
  const language = codeLanguage(lang)
  return `<div class="code-block-toolbar">
    <span class="code-block-lang">${escapeHtml(language)}</span>
    <span class="code-block-actions">
      <button type="button" data-code-action="copy">复制</button>
      <button type="button" data-code-action="download">下载</button>
    </span>
  </div>`
}

function codeBlockHtml(text: string, lang: string) {
  const language = codeLanguage(lang)
  const className = language ? ` class="language-${escapeHtmlAttr(language)}"` : ''
  return `<div class="rendered-code-block" data-code-lang="${escapeHtmlAttr(language)}">
    ${codeBlockToolbar(language)}
    <pre><code${className}>${escapeHtml(text)}</code></pre>
  </div>`
}

function createMarkdownRenderer() {
  const renderer = new marked.Renderer()
  renderer.code = (code: string, infostring: string | undefined) => codeBlockHtml(code, infostring || '')
  return renderer
}

function markdownFenceLine(line: string): FenceLine | null {
  const match = String(line || '').match(/^[ \t]{0,3}([`~]{3,})(?:[ \t]*([^ \t`~]+))?[ \t]*$/)
  if (!match) return null
  const fence = match[1]
  const info = String(match[2] || '').trim()
  return { marker: fence[0], length: fence.length, info, closing: info.length === 0 }
}

function fenceMatchesOpening(opening: FenceLine, fence: FenceLine | null) {
  return !!fence && fence.marker === opening.marker && fence.length === opening.length
}

function outerFencedRange(lines: string[]) {
  let first = 0
  while (first < lines.length && !lines[first].trim()) first += 1
  let last = lines.length - 1
  while (last > first && !lines[last].trim()) last -= 1
  if (first >= last) return null
  const opening = markdownFenceLine(lines[first])
  const closing = markdownFenceLine(lines[last])
  if (!opening || !closing?.closing || !fenceMatchesOpening(opening, closing)) return null
  return { first, last, opening }
}

function findFenceClose(lines: string[], startIndex: number) {
  const opening = markdownFenceLine(lines[startIndex])
  if (!opening) return -1
  let nestedOpenCount = 0
  let closeCount = 0
  let lastClose = -1
  for (let index = startIndex + 1; index < lines.length; index += 1) {
    const fence = markdownFenceLine(lines[index])
    if (!fenceMatchesOpening(opening, fence)) continue
    if (fence!.closing) {
      closeCount += 1
      lastClose = index
      if (closeCount > nestedOpenCount) return index
    } else {
      nestedOpenCount += 1
    }
  }
  return lastClose
}

function renderPlainMarkdown(text: string) {
  return marked.parse(String(text || ''), { renderer: createMarkdownRenderer() }) as string
}

export function renderMarkdownText(text: string) {
  const source = String(text || '').replace(/\r\n?/g, '\n')
  const lines = source.split('\n')
  const outer = outerFencedRange(lines)
  if (outer) {
    return DOMPurify.sanitize(
      codeBlockHtml(lines.slice(outer.first + 1, outer.last).join('\n'), outer.opening.info || 'text'),
      htmlSanitizeOptions(),
    )
  }
  let html = ''
  let buffer: string[] = []
  const flush = () => {
    if (!buffer.length) return
    html += renderPlainMarkdown(buffer.join('\n'))
    buffer = []
  }
  for (let index = 0; index < lines.length; index += 1) {
    const fence = markdownFenceLine(lines[index])
    if (!fence) {
      buffer.push(lines[index])
      continue
    }
    const closeIndex = findFenceClose(lines, index)
    if (closeIndex < 0) {
      buffer.push(lines[index])
      continue
    }
    flush()
    html += codeBlockHtml(lines.slice(index + 1, closeIndex).join('\n'), fence.info || 'text')
    index = closeIndex
  }
  flush()
  return DOMPurify.sanitize(html, htmlSanitizeOptions())
}

export function parseRenderedParts(text: string): RenderedPart[] {
  const source = String(text || '').replace(/\r\n?/g, '\n')
  const lines = source.split('\n')
  const outer = outerFencedRange(lines)
  if (outer) {
    if (codeLanguage(outer.opening.info) === 'html') {
      return [{ type: 'html', content: lines.slice(outer.first + 1, outer.last).join('\n').trim() }]
    }
    return [{ type: 'markdown', content: source }]
  }
  const parts: RenderedPart[] = []
  let buffer: string[] = []
  const flush = () => {
    if (!buffer.length) return
    parts.push({ type: 'markdown', content: buffer.join('\n') })
    buffer = []
  }
  for (let index = 0; index < lines.length; index += 1) {
    const fence = markdownFenceLine(lines[index])
    if (!fence) {
      buffer.push(lines[index])
      continue
    }
    const closeIndex = findFenceClose(lines, index)
    if (closeIndex < 0) {
      buffer.push(lines[index])
      continue
    }
    if (codeLanguage(fence.info) === 'html') {
      flush()
      parts.push({ type: 'html', content: lines.slice(index + 1, closeIndex).join('\n').trim() })
    } else {
      buffer.push(...lines.slice(index, closeIndex + 1))
    }
    index = closeIndex
  }
  flush()
  return parts.length ? parts : [{ type: 'markdown', content: source }]
}

function codeFileExtension(lang: string) {
  const language = codeLanguage(lang).toLowerCase()
  const extensions: Record<string, string> = {
    html: 'html', css: 'css', javascript: 'js', js: 'js', typescript: 'ts', ts: 'ts',
    json: 'json', rust: 'rs', rs: 'rs', python: 'py', py: 'py', shell: 'sh', bash: 'sh',
    sh: 'sh', markdown: 'md', md: 'md', vue: 'vue', sql: 'sql', yaml: 'yaml', yml: 'yml',
  }
  return extensions[language] || 'txt'
}

function codeBlockSource(block: Element) {
  return block.querySelector('code')?.textContent || ''
}

async function copyText(value: string) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(value)
    return
  }
  const textarea = document.createElement('textarea')
  textarea.value = value
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  const copied = document.execCommand('copy')
  textarea.remove()
  if (!copied) throw new Error('浏览器拒绝复制操作')
}

function downloadText(value: string, filename: string) {
  const blob = new Blob([value], { type: 'text/plain;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = filename
  document.body.appendChild(link)
  link.click()
  link.remove()
  URL.revokeObjectURL(url)
}

/** 处理渲染区内的点击：代码块复制/下载按钮、图片点击预览。 */
export function handleMarkdownClick(event: MouseEvent, onPreviewImage?: (url: string) => void) {
  const target = event.target as HTMLElement | null
  const action = target?.closest?.('[data-code-action]') as HTMLElement | null
  if (action) {
    const block = action.closest('.rendered-code-block')
    if (!block) return
    event.preventDefault()
    const source = codeBlockSource(block)
    if (!source) return
    const lang = (block as HTMLElement).dataset.codeLang || 'text'
    if (action.dataset.codeAction === 'copy') {
      copyText(source)
        .then(() => ElMessage.success('复制成功'))
        .catch((err) => ElMessage.error(`复制失败：${err?.message || String(err)}`))
      return
    }
    if (action.dataset.codeAction === 'download') {
      downloadText(source, `code.${codeFileExtension(lang)}`)
      return
    }
  }

  const image = target?.closest?.('img') as HTMLImageElement | null
  if (!image) return
  const src = image.getAttribute('src')
  if (!src) return
  event.preventDefault()
  onPreviewImage?.(src)
}
