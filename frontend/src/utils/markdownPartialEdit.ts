export type DocumentEditMutationMode = 'rewrite_document_section' | 'replace_document_block'

export interface DocumentEditMutationResult {
  ok: boolean
  value: string
  reason?: string
  target?: string | null
  replacementPreview?: string
}

interface MarkdownSectionRange {
  start: number
  end: number
  level: number
  heading: string
}

function extractTaggedContent(source: string, tag: string) {
  const pattern = new RegExp(`\\[\\[${tag}\\]\\]([\\s\\S]*?)\\[\\[\\/${tag}\\]\\]`, 'i')
  const match = source.match(pattern)
  return match?.[1]?.trim() || ''
}

function findExactMatchOffsets(source: string, target: string) {
  if (!target) return []
  const offsets: number[] = []
  let searchFrom = 0
  while (searchFrom <= source.length) {
    const index = source.indexOf(target, searchFrom)
    if (index === -1) break
    offsets.push(index)
    searchFrom = index + target.length
  }
  return offsets
}

function normalizeHeadingText(value: string) {
  return value.replace(/^#{1,6}\s*/, '').trim()
}

function findSectionRange(lines: string[], targetHeading: string): MarkdownSectionRange | null {
  const target = normalizeHeadingText(targetHeading)
  if (!target) return null

  const headingPattern = /^(#{1,6})\s+(.+)$/
  let start = -1
  let level = 0

  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(headingPattern)
    if (!match) continue
    const headingText = normalizeHeadingText(match[2] || '')
    if (headingText === target) {
      start = index
      level = (match[1] || '').length
      break
    }
  }

  if (start === -1) return null

  let end = lines.length
  for (let index = start + 1; index < lines.length; index += 1) {
    const match = lines[index].match(headingPattern)
    if (!match) continue
    const nextLevel = (match[1] || '').length
    if (nextLevel <= level) {
      end = index
      break
    }
  }

  return {
    start,
    end,
    level,
    heading: target,
  }
}

export function applySectionRewritePayload(original: string, payload: string): DocumentEditMutationResult {
  const targetRaw = extractTaggedContent(payload, 'TARGET')
  const contentRaw = extractTaggedContent(payload, 'CONTENT')
  const target = normalizeHeadingText(targetRaw)
  const sectionContent = (contentRaw || payload).trim()
  if (!target || !sectionContent) {
    return {
      ok: false,
      value: original,
      reason: 'rewrite_document_section 缺少 [[TARGET]] 或 [[CONTENT]]，无法定位目标章节并应用替换。',
      target: target || null,
      replacementPreview: sectionContent || payload.trim(),
    }
  }

  const lines = original.split(/\r?\n/)
  const range = findSectionRange(lines, target)
  if (!range) {
    return {
      ok: false,
      value: original,
      reason: `未找到标题为“${target}”的章节，rewrite_document_section 未应用。`,
      target,
      replacementPreview: sectionContent,
    }
  }

  const replacementLines = sectionContent.split(/\r?\n/)
  return {
    ok: true,
    value: [...lines.slice(0, range.start), ...replacementLines, ...lines.slice(range.end)]
      .join('\n')
      .replace(/\n{3,}/g, '\n\n'),
    target,
    replacementPreview: sectionContent,
  }
}

export function applyBlockReplacePayload(original: string, payload: string): DocumentEditMutationResult {
  const find = extractTaggedContent(payload, 'FIND')
  const replacement = extractTaggedContent(payload, 'REPLACE')
  if (!find) {
    return {
      ok: false,
      value: original,
      reason: 'replace_document_block 缺少 [[FIND]]，无法定位要替换的原文片段。',
      replacementPreview: replacement || payload.trim(),
    }
  }
  const matches = findExactMatchOffsets(original, find)
  if (!matches.length) {
    return {
      ok: false,
      value: original,
      reason: 'replace_document_block 未在当前文档中找到完全匹配的 [[FIND]] 片段，局部替换未应用。',
      target: find,
      replacementPreview: replacement,
    }
  }
  if (matches.length > 1) {
    return {
      ok: false,
      value: original,
      reason: `replace_document_block 命中了 ${matches.length} 处完全相同的片段。为避免误替换，请提供更精确的 [[FIND]] 上下文。`,
      target: find,
      replacementPreview: replacement,
    }
  }
  const index = matches[0]
  return {
    ok: true,
    value: `${original.slice(0, index)}${replacement}${original.slice(index + find.length)}`,
    target: find,
    replacementPreview: replacement,
  }
}

export function applySectionSwap(
  original: string,
  firstHeading: string,
  secondHeading: string,
): DocumentEditMutationResult {
  const first = normalizeHeadingText(firstHeading)
  const second = normalizeHeadingText(secondHeading)
  if (!first || !second) {
    return {
      ok: false,
      value: original,
      reason: 'swap_document_sections 需要 first_heading 和 second_heading 参数。',
      target: `${first || ''}|${second || ''}`,
    }
  }
  if (first === second) {
    return {
      ok: false,
      value: original,
      reason: 'swap_document_sections 需要两个不同的章节标题。',
      target: first,
    }
  }

  const lines = original.split(/\r?\n/)
  const firstRange = findSectionRange(lines, first)
  if (!firstRange) {
    return {
      ok: false,
      value: original,
      reason: `未找到标题为“${first}”的章节，swap_document_sections 未应用。`,
      target: first,
    }
  }
  const secondRange = findSectionRange(lines, second)
  if (!secondRange) {
    return {
      ok: false,
      value: original,
      reason: `未找到标题为“${second}”的章节，swap_document_sections 未应用。`,
      target: second,
    }
  }

  const [earlier, later] = firstRange.start < secondRange.start
    ? [firstRange, secondRange]
    : [secondRange, firstRange]

  if (earlier.end > later.start) {
    return {
      ok: false,
      value: original,
      reason: `swap_document_sections 检测到“${first}”与“${second}”的章节范围重叠（通常是父子章节关系），为避免破坏文档结构，已拒绝执行。`,
      target: `${first} <-> ${second}`,
    }
  }

  const swappedLines = [
    ...lines.slice(0, earlier.start),
    ...lines.slice(later.start, later.end),
    ...lines.slice(earlier.end, later.start),
    ...lines.slice(earlier.start, earlier.end),
    ...lines.slice(later.end),
  ]

  return {
    ok: true,
    value: swappedLines.join('\n').replace(/\n{3,}/g, '\n\n'),
    target: `${first} <-> ${second}`,
    replacementPreview: `${second} / ${first}`,
  }
}

export function applyDocumentEditMutation(
  mode: DocumentEditMutationMode,
  original: string,
  payload: string,
): DocumentEditMutationResult {
  if (mode === 'rewrite_document_section') {
    return applySectionRewritePayload(original, payload)
  }
  return applyBlockReplacePayload(original, payload)
}
