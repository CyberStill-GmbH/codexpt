import { expect, test } from '@playwright/test'
import AxeBuilder from '@axe-core/playwright'

async function createReview(page: import('@playwright/test').Page, title: string) {
  await page.goto('/')
  await page.getByRole('button', { name: 'Crear revisión' }).first().click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByLabel('Título')).toBeFocused()
  await dialog.getByLabel('Título').fill(title)
  await dialog.getByLabel(/Contexto/).fill('Recorrido E2E sintético con SQLite real.')
  await dialog.getByRole('button', { name: 'Crear revisión' }).click()
  await expect(page.getByRole('heading', { name: title })).toBeVisible()
}

async function expectAccessibleCapture(page: import('@playwright/test').Page, path: string) {
  const accessibility = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
  expect(accessibility.violations).toEqual([])
  await page.screenshot({ path: test.info().outputPath(path), fullPage: true })
}

test('login and invite screens: responsive empty identity states are accessible', async ({ page }) => {
  const pageErrors: string[] = []
  page.on('pageerror', (error) => pageErrors.push(error.message))

  for (const [width, height, suffix] of [[390, 844, 'mobile'], [768, 1024, 'tablet'], [1280, 900, 'desktop']] as const) {
    await page.setViewportSize({ width, height })
    await page.goto('/login')
    await expect(page.getByRole('heading', { name: 'Iniciar sesión' })).toBeVisible()
    await expect(page.locator('.login-button')).toHaveAttribute('aria-disabled', 'true')
    await expect(page.locator('.login-button')).toBeDisabled()
    await expect(page.getByText('La conexión de identidad estará disponible cuando termine la configuración del entorno.')).toBeVisible()
    const illustration = page.locator('.login-art-image')
    if (width <= 960) {
      await expect(illustration).toBeHidden()
    } else {
      await expect(illustration).toBeVisible()
      await expect.poll(() => illustration.evaluate((image: HTMLImageElement) => image.naturalWidth)).toBeGreaterThan(0)
    }
    await expectAccessibleCapture(page, `login-${suffix}.png`)

    await page.goto('/invite')
    await expect(page.getByRole('heading', { name: /Entra a un espacio de trabajo compartido/ })).toBeVisible()
    await expect(page.getByText('La conexión de identidad aún no está configurada.')).toBeVisible()
    await expect(page.getByRole('link', { name: 'Volver al inicio seguro' })).toHaveAttribute('href', '/login')
    await expectAccessibleCapture(page, `invite-${suffix}.png`)
  }

  expect(pageErrors).toEqual([])
}, 90_000)

test('configured OpenAI OIDC shows ChatGPT sign-in and keeps CodeXPT roles explicit', async ({ page }) => {
  await page.route('**/api/auth/status', (route) => route.fulfill({ json: { configured: true, provider: 'ChatGPT' } }))
  await page.route('**/api/auth/me', (route) => route.fulfill({ status: 401, json: { error: 'unauthorized' } }))
  await page.goto('/login')
  const button = page.getByRole('link', { name: 'Continuar con ChatGPT' })
  await expect(button).toBeVisible()
  await expect(button).toHaveAttribute('href', /\/api\/auth\/login$/)
  await expect(page.getByText('OpenAI verifica tu identidad; CodeXPT administra tus permisos y workspace.')).toBeVisible()
})

test('startup splash shows a smiling panda wink on white before yielding to the app', async ({ page }) => {
  await page.route('**/api/auth/status', (route) => route.fulfill({ json: { configured: false } }))
  await page.route('**/api/tasks', (route) => route.fulfill({ json: [] }))
  await page.goto('/')

  const splash = page.getByRole('status', { name: 'CodeXPT se está preparando' })
  const panda = splash.getByRole('img', { name: 'Cara sonriente de un panda.' })
  const wink = splash.locator('.panda-wink')
  await expect(splash).toBeVisible()
  await expect(panda).toBeVisible()
  await expect.poll(() => splash.evaluate((element) => getComputedStyle(element).backgroundColor)).toBe('rgb(255, 255, 255)')
  await expect.poll(() => wink.evaluate((element) => getComputedStyle(element).opacity)).toBe('1')
  await expect(splash).toBeHidden()
  await expect(page.getByRole('heading', { name: 'El espacio está listo.' })).toBeVisible()
})

test('post-splash unavailable screen and illustration enter softly', async ({ page }) => {
  await page.route('**/api/auth/status', (route) => route.fulfill({ status: 503, json: { error: 'API proxy is not configured' } }))
  await page.goto('/')

  await expect(page.getByRole('heading', { name: 'Estamos comiendo bambú. Volvemos pronto.' })).toBeVisible()
  const pageContent = page.locator('.service-unavailable-main > section')
  const illustration = page.locator('.service-unavailable-art')
  await expect.poll(() => pageContent.evaluate((element) => getComputedStyle(element).animationName)).toBe('content-arrive')
  await expect.poll(() => illustration.evaluate((element) => getComputedStyle(element).animationName)).toBe('illustration-arrive')
  await expect.poll(() => illustration.locator('img').evaluate((image: HTMLImageElement) => image.naturalWidth)).toBeGreaterThan(0)
  await Promise.all([pageContent, illustration].map((locator) => locator.evaluate((element) => Promise.all(element.getAnimations().map((animation) => animation.finished)))))
  await page.screenshot({ path: test.info().outputPath('backend-unavailable-desktop.png'), fullPage: true })
  await page.setViewportSize({ width: 390, height: 844 })
  await page.screenshot({ path: test.info().outputPath('backend-unavailable-mobile.png'), fullPage: true })
})

test('dashboard: empty workspace state is responsive and accessible', async ({ page }) => {
  const pageErrors: string[] = []
  page.on('pageerror', (error) => pageErrors.push(error.message))

  for (const [width, height, suffix] of [[390, 844, 'mobile'], [1280, 900, 'desktop']] as const) {
    await page.setViewportSize({ width, height })
    await page.goto('/')
    await expect(page.getByRole('heading', { name: 'El espacio está listo.' })).toBeVisible()
    await expect(page.locator('.first-run').getByRole('button', { name: 'Crear revisión' })).toBeVisible()
    await expectAccessibleCapture(page, `workspace-empty-${suffix}.png`)
  }

  expect(pageErrors).toEqual([])
})

test('context checkpoint: reviewed package is clear and usable on mobile, tablet and desktop', async ({ page }) => {
  const pageErrors: string[] = []
  page.on('pageerror', (error) => pageErrors.push(error.message))
  await page.setViewportSize({ width: 390, height: 844 })
  await createReview(page, 'Continuidad de contexto')
  await expect(page.getByText('Capacidad restante: desconocida')).toBeVisible()
  await page.getByRole('button', { name: 'Preparar checkpoint manual' }).click()
  await page.getByLabel('Objetivo actual').fill('Continuar la revisión sin perder decisiones vigentes.')
  await page.getByRole('textbox', { name: 'Decisiones vigentes' }).fill('No se transfiere la transcripción ni secretos.')
  await page.getByLabel('Siguiente acción').fill('Verificar las referencias en el hilo de destino.')
  await page.getByRole('button', { name: 'Revisé y congelar' }).click()
  await expect(page.getByText('Listo para transferir')).toBeVisible()
  await expect(page.locator('.checkpoint-record code')).toContainText('SHA-256')

  for (const [width, height, suffix] of [[390, 844, 'mobile'], [768, 1024, 'tablet'], [1280, 900, 'desktop']] as const) {
    await page.setViewportSize({ width, height })
    await expect(page.getByText('Listo para transferir')).toBeVisible()
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)
    expect(overflow).toBe(false)
    if (width !== 768) {
      const accessibility = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
      expect(accessibility.violations).toEqual([])
    }
    await page.screenshot({ path: test.info().outputPath(`checkpoint-${suffix}.png`), fullPage: true })
  }
  expect(pageErrors).toEqual([])
})

test('hosted owner: member management screen renders and creates a one-time invite', async ({ page }) => {
  const task = {
    task_id: '00000000-0000-4000-8000-000000000001', title: 'Review de muestra', description: 'Datos sintéticos de QA.',
    status: 'open', pending_decision: false, version: 1,
    created_at: '2026-10-10T10:00:00Z', updated_at: '2026-10-10T10:00:00Z',
  }
  const workspaceId = '00000000-0000-4000-8000-000000000002'
  let inviteRequest: { email?: string; role?: string } | null = null
  let roleUpdate: { role?: string } | null = null
  let revokedMember: string | null = null
  const pageErrors: string[] = []
  page.on('pageerror', (error) => pageErrors.push(error.message))

  await page.route('**/api/**', async (route) => {
    const request = route.request()
    const url = new URL(request.url())
    if (url.pathname === '/api/auth/status') return route.fulfill({ json: { configured: true } })
    if (url.pathname === '/api/auth/me') return route.fulfill({ json: { csrf_token: 'synthetic-csrf', email: 'owner@example.test' } })
    if (url.pathname === '/api/workspaces') return route.fulfill({ json: [{ workspace_id: workspaceId, name: 'QA workspace', role: 'OWNER' }] })
    if (url.pathname === '/api/tasks') return route.fulfill({ json: [task] })
    if (url.pathname === `/api/tasks/${task.task_id}`) return route.fulfill({ json: { task, messages: [], events: [], pending_decision: null } })
    if (url.pathname.endsWith('/members') && request.method() === 'GET') return route.fulfill({ json: [{ user_id: 'owner-1', email: 'owner@example.test', role: 'OWNER' }, { user_id: 'editor-1', email: 'editor@example.test', role: 'EDITOR' }] })
    if (url.pathname === `/api/workspaces/${workspaceId}/members/editor-1` && request.method() === 'PATCH') {
      roleUpdate = request.postDataJSON() as { role?: string }
      return route.fulfill({ status: 204, body: '' })
    }
    if (url.pathname === `/api/workspaces/${workspaceId}/members/editor-1` && request.method() === 'DELETE') {
      revokedMember = 'editor-1'
      return route.fulfill({ status: 204, body: '' })
    }
    if (url.pathname === `/api/workspaces/${workspaceId}/invites` && request.method() === 'POST') {
      inviteRequest = request.postDataJSON() as { email?: string; role?: string }
      return route.fulfill({ json: { invite_id: 'synthetic-invite', raw_token: 'synthetic-one-time-token', expires_at: '2026-10-17T10:00:00Z' } })
    }
    return route.fulfill({ status: 404, json: { error: `No synthetic QA route for ${request.method()} ${url.pathname}` } })
  })

  for (const [width, height, suffix] of [[390, 844, 'mobile'], [390, 600, 'mobile-short'], [1280, 900, 'desktop']] as const) {
    await page.setViewportSize({ width, height })
    await page.goto('/')
    await expect(page.getByRole('button', { name: 'Miembros' })).toBeVisible()
    const opener = page.getByRole('button', { name: 'Miembros' })
    await opener.click()
    const dialog = page.getByRole('dialog', { name: 'Miembros' })
    const headerClose = dialog.locator('.dialog-head').getByRole('button', { name: 'Cerrar' })
    await expect(headerClose).toBeFocused()
    await page.keyboard.press('Shift+Tab')
    await expect(dialog.locator('.dialog-actions').getByRole('button', { name: 'Cerrar' })).toBeFocused()
    await page.keyboard.press('Tab')
    await expect(headerClose).toBeFocused()
    if (height === 600) {
      const bounds = await dialog.evaluate((element) => {
        const rect = element.getBoundingClientRect()
        return { top: rect.top, bottom: rect.bottom, height: rect.height, scrollHeight: element.scrollHeight, clientHeight: element.clientHeight }
      })
      expect(bounds.top).toBeGreaterThanOrEqual(0)
      expect(bounds.bottom).toBeLessThanOrEqual(height)
      expect(bounds.height).toBeLessThanOrEqual(height - 24)
      expect(bounds.scrollHeight).toBeGreaterThanOrEqual(bounds.clientHeight)
      expect(await dialog.getByLabel('Invitar por correo verificado').evaluate((input) => getComputedStyle(input).minHeight)).toBe('44px')
    }
    await page.keyboard.press('Escape')
    await expect(dialog).toBeHidden()
    await expect(opener).toBeFocused()
    await opener.click()
    await expect(dialog).toBeVisible()
    await expect(dialog.getByText('owner@example.test')).toBeVisible()
    await expect(dialog.getByText('editor@example.test', { exact: true })).toBeVisible()
    const evidence = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
    expect(evidence.violations).toEqual([])
    await page.screenshot({ path: test.info().outputPath(`members-${suffix}.png`), fullPage: true })
    await dialog.getByLabel('Invitar por correo verificado').fill('new-member@example.test')
    await dialog.getByLabel('Rol inicial').selectOption('VIEWER')
    await dialog.getByRole('button', { name: 'Crear invitación' }).click()
    await expect(dialog.getByText('synthetic-one-time-token')).toBeVisible()
    expect(inviteRequest).toEqual({ email: 'new-member@example.test', role: 'VIEWER' })
    if (width === 1280) {
      await dialog.getByLabel('Rol de editor@example.test').selectOption('VIEWER')
      await expect(dialog.locator('.member-list').getByText('Viewer · solo lectura')).toBeVisible()
      expect(roleUpdate).toEqual({ role: 'VIEWER' })
      await dialog.getByRole('button', { name: 'Revocar acceso a editor@example.test' }).click()
      await dialog.getByRole('button', { name: 'Confirmar' }).click()
      await expect(dialog.getByText('editor@example.test', { exact: true })).toHaveCount(0)
      expect(revokedMember).toBe('editor-1')
    }
    await dialog.locator('.dialog-head').getByRole('button', { name: 'Cerrar' }).click()
  }

  expect(pageErrors).toEqual([])
})

test('mobile: completes the human review flow with no horizontal overflow', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  const pageErrors: string[] = []
  page.on('pageerror', (error) => pageErrors.push(error.message))
  const title = `Review ${Date.now()}`
  await createReview(page, title)
  const quickReply = page.locator('.topbar').getByRole('button', { name: 'Responder' })
  await expect(quickReply).toBeVisible()
  await quickReply.click()
  await expect(page.getByLabel('Contenido del mensaje')).toBeFocused()
  const mobileSearch = page.locator('.mobile-review-search input')
  await mobileSearch.fill('no existe')
  await expect(page.getByText('No hay revisiones con ese filtro.')).toBeVisible()
  await mobileSearch.fill(title)
  const decisionsFilter = page.getByRole('button', { name: 'Pendientes' })
  await decisionsFilter.click()
  await expect(decisionsFilter).toHaveAttribute('aria-pressed', 'true')
  await expect(page.getByRole('button', { name: new RegExp(title) })).toHaveCount(0)
  await page.getByRole('button', { name: 'Todas', exact: true }).click()
  await expect(page.getByRole('button', { name: new RegExp(title) })).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Cronología' })).toBeVisible()

  await page.getByLabel('Contenido del mensaje').fill('Propuesta: mantener las decisiones humanas explícitas.')
  await page.getByRole('button', { name: 'Registrar mensaje' }).click()
  await expect(page.getByText(/Propuesta: mantener las decisiones/)).toBeVisible()
  await expect(page.getByText('Mensaje de conversación').first()).toBeVisible()
  await expect(page.getByText('Registro de auditoría').first()).toBeVisible()
  await page.getByRole('button', { name: 'Registrar propuesta' }).click()
  await page.getByRole('button', { name: 'Iniciar revisión' }).click()

  await page.getByLabel('Pregunta para César').fill('¿Aprobamos esta opción?')
  await page.getByRole('button', { name: 'Solicitar respuesta' }).click()
  await expect(page.getByText('Tu decisión es necesaria')).toBeVisible()
  const decisionReply = page.locator('.topbar').getByRole('button', { name: 'Responder decisión' })
  await expect(decisionReply).toBeVisible()
  await decisionReply.click()
  await expect(page.getByLabel('Tu respuesta')).toBeFocused()
  await page.getByLabel('Tu respuesta').fill('Sí, para la prueba sintética.')
  await page.locator('.decision-answer').getByRole('button', { name: 'Enviar respuesta' }).click()
  await expect(page.getByText(/Respuesta: Sí, para la prueba sintética/)).toBeVisible()

  await page.getByRole('button', { name: 'Marcar como resuelta' }).click()
  await page.getByRole('button', { name: 'Aprobar decisión' }).click()
  await expect(page.getByText('Aprobada', { exact: true }).first()).toBeVisible()
  await expect(page.getByText(/Actor declarado · origen no autenticado/).first()).toBeVisible()
  const addComment = page.locator('.topbar').getByRole('button', { name: 'Añadir comentario' })
  await expect(addComment).toBeVisible()
  await addComment.click()
  await expect(page.getByLabel('Contenido del mensaje')).toBeFocused()

  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)
  expect(overflow).toBe(false)
  expect(pageErrors).toEqual([])
  const accessibility = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
  expect(accessibility.violations).toEqual([])
  await page.screenshot({ path: test.info().outputPath('dashboard-mobile.png'), fullPage: true })
})

test('mobile: search and review filters remain available to keyboard and touch users', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  const title = `Filtro móvil ${Date.now()}`
  await createReview(page, title)
  const search = page.locator('.mobile-review-search input')
  await expect(search).toBeVisible()
  await expect(page.locator('.mobile-review-search')).toHaveCSS('min-height', '44px')
  await search.fill('sin coincidencias')
  await expect(page.getByText('No hay revisiones con ese filtro.')).toBeVisible()
  await search.fill(title)
  const pending = page.getByRole('button', { name: 'Pendientes' })
  await pending.click()
  await expect(pending).toHaveAttribute('aria-pressed', 'true')
  await expect(page.getByRole('button', { name: new RegExp(title) })).toHaveCount(0)
  await page.getByRole('button', { name: 'Todas', exact: true }).click()
  await expect(page.getByRole('button', { name: new RegExp(title) })).toBeVisible()
})

test('dialog supports focus containment, Escape, and focus restoration', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 })
  await page.goto('/')
  const opener = page.getByRole('button', { name: 'Crear revisión' }).first()
  await opener.click()
  const dialog = page.getByRole('dialog', { name: 'Crear revisión' })
  const title = dialog.getByLabel('Título')
  await expect(title).toBeFocused()
  await page.keyboard.press('Shift+Tab')
  await expect(dialog.getByRole('button', { name: 'Cerrar' })).toBeFocused()
  await page.keyboard.press('Shift+Tab')
  await expect(dialog.getByRole('button', { name: 'Crear revisión' })).toBeFocused()
  await page.keyboard.press('Escape')
  await expect(dialog).toBeHidden()
  await expect(opener).toBeFocused()
})

test('tablet: keeps the chronology full-width and navigable at 768px', async ({ page }) => {
  await page.setViewportSize({ width: 768, height: 1024 })
  const title = `Tablet ${Date.now()}`
  await createReview(page, title)
  await page.getByLabel('Contenido del mensaje').fill('La cronología debe conservar prioridad en tablet.')
  await page.getByRole('button', { name: 'Registrar mensaje' }).click()
  await expect(page.getByText(/La cronología debe conservar prioridad/)).toBeVisible()

  const layout = await page.evaluate(() => {
    const activity = document.querySelector('.activity-panel')!.getBoundingClientRect()
    const rail = document.querySelector('.annotation-rail')!.getBoundingClientRect()
    const question = document.querySelector('.question-section')!.getBoundingClientRect()
    return { viewportWidth: window.innerWidth, documentWidth: document.documentElement.scrollWidth, activityWidth: activity.width, railWidth: rail.width, questionWidth: question.width, railTop: rail.top, activityBottom: activity.bottom }
  })
  expect(layout.documentWidth).toBeLessThanOrEqual(layout.viewportWidth)
  expect(layout.activityWidth).toBeGreaterThan(420)
  expect(layout.questionWidth).toBeGreaterThan(420)
  expect(layout.questionWidth).toBeGreaterThanOrEqual(layout.railWidth - 2)
  expect(layout.railTop).toBeGreaterThanOrEqual(layout.activityBottom)
  await page.screenshot({ path: test.info().outputPath('dashboard-tablet.png'), fullPage: true })
})

test('desktop: accessible, readable dashboard capture', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 })
  await createReview(page, `Desktop ${Date.now()}`)
  await expect(page.locator('.topbar').getByRole('button', { name: 'Responder' })).toBeHidden()
  await expect(page.locator('.desktop-review-filters').getByRole('button', { name: 'Pendientes' })).toBeVisible()
  await expect(page.locator('.desktop-review-filters').getByRole('button', { name: 'Aprobadas' })).toBeVisible()
  await page.locator('.desktop-review-search input').focus()
  await expect(page.locator('.desktop-review-search')).toHaveCSS('outline-style', 'solid')
  await page.locator('.desktop-review-search input').evaluate((input: HTMLInputElement) => input.blur())
  const accessibility = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
  expect(accessibility.violations).toEqual([])
  await page.screenshot({ path: test.info().outputPath('dashboard-desktop.png'), fullPage: true })
})

test('timeline labels do not overlap at 320px or the 640px 200% zoom layout', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await createReview(page, `Legibilidad ${Date.now()}`)
  await page.getByLabel('Contenido del mensaje').fill('Mensaje de prueba para comprobar autoría, destinatario y hora en pantallas estrechas.')
  await page.getByRole('button', { name: 'Registrar mensaje' }).click()
  const label = page.getByText('Mensaje de conversación').first()
  await expect(label).toBeVisible()

  for (const width of [320, 640]) {
    await page.setViewportSize({ width, height: 900 })
    const layout = await page.evaluate(() => {
      const meta = document.querySelector('.message-entry .entry-meta')!
      const region = document.querySelector('.activity-panel')!
      const children = [...meta.children].map((element) => {
        const { left, right, top, bottom } = element.getBoundingClientRect()
        return { left, right, top, bottom }
      })
      const metaBounds = meta.getBoundingClientRect()
      const regionBounds = region.getBoundingClientRect()
      const overlaps = children.some((a, index) => children.slice(index + 1).some((b) => a.left < b.right - 1 && a.right > b.left + 1 && a.top < b.bottom - 1 && a.bottom > b.top + 1))
      return { viewport: window.innerWidth, document: document.documentElement.scrollWidth, metaRight: metaBounds.right, regionRight: regionBounds.right, overlaps }
    })
    expect(layout.document, `horizontal overflow at ${width}px`).toBeLessThanOrEqual(layout.viewport)
    expect(layout.metaRight, `timeline metadata exceeds its column at ${width}px`).toBeLessThanOrEqual(layout.regionRight + 1)
    expect(layout.overlaps, `timeline metadata overlaps at ${width}px`).toBe(false)
    await expect(label).toBeVisible()
  }
})

test('long review titles stay contained from 320px through desktop', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  const title = 'ArchitectureDecisionRecord'.repeat(4)
  await createReview(page, title)

  for (const width of [320, 390, 768, 1280]) {
    await page.setViewportSize({ width, height: 900 })
    const dimensions = await page.evaluate(() => ({ viewport: window.innerWidth, document: document.documentElement.scrollWidth }))
    expect(dimensions.document, `horizontal overflow at ${width}px`).toBeLessThanOrEqual(dimensions.viewport)
    await expect(page.getByRole('heading', { name: title })).toBeVisible()
  }
})

test('brand and favicon render in their intended size variants', async ({ page }) => {
  await page.goto('/login')
  await page.evaluate(() => {
    const preview = document.createElement('section')
    preview.id = 'brand-size-preview'
    preview.setAttribute('aria-label', 'Brand size preview')
    preview.style.cssText = 'position:fixed;z-index:20;inset:auto 16px 16px auto;display:flex;gap:18px;align-items:center;padding:14px;background:#fff;border:1px solid #d4d2cb'
    preview.innerHTML = [
      ['favicon.svg', 16], ['codexpt.svg', 24], ['codexpt.svg', 32],
    ].map(([source, size]) => `<figure style="display:grid;justify-items:center;gap:6px;margin:0;font:11px sans-serif;color:#171c24"><img src="/${source}" width="${size}" height="${size}" alt="${source} at ${size}px"><figcaption>${source} · ${size}px</figcaption></figure>`).join('')
    document.body.replaceChildren(preview)
    document.body.style.cssText = 'min-height:100vh;display:grid;place-items:center;background:#f7f6f2'
  })
  const preview = page.locator('#brand-size-preview')
  await expect(preview.getByRole('img')).toHaveCount(3)
  await expect(preview.getByRole('img', { name: 'favicon.svg at 16px' })).toBeVisible()
  await expect(preview.getByRole('img', { name: 'codexpt.svg at 24px' })).toBeVisible()
  await expect(preview.getByRole('img', { name: 'codexpt.svg at 32px' })).toBeVisible()
  await expect(page.locator('link[rel="icon"]')).toHaveAttribute('href', '/favicon.svg')
  const imageLoads = await preview.getByRole('img').evaluateAll((images) => images.map((image) => {
    const element = image as HTMLImageElement
    return element.complete && element.naturalWidth > 0
  }))
  expect(imageLoads).toEqual([true, true, true])
  await page.screenshot({ path: test.info().outputPath('brand-size-preview.png'), fullPage: false })
})
