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

test('mobile: completes the human review flow with no horizontal overflow', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  const pageErrors: string[] = []
  page.on('pageerror', (error) => pageErrors.push(error.message))
  const title = `Review ${Date.now()}`
  await createReview(page, title)
  await expect(page.getByRole('heading', { name: 'Cronología' })).toBeVisible()

  await page.getByLabel('Contenido del mensaje').fill('Propuesta: mantener las decisiones humanas explícitas.')
  await page.getByRole('button', { name: 'Registrar propuesta' }).click()
  await expect(page.getByText(/Propuesta: mantener las decisiones/)).toBeVisible()
  await page.getByRole('button', { name: 'Iniciar revisión' }).click()

  await page.getByLabel('Pregunta para César').fill('¿Aprobamos esta opción?')
  await page.getByRole('button', { name: 'Solicitar respuesta' }).click()
  await expect(page.getByText('Tu decisión es necesaria')).toBeVisible()
  await page.getByLabel('Tu respuesta').fill('Sí, para la prueba sintética.')
  await page.getByRole('button', { name: 'Responder' }).click()
  await expect(page.getByText(/Respuesta: Sí, para la prueba sintética/)).toBeVisible()

  await page.getByRole('button', { name: 'Marcar como resuelta' }).click()
  await page.getByRole('button', { name: 'Aprobar decisión' }).click()
  await expect(page.getByText('Aprobada', { exact: true }).first()).toBeVisible()
  await expect(page.getByText(/Actor declarado · origen no autenticado/).first()).toBeVisible()

  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)
  expect(overflow).toBe(false)
  expect(pageErrors).toEqual([])
  const accessibility = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
  expect(accessibility.violations).toEqual([])
  await page.screenshot({ path: test.info().outputPath('dashboard-mobile.png'), fullPage: true })
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
  const accessibility = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
  expect(accessibility.violations).toEqual([])
  await page.screenshot({ path: test.info().outputPath('dashboard-desktop.png'), fullPage: true })
})
