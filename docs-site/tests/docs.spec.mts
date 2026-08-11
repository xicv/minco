import { expect, test, type Page } from '@playwright/test'
import { readFileSync } from 'node:fs'

const release = JSON.parse(
  readFileSync(new URL('../release.json', import.meta.url), 'utf8')
) as { stable: string; workspace: string; state: 'candidate' | 'published' }

const stablePath = `./${release.stable}/`
const candidatePath = `./${release.workspace}/`
const workspaceSegment = release.state === 'published' ? release.workspace : 'next'
const workspacePath = `./${workspaceSegment}/`

async function waitForHydration(page: Page) {
  await expect(page.locator('.VPSwitchAppearance').first()).toHaveAttribute(
    'title',
    /^Switch to (dark|light) theme$/
  )
}

test('landing page leads to stable documentation', async ({ page }) => {
  await page.goto('./')
  await waitForHydration(page)
  await expect(page.getByRole('heading', { level: 1, name: 'Minco' })).toBeVisible()
  await expect(
    page.getByText('One contract. Every runtime. No idle application compute.', {
      exact: true
    })
  ).toBeVisible()
  await expect(
    page.getByRole('img', {
      name: /Minco contract flowing into local, HTTP API, and queue worker runtimes/
    })
  ).toBeVisible()
  await page.getByRole('link', { name: `Read the ${release.stable} docs` }).click()
  await expect(page).toHaveURL(
    new RegExp(`/minco/${release.stable.replaceAll('.', '\\.')}\\/$`)
  )
  await expect(
    page.getByRole('heading', { level: 1, name: `Minco ${release.stable}` })
  ).toBeVisible()
  await expect(page.getByText('Latest stable release.')).toBeVisible()
})

test('landing page explains the operating model and links to a real blueprint', async ({
  page,
  isMobile
}) => {
  await page.goto('./')
  await waitForHydration(page)
  await expect(
    page.getByRole('heading', {
      level: 2,
      name: 'Keep the contract, application, infrastructure, and evidence connected.'
    })
  ).toBeVisible()
  const flow = page.locator('.home-flow')
  const flowSteps = page.locator('.home-flow-step')
  await expect(flowSteps).toHaveCount(4)
  await expect(flow).toHaveCSS('list-style-type', 'none')
  await expect(flowSteps.nth(1)).toHaveCSS('margin-top', '0px')
  if (!isMobile) {
    const stepTops = await flowSteps.evaluateAll((steps) =>
      steps.map((step) => Math.round(step.getBoundingClientRect().top))
    )
    expect(new Set(stepTops).size).toBe(1)
  }
  await expect(page.locator('.home-zero-value')).toHaveText('0')

  await page.getByRole('link', { name: 'Explore the production blueprint' }).click()
  await expect(page).toHaveURL(
    new RegExp(
      `/minco/${release.stable.replaceAll('.', '\\.')}\\/cookbook\\/production-blueprint$`
    )
  )
  await expect(
    page.getByRole('heading', {
      level: 1,
      name: 'Production Blueprint for a Burst-Ready Orders Service'
    })
  ).toBeVisible()
})

test('local search finds the resource API reference', async ({ page }) => {
  await page.goto(stablePath)
  await waitForHydration(page)
  await page.getByRole('button', { name: 'Search Minco documentation' }).click()
  const search = page.locator('input[type="search"]')
  await search.fill('Resource API')
  const result = page
    .locator('.VPLocalSearchBox')
    .locator(`a[href*="/${release.stable}/reference/resource-api"]`)
    .first()
  await expect(result).toBeVisible()
  await result.click()
  await expect(page).toHaveURL(
    new RegExp(
      `/${release.stable.replaceAll('.', '\\.')}\\/reference\\/resource-api(?:#resource-api)?$`
    )
  )
})

test('next is visibly unreleased and exposes detailed learning paths', async ({ page }) => {
  await page.goto('./next/')
  await waitForHydration(page)
  await expect(page.getByRole('heading', { level: 1, name: 'Next' })).toBeVisible()
  await expect(page.getByText('Unreleased documentation.')).toBeVisible()
  await expect(
    page.getByRole('link', { name: `Use stable ${release.stable}` })
  ).toHaveAttribute('href', `/minco/${release.stable}/`)
  await expect(page.getByRole('link', { name: 'Build an application' })).toBeVisible()
  await expect(page.getByRole('link', { name: 'Use resource APIs' })).toBeVisible()
  await expect(page.getByRole('link', { name: 'Author a plugin' })).toBeVisible()
  await expect(
    page.getByRole('link', { name: 'Develop with coding agents' })
  ).toBeVisible()
  await expect(page.getByRole('link', { name: 'Operate on AWS' })).toBeVisible()
  await expect(page.getByRole('link', { name: 'Browse all features' })).toBeVisible()
  await expect(
    page.getByRole('link', { name: 'Choose built-in plugins' })
  ).toBeVisible()
  await expect(
    page.getByRole('link', { name: 'Follow practical recipes' })
  ).toBeVisible()
  await expect(
    page.getByRole('link', { name: 'Follow a production blueprint' })
  ).toBeVisible()

  await page.getByRole('link', { name: 'Use resource APIs' }).click()
  await expect(page).toHaveURL(/\/next\/guides\/resource-api$/)
  await expect(
    page.getByRole('heading', { level: 1, name: 'Build a Resource API' })
  ).toBeVisible()
  await expect(page.getByText('Unreleased documentation.')).toBeVisible()
  await expect(
    page.getByRole('heading', { level: 2, name: 'Complete Request Flow' })
  ).toBeVisible()
})

test('production blueprint documents cost, failure, and evidence boundaries', async ({
  page
}) => {
  await page.goto(`${stablePath}cookbook/production-blueprint`)
  await waitForHydration(page)
  await expect(
    page.getByRole('heading', {
      level: 1,
      name: 'Production Blueprint for a Burst-Ready Orders Service'
    })
  ).toBeVisible()
  await expect(
    page.getByRole('heading', { level: 2, name: 'Design from the traffic pattern' })
  ).toBeVisible()
  await expect(
    page.getByRole('heading', { level: 2, name: 'Wake and residual cost model' })
  ).toBeVisible()
  await expect(
    page.getByRole('heading', { level: 2, name: 'Failure and recovery design' })
  ).toBeVisible()
  await expect(
    page.locator('pre').filter({ hasText: 'deny_fixed_compute = true' })
  ).toBeVisible()
  await expect(
    page.getByRole('link', { name: 'Orders API end-to-end recipe' })
  ).toBeVisible()
})

test('next documents the complete built-in component catalog', async ({ page }) => {
  await page.goto('./next/plugins/')
  await waitForHydration(page)
  await expect(
    page.getByRole('heading', { level: 1, name: 'Built-in Plugins and Adapters' })
  ).toBeVisible()
  await expect(page.getByText('19 built-in components')).toBeVisible()
  for (const name of [
    'Health',
    'Idempotency',
    'Waffo Payments',
    'Identity',
    'Sessions',
    'Feedback',
    'Realtime',
    'AWS Lambda',
    'AWS Worker',
    'AWS DynamoDB',
    'SQLx PostgreSQL',
    'SQLx SQLite'
  ]) {
    await expect(page.getByRole('heading', { level: 2, name })).toBeVisible()
  }

  await page.getByRole('button', { name: 'Search Minco documentation' }).click()
  const search = page.locator('input[type="search"]')
  await search.fill('queues and workers')
  await expect(
    page
      .locator('.VPLocalSearchBox')
      .locator(`a[href*="/${workspaceSegment}/guides/background-work"]`)
      .first()
  ).toBeVisible()
})

test('current manual documents the Waffo authority boundary', async ({ page }) => {
  await page.goto(`./${release.workspace}/guides/payments-waffo`)
  await waitForHydration(page)
  await expect(
    page.getByRole('heading', { level: 1, name: 'Waffo Hosted Payments' })
  ).toBeVisible()
  await expect(page.getByText('It does not add a generic billing model')).toBeVisible()
  await expect(page.getByText('A checkout return URL is navigation, not payment proof.')).toBeVisible()
  await expect(page.getByText('provider_live = not_run')).toBeVisible()
})

test('version navigation resolves to the frozen current stable manual', async ({
  page,
  isMobile
}) => {
  if (release.state === 'candidate') {
    await page.goto(stablePath)
    await waitForHydration(page)
    if (isMobile) {
      await page.getByRole('button', { name: 'mobile navigation' }).click()
      const versionButton = page.getByRole('button', {
        name: `Version ${release.stable}`
      })
      await versionButton.focus()
      await page.keyboard.press('Enter')
    } else {
      await page.getByRole('button', { name: `Version ${release.stable}` }).click()
    }
    const candidateLink = page.getByRole('link', {
      name: `${release.workspace} · Release candidate`
    })
    if (isMobile) {
      await candidateLink.focus()
      await page.keyboard.press('Enter')
    } else {
      await candidateLink.click()
    }
  } else {
    await page.goto(candidatePath)
    await waitForHydration(page)
  }
  await expect(page).toHaveURL(
    new RegExp(`/minco/${release.workspace.replaceAll('.', '\\.')}\\/$`)
  )
  await expect(
    page.getByRole('heading', { level: 1, name: `Minco ${release.workspace}` })
  ).toBeVisible()
  if (release.state === 'candidate') {
    await expect(page.getByText('Release candidate documentation.')).toBeVisible()
  } else {
    await expect(page.getByText('Latest stable release.')).toBeVisible()
    await expect(page.getByText('Release candidate documentation.')).toHaveCount(0)
  }

  await page.goto(`${candidatePath}getting-started/installation`)
  await waitForHydration(page)
  const expectedInstallVersion =
    release.state === 'candidate' ? release.stable : release.workspace
  await expect(
    page.getByText(`cargo add minco@${expectedInstallVersion}`, { exact: true })
  ).toBeVisible()

  for (const path of [
    `${candidatePath}guides/realtime`,
    `${candidatePath}guides/dynamodb`,
    `${candidatePath}guides/project-view`,
    `${candidatePath}guides/agent-development`,
    `${candidatePath}guides/deployment`
  ]) {
    await page.goto(path)
    await waitForHydration(page)
    await expect(page.locator('h1')).toHaveCount(1)
    if (release.state === 'candidate') {
      await expect(page.getByText('Release candidate documentation.')).toBeVisible()
    } else {
      await expect(page.getByText('Release candidate documentation.')).toHaveCount(0)
    }
  }
})

test('local search finds workspace plugin conformance documentation', async ({ page }) => {
  await page.goto('./next/')
  await waitForHydration(page)
  await page.getByRole('button', { name: 'Search Minco documentation' }).click()
  const search = page.locator('input[type="search"]')
  await search.fill('Plugin conformance')
  const result = page
    .locator('.VPLocalSearchBox')
    .locator(`a[href*="/${workspaceSegment}/reference/plugin-conformance"]`)
    .first()
  await expect(result).toBeVisible()
  await result.click()
  await expect(page).toHaveURL(
    new RegExp(
      `/${workspaceSegment.replaceAll('.', '\\.')}\\/reference\\/plugin-conformance#plugin-conformance$`
    )
  )
})

test('workspace documentation includes the plugin conformance API', async ({ page }) => {
  await page.goto(`${workspacePath}reference/plugin-conformance`)
  await waitForHydration(page)
  await expect(
    page.getByRole('heading', { level: 1, name: 'Plugin Conformance' })
  ).toBeVisible()
  if (release.state === 'published') {
    await expect(page.getByText('Unreleased documentation.')).toHaveCount(0)
  } else {
    await expect(page.getByText('Unreleased documentation.')).toBeVisible()
  }
})

test('workspace documentation explains guarded agent setup', async ({ page }) => {
  await page.goto(`${workspacePath}guides/agent-development`)
  await waitForHydration(page)
  await expect(
    page.getByRole('heading', {
      level: 1,
      name: 'Develop with Codex and Claude Code'
    })
  ).toBeVisible()
  await expect(page.getByText('cargo minco agent plan --target all --json')).toBeVisible()
  await expect(
    page.getByRole('heading', { name: 'Authority remains explicit' })
  ).toBeVisible()
})

test('navigation stays within the mobile viewport', async ({ page, isMobile }) => {
  test.skip(!isMobile, 'mobile project only')
  for (const path of [
    `${stablePath}getting-started/first-application`,
    `${stablePath}cookbook/production-blueprint`,
    './next/guides/resource-api',
    './next/guides/agent-development',
    './next/plugins/',
    './next/cookbook/',
    './next/cookbook/production-blueprint'
  ]) {
    await page.goto(path)
    await waitForHydration(page)
    const dimensions = await page.evaluate(() => ({
      viewport: document.documentElement.clientWidth,
      content: document.documentElement.scrollWidth
    }))
    expect(dimensions.content).toBeLessThanOrEqual(dimensions.viewport)
  }

  await expect(page.getByText('Unreleased documentation.')).toBeVisible()
  await page.locator('.VPNavBarHamburger').click()
  await expect(page.getByRole('link', { name: 'Documentation' })).toBeVisible()
  const navigationScreen = await page.locator('.VPNavScreen').evaluate(screen => {
    const bounds = screen.getBoundingClientRect()
    const topElement = document.elementFromPoint(
      window.innerWidth / 2,
      bounds.top + 24
    )
    return {
      height: bounds.height,
      viewportHeight: window.innerHeight,
      ownsTopPoint: topElement !== null && screen.contains(topElement)
    }
  })
  expect(navigationScreen.height).toBeGreaterThan(
    navigationScreen.viewportHeight / 2
  )
  expect(navigationScreen.ownsTopPoint).toBe(true)
})

test('core pages have labelled semantics and no browser errors', async ({ page }) => {
  const errors: string[] = []
  page.on('pageerror', error => errors.push(error.message))
  page.on('console', message => {
    if (message.type() === 'error') errors.push(message.text())
  })

  for (const path of [
    './',
    stablePath,
    `${stablePath}reference/resource-api`,
    `${stablePath}cookbook/production-blueprint`,
    `${workspacePath}reference/plugin-conformance`,
    './next/',
    './next/guides/resource-api',
    './next/reference/plugin-conformance',
    './next/examples/',
    './next/features/',
    './next/plugins/',
    './next/cookbook/',
    './next/cookbook/production-blueprint'
  ]) {
    await page.goto(path)
    await waitForHydration(page)
    await expect(page.locator('h1')).toHaveCount(1)
    await expect(page.getByRole('banner')).toBeVisible()
    const missingLabels = await page.evaluate(() => {
      const images = [...document.querySelectorAll('img')].filter(
        image => !image.hasAttribute('alt')
      )
      const controls = [
        ...document.querySelectorAll('input, button, select, textarea')
      ].filter(
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
    if (path.startsWith('./next/')) {
      await expect(page.getByText('Unreleased documentation.')).toBeVisible()
    }
  }

  expect(errors).toEqual([])
})
