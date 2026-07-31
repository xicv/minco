import { expect, test } from '@playwright/test'

test('landing page leads to stable documentation', async ({ page }) => {
  await page.goto('./')
  await expect(page.getByRole('heading', { level: 1, name: 'Minco' })).toBeVisible()
  await expect(
    page.getByText('Ship Rust web apps straight to AWS.', { exact: true })
  ).toBeVisible()
  await page.getByRole('link', { name: 'Read the 0.5.0 docs' }).click()
  await expect(page).toHaveURL(/\/minco\/0\.5\.0\/$/)
  await expect(page.getByRole('heading', { level: 1, name: 'Minco 0.5.0' })).toBeVisible()
  await expect(page.getByText('Latest stable release.')).toBeVisible()
})

test('local search finds the resource API reference', async ({ page }) => {
  await page.goto('./0.5.0/')
  await page.getByRole('button', { name: 'Search Minco documentation' }).click()
  const search = page.locator('input[type="search"]')
  await search.fill('Resource API reference')
  const result = page.getByText('Resource API reference', { exact: true }).first()
  await expect(result).toBeVisible()
  await result.click()
  await expect(page).toHaveURL(
    /\/0\.5\.0\/reference\/resource-api#resource-api-reference$/
  )
})

test('next is visibly unreleased and links back to stable', async ({ page }) => {
  await page.goto('./next/')
  await expect(page.getByRole('heading', { level: 1, name: 'Next' })).toBeVisible()
  await expect(page.getByText('Unreleased documentation.')).toBeVisible()
  await expect(page.getByRole('link', { name: 'Use stable 0.5.0' })).toHaveAttribute(
    'href',
    '../0.5.0/'
  )
})

test('navigation stays within the mobile viewport', async ({ page, isMobile }) => {
  test.skip(!isMobile, 'mobile project only')
  await page.goto('./0.5.0/tutorials/first-api')
  const dimensions = await page.evaluate(() => ({
    viewport: document.documentElement.clientWidth,
    content: document.documentElement.scrollWidth
  }))
  expect(dimensions.content).toBeLessThanOrEqual(dimensions.viewport)
  await expect(
    page.getByRole('heading', { level: 1, name: 'Build your first API' })
  ).toBeVisible()
  await page.locator('.VPNavBarHamburger').click()
  await expect(page.getByRole('link', { name: 'Documentation' })).toBeVisible()
})

test('core pages have labelled semantics and no browser errors', async ({ page }) => {
  const errors: string[] = []
  page.on('pageerror', error => errors.push(error.message))
  page.on('console', message => {
    if (message.type() === 'error') errors.push(message.text())
  })

  for (const path of ['./', './0.5.0/', './0.5.0/reference/resource-api', './next/']) {
    await page.goto(path)
    await expect(page.locator('h1')).toHaveCount(1)
    await expect(page.getByRole('banner')).toBeVisible()
    const missingLabels = await page.evaluate(() => {
      const images = [...document.querySelectorAll('img')].filter(
        image => !image.hasAttribute('alt')
      )
      const controls = [...document.querySelectorAll('input, button, select, textarea')].filter(
        control =>
          control.getClientRects().length > 0 &&
          !control.getAttribute('aria-label') &&
          !control.getAttribute('aria-labelledby') &&
          !control.getAttribute('title') &&
          !control.textContent?.trim() &&
          !(control instanceof HTMLInputElement && control.labels?.length)
      )
      return { images: images.length, controls: controls.length }
    })
    expect(missingLabels).toEqual({ images: 0, controls: 0 })
  }

  expect(errors).toEqual([])
})
