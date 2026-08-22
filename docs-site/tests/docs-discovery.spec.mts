import { expect, test, type Page } from '@playwright/test'
import { readFileSync } from 'node:fs'

const release = JSON.parse(
  readFileSync(new URL('../release.json', import.meta.url), 'utf8')
) as { stable: string; workspace: string; state: 'candidate' | 'published' }

const stablePath = `./${release.stable}/`
const workspacePath = `./${release.workspace}/`

async function waitForHydration(page: Page) {
  await expect(page.locator('.VPSwitchAppearance').first()).toHaveAttribute(
    'title',
    /^Switch to (dark|light) theme$/
  )
}

test('current documentation map exposes every major documentation surface', async ({
  page
}) => {
  await page.goto(`${stablePath}reference/documentation-map`)
  await waitForHydration(page)

  await expect(
    page.getByRole('heading', { level: 1, name: 'Documentation Map' })
  ).toBeVisible()

  for (const linkName of [
    'Installation',
    'Build your first application',
    'Architecture',
    'Build a resource API',
    'Configuration',
    'Local development',
    'Troubleshooting',
    'Codex and Claude Code',
    'Waffo hosted payments',
    'Built-in plugins and adapters',
    'Plan an AWS deployment',
    'Production blueprint',
    'CLI commands',
    'Testing and evidence'
  ]) {
    await expect(page.getByRole('link', { name: linkName }).first()).toBeVisible()
  }
})

test('published stable and next expose browser and native client guidance', async ({
  page
}) => {
  await page.goto('./next/reference/documentation-map')
  await waitForHydration(page)
  await expect(
    page.getByRole('link', { name: 'Browser and native clients' }).first()
  ).toBeVisible()

  await page.goto(`${workspacePath}reference/documentation-map`)
  await waitForHydration(page)
  await expect(
    page
      .locator('.VPSidebar')
      .getByRole('link', { name: 'Browser and native clients' })
  ).toBeVisible()

  await page.goto(`${stablePath}reference/documentation-map`)
  await waitForHydration(page)
  await expect(
    page.getByRole('link', { name: 'Browser and native clients' }).first()
  ).toBeVisible()

  await page.goto('./next/')
  await waitForHydration(page)
  await page.getByRole('button', { name: 'Search Minco documentation' }).click()
  await page.locator('input[type="search"]').fill('PKCE mobile API')
  await expect(
    page
      .locator('.VPLocalSearchBox')
      .getByRole('link', { name: 'Browser and native clients' })
      .first()
  ).toBeVisible()
})

test('candidate and next expose the contract-enforced request boundary', async ({
  page
}) => {
  for (const [path, marker] of [
    ['./next/guides/contract-request-validation', 'ValidatedJson'],
    [`${workspacePath}guides/contract-request-validation`, 'ValidatedJson'],
    ['./next/reference/http-request-boundary', 'validation_failed'],
    [`${workspacePath}reference/http-request-boundary`, 'validation_failed']
  ]) {
    await page.goto(path)
    await waitForHydration(page)
    await expect(page.locator('main')).toContainText(marker)
  }

  await page.goto(`${workspacePath}reference/documentation-map`)
  await expect(
    page.getByRole('link', { name: 'Enforce request contracts' }).first()
  ).toBeVisible()
  await expect(
    page.getByRole('link', { name: 'HTTP request boundary' }).first()
  ).toBeVisible()
})

test('search includes current troubleshooting language and excludes frozen manuals', async ({
  page
}) => {
  await page.goto(stablePath)
  await waitForHydration(page)
  await page.getByRole('button', { name: 'Search Minco documentation' }).click()

  const search = page.locator('input[type="search"]')
  await search.fill('stale ETag')

  const results = page.locator('.VPLocalSearchBox')
  const resultLinkTo = (path: string) =>
    results.locator(`[role="option"] > a[href*="${path}"]`)
  const currentSearchSegment = release.state === 'candidate' ? 'next' : release.stable
  await expect(
    resultLinkTo(`/${currentSearchSegment}/guides/troubleshooting`).first()
  ).toBeVisible()

  await search.fill('login')
  await expect(
    resultLinkTo(`/${currentSearchSegment}/guides/identity-and-sessions`).first()
  ).toBeVisible()

  await search.fill('direct upload')
  await expect(
    resultLinkTo(`/${currentSearchSegment}/guides/files-and-static-sites`).first()
  ).toBeVisible()

  await expect(resultLinkTo('/0.5.0/')).toHaveCount(0)
  await expect(resultLinkTo('/0.6.0/')).toHaveCount(0)
  await expect(resultLinkTo('/1.0.0/')).toHaveCount(0)
  if (release.state === 'candidate') {
    await expect(resultLinkTo(`/${release.workspace}/`)).toHaveCount(0)
    await expect(resultLinkTo(`/${release.stable}/`)).toHaveCount(0)
  }
})

test('stable overview presents all five framework planes', async ({ page }) => {
  await page.goto(stablePath)
  await waitForHydration(page)

  const planes = page.locator('.framework-plane')
  await expect(planes).toHaveCount(5)

  for (const label of [
    'Contract',
    'Code',
    'Capabilities',
    'Resources',
    'Evidence'
  ]) {
    await expect(
      planes.locator('span').filter({ hasText: new RegExp(`${label}$`) })
    ).toHaveCount(1)
  }
})

test('versioned command examples match the shipped CLI surface', async ({
  page
}) => {
  await page.goto(`${stablePath}guides/database-lifecycle`)
  await waitForHydration(page)

  const databaseGuide = page.locator('main')
  await expect(databaseGuide).toContainText('--database-url-env')
  await expect(databaseGuide).toContainText('--expected-plan-digest')
  await expect(databaseGuide).toContainText('--receipt')
  await expect(databaseGuide).not.toContainText('--approve-digest')

  await page.goto(`${stablePath}guides/files-and-static-sites`)
  await expect(page.locator('main')).toContainText(
    '--manifest target/minco/release.json'
  )
  await expect(page.locator('main')).not.toContainText('--release-manifest')

  await page.goto(`${stablePath}reference/cli`)
  const cliReference = page.locator('main')
  const pluginCommands = cliReference
    .getByRole('row')
    .filter({ has: page.getByRole('cell', { name: 'Plugins', exact: true }) })
  await expect(pluginCommands).toContainText('add')
  await expect(pluginCommands).toContainText('doctor')
  await expect(pluginCommands).toContainText('remove')
})

test('architecture and troubleshooting pages stay readable on narrow viewports', async ({
  page,
  isMobile
}) => {
  test.skip(!isMobile, 'mobile project only')

  for (const path of [
    `${stablePath}explanation/architecture`,
    `${stablePath}guides/troubleshooting`,
    `${stablePath}reference/documentation-map`,
    './next/explanation/architecture',
    './next/guides/troubleshooting',
    './next/reference/documentation-map'
  ]) {
    await page.goto(path)
    await waitForHydration(page)
    const dimensions = await page.evaluate(() => ({
      viewport: document.documentElement.clientWidth,
      content: document.documentElement.scrollWidth
    }))
    expect(dimensions.content).toBeLessThanOrEqual(dimensions.viewport)
  }
})
