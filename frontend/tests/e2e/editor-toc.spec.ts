import { expect, test, type Page, type Route } from '@playwright/test'

const now = '2026-03-20T08:00:00Z'
const firstSectionBody = Array.from({ length: 18 }, (_, index) => `第一章段落 ${index + 1}：用于拉长预览滚动区域。`).join('\n\n')
const secondSectionBody = Array.from({ length: 16 }, (_, index) => `第二章段落 ${index + 1}：用于验证锚点跳转后的停靠位置。`).join('\n\n')

const initialMarkdown = `# 测试文档

## 第一章

${firstSectionBody}

## 第二章

${secondSectionBody}
`

const updatedMarkdown = `# 测试文档

## 新章节

这里是新章节内容。
`

const currentUser = {
  id: 1,
  username: 'e2e',
  avatar: '',
  totp_enabled: false,
  is_super_admin: true,
  is_active: true,
}

async function mockHomeApis(route: Route) {
  const request = route.request()
  const url = new URL(request.url())

  if (url.pathname === '/api/auth/public-settings') {
    await route.fulfill({
      json: {
        settings: {
          registration_enabled: true,
          upload_max_bytes: 20 * 1024 * 1024,
          upload_max_mb: 20,
          updated_at: now,
        },
      },
    })
    return
  }

  if (url.pathname === '/api/auth/me') {
    await route.fulfill({
      json: {
        user: currentUser,
      },
    })
    return
  }

  if (url.pathname === '/api/projects') {
    await route.fulfill({
      json: {
        projects: [
          {
            id: 1,
            name: '目录测试项目',
            description: 'editor toc e2e',
            background_image: null,
            sort_order: 0,
            created_at: now,
            updated_at: now,
          },
        ],
      },
    })
    return
  }

  if (url.pathname === '/api/docs' && request.method() === 'GET') {
    await route.fulfill({
      json: {
        tree: [
          {
            id: 101,
            project_id: 1,
            parent_id: null,
            name: '测试文档',
            node_type: 'doc',
            sort_order: 0,
            created_at: now,
            updated_at: now,
            children: [],
          },
        ],
      },
    })
    return
  }

  if (url.pathname === '/api/docs/101' && request.method() === 'GET') {
    await route.fulfill({
      json: {
        node: {
          id: 101,
          project_id: 1,
          parent_id: null,
          name: '测试文档',
          node_type: 'doc',
          content: initialMarkdown,
          sort_order: 0,
          created_at: now,
          updated_at: now,
          children: [],
        },
      },
    })
    return
  }

  await route.abort()
}

async function bootstrapEditor(page: Page) {
  await page.route('**/api/**', mockHomeApis)
  await page.addInitScript((user) => {
    localStorage.setItem('token', 'e2e-token')
    localStorage.setItem('user', JSON.stringify(user))
  }, currentUser)
  await page.goto('/?project=1&doc=101')
  await expect(page.locator('.sy-doc-title')).toHaveText('测试文档')
  await page.waitForFunction(() => Boolean((window as typeof window & { editor?: { setValue?: (value: string) => void } }).editor?.setValue))
}

async function getHeadingOffsetInPreview(page: Page, text: string) {
  return await page.evaluate((headingText) => {
    const container = document.querySelector('.sy-preview-body')
    const headings = Array.from(
      document.querySelectorAll<HTMLElement>('.sy-preview-body .vditor-reset h1, .sy-preview-body .vditor-reset h2, .sy-preview-body .vditor-reset h3')
    )
    const target = headings.find((item) => item.textContent?.trim() === headingText)
    if (!(container instanceof HTMLElement) || !target) return null
    return target.getBoundingClientRect().top - container.getBoundingClientRect().top
  }, text)
}

test('shows and updates the editor toc from the rendered preview', async ({ page }) => {
  await bootstrapEditor(page)

  await expect(page.getByText('文档目录')).toBeVisible()
  await expect(page.getByRole('button', { name: '第一章' })).toBeVisible()
  await expect(page.getByRole('button', { name: '第二章' })).toBeVisible()

  await page.getByRole('button', { name: '第二章' }).click()
  await expect.poll(async () => {
    const offset = await getHeadingOffsetInPreview(page, '第二章')
    return typeof offset === 'number' ? Math.round(offset) : null
  }).toBeLessThanOrEqual(140)

  await page.evaluate((markdown) => {
    ;(window as typeof window & { editor?: { setValue: (value: string) => void } }).editor?.setValue(markdown)
  }, updatedMarkdown)

  await expect(page.getByRole('button', { name: '新章节' })).toBeVisible()
  await expect(page.getByRole('button', { name: '第二章' })).toHaveCount(0)
})

test('does not flash the toc back to loading after the preview has already been established', async ({ page }) => {
  await bootstrapEditor(page)
  await expect(page.getByRole('button', { name: '第一章' })).toBeVisible()

  await page.evaluate(() => {
    const toc = document.querySelector('[data-testid="editor-toc"]')
    ;(window as typeof window & {
      __tocLoadingHits?: number
      __tocObserver?: MutationObserver
    }).__tocLoadingHits = 0
    if (!toc) return

    const targetWindow = window as typeof window & {
      __tocLoadingHits?: number
      __tocObserver?: MutationObserver
    }

    const observer = new MutationObserver(() => {
      const loading = toc.querySelector('.sy-editor-toc-empty')
      if (loading?.textContent?.includes('正在识别目录')) {
        targetWindow.__tocLoadingHits = (targetWindow.__tocLoadingHits || 0) + 1
      }
    })

    observer.observe(toc, {
      childList: true,
      subtree: true,
      characterData: true,
    })

    targetWindow.__tocObserver = observer
  })

  await page.waitForTimeout(300)
  await page.evaluate(() => {
    ;(window as typeof window & { __tocLoadingHits?: number }).__tocLoadingHits = 0
  })

  await page.evaluate((markdown) => {
    ;(window as typeof window & { editor?: { setValue: (value: string) => void } }).editor?.setValue(markdown)
  }, updatedMarkdown)

  await expect(page.getByRole('button', { name: '新章节' })).toBeVisible()
  await expect.poll(async () => {
    return await page.evaluate(() => {
      return (window as typeof window & { __tocLoadingHits?: number }).__tocLoadingHits || 0
    })
  }).toBe(0)

  await page.evaluate(() => {
    ;(window as typeof window & { __tocObserver?: MutationObserver }).__tocObserver?.disconnect()
  })
})

test('keeps preview content width unchanged when the toc is visible', async ({ page }) => {
  await bootstrapEditor(page)
  await expect(page.getByText('文档目录')).toBeVisible()

  const paddingRight = await page.evaluate(() => {
    const previewBody = document.querySelector('.sy-preview-body')
    if (!(previewBody instanceof HTMLElement)) return null
    return window.getComputedStyle(previewBody).paddingRight
  })

  expect(paddingRight).toBe('24px')
})

test('uses a wider expanded toc panel close to the share view size', async ({ page }) => {
  await bootstrapEditor(page)
  await expect(page.getByText('文档目录')).toBeVisible()

  const width = await page.evaluate(() => {
    const toc = document.querySelector('[data-testid="editor-toc"]')
    if (!(toc instanceof HTMLElement)) return null
    return window.getComputedStyle(toc).width
  })

  expect(width).toBe('270px')
})
