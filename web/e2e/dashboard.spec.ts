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

  for (const [width, height, suffix] of [[390, 844, 'mobile'], [1280, 900, 'desktop']] as const) {
    await page.setViewportSize({ width, height })
    await page.goto('/login')
    await expect(page.getByRole('heading', { name: /Las buenas decisiones se revisan en equipo/ })).toBeVisible()
    await expect(page.locator('.login-button')).toHaveAttribute('aria-disabled', 'true')
    await expect(page.getByText('La conexión de identidad se habilitará al completar la configuración del entorno.')).toBeVisible()
    await expectAccessibleCapture(page, `login-${suffix}.png`)

    await page.goto('/invite')
    await expect(page.getByRole('heading', { name: /Entra a un espacio de trabajo compartido/ })).toBeVisible()
    await expect(page.getByText('La conexión de identidad aún no está configurada.')).toBeVisible()
    await expectAccessibleCapture(page, `invite-${suffix}.png`)
  }

  expect(pageErrors).toEqual([])
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

test('hosted owner: member management screen renders and creates a one-time invite', async ({ page }) => {
  const task = {
    task_id: '00000000-0000-4000-8000-000000000001', title: 'Review de muestra', description: 'Datos sintéticos de QA.',
    status: 'open', pending_decision: false, version: 1,
    created_at: '2026-10-10T10:00:00Z', updated_at: '2026-10-10T10:00:00Z',
  }
  const workspaceId = '00000000-0000-4000-8000-000000000002'
  let inviteRequest: { email?: string; role?: string } | null = null
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
    if (url.pathname === `/api/workspaces/${workspaceId}/invites` && request.method() === 'POST') {
      inviteRequest = request.postDataJSON() as { email?: string; role?: string }
      return route.fulfill({ json: { invite_id: 'synthetic-invite', raw_token: 'synthetic-one-time-token', expires_at: '2026-10-17T10:00:00Z' } })
    }
    return route.fulfill({ status: 404, json: { error: `No synthetic QA route for ${request.method()} ${url.pathname}` } })
  })

  for (const [width, height, suffix] of [[390, 844, 'mobile'], [1280, 900, 'desktop']] as const) {
    await page.setViewportSize({ width, height })
    await page.goto('/')
    await expect(page.getByRole('button', { name: 'Miembros' })).toBeVisible()
    await page.getByRole('button', { name: 'Miembros' }).click()
    const dialog = page.getByRole('dialog', { name: 'Miembros' })
    await expect(dialog.getByText('owner@example.test')).toBeVisible()
    await expect(dialog.getByText('editor@example.test')).toBeVisible()
    const evidence = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
    expect(evidence.violations).toEqual([])
    await page.screenshot({ path: test.info().outputPath(`members-${suffix}.png`), fullPage: true })
    await dialog.getByLabel('Invitar por correo verificado').fill('new-member@example.test')
    await dialog.getByLabel('Rol inicial').selectOption('VIEWER')
    await dialog.getByRole('button', { name: 'Crear invitación' }).click()
    await expect(dialog.getByText('synthetic-one-time-token')).toBeVisible()
    expect(inviteRequest).toEqual({ email: 'new-member@example.test', role: 'VIEWER' })
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
  const decisionsFilter = page.getByRole('button', { name: 'Decisiones pendientes' })
  await decisionsFilter.click()
  await expect(decisionsFilter).toHaveAttribute('aria-pressed', 'true')
  await expect(page.getByRole('button', { name: new RegExp(title) })).toHaveCount(0)
  await page.getByRole('button', { name: 'Todas', exact: true }).click()
  await expect(page.getByRole('button', { name: new RegExp(title) })).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Cronología' })).toBeVisible()

  await page.getByLabel('Contenido del mensaje').fill('Propuesta: mantener las decisiones humanas explícitas.')
  await page.getByRole('button', { name: 'Registrar propuesta' }).click()
  await expect(page.getByText(/Propuesta: mantener las decisiones/)).toBeVisible()
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
  await search.fill('sin coincidencias')
  await expect(page.getByText('No hay revisiones con ese filtro.')).toBeVisible()
  await search.fill(title)
  const pending = page.getByRole('button', { name: 'Decisiones pendientes' })
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
    return { viewportWidth: window.innerWidth, documentWidth: document.documentElement.scrollWidth, activityWidth: activity.width, railTop: rail.top, activityBottom: activity.bottom }
  })
  expect(layout.documentWidth).toBeLessThanOrEqual(layout.viewportWidth)
  expect(layout.activityWidth).toBeGreaterThan(420)
  expect(layout.railTop).toBeGreaterThanOrEqual(layout.activityBottom)
  await page.screenshot({ path: test.info().outputPath('dashboard-tablet.png'), fullPage: true })
})

test('desktop: accessible, readable dashboard capture', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 })
  await createReview(page, `Desktop ${Date.now()}`)
  await expect(page.locator('.topbar').getByRole('button', { name: 'Responder' })).toBeHidden()
  const accessibility = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
  expect(accessibility.violations).toEqual([])
  await page.screenshot({ path: test.info().outputPath('dashboard-desktop.png'), fullPage: true })
})
