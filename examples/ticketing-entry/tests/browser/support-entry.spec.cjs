const fs = require('node:fs');
const path = require('node:path');
const { test, expect } = require('@playwright/test');

const launcherSource = fs.readFileSync(
  path.resolve(__dirname, '../../support-entry.js'),
  'utf8',
);

async function loadLauncher(
  page,
  { portalReady = true, hostTheme = 'light', includeInline = false } = {},
) {
  const requests = [];
  await page.context().route(/^https:\/\/app\.example\.test\//, async route => {
    const request = route.request();
    const url = new URL(request.url());
    requests.push({ method: request.method(), pathname: url.pathname });
    if (request.isNavigationRequest()) {
      await route.fulfill({
        contentType: 'text/html; charset=utf-8',
        body: `<!doctype html>
          <html lang="en" style="color-scheme:${hostTheme}">
            <head>
              <meta name="csrf-token" content="test-csrf" />
              <title>Sensitive employee order title</title>
              <style>
                body { margin: 0; min-height: 100vh; background: ${hostTheme === 'dark' ? '#111827' : '#ffffff'}; }
                #inline-host { margin: 32px; }
              </style>
            </head>
            <body>
              <a id="before" href="#before">Before launcher</a>
              <main><h1>Host application</h1><div id="inline-host"></div></main>
              <minco-support-launcher
                id="floating"
                portal="https://support.example.test/"
                project="example"
                handoff-endpoint="/api/support/handoff"
                label="Get support"
                ready-timeout-ms="1000"
              ></minco-support-launcher>
              ${includeInline ? `<minco-support-launcher
                id="inline"
                mode="inline"
                portal="https://support.example.test/"
                project="example"
                handoff-endpoint="/api/support/handoff"
                label="Inline support"
              ></minco-support-launcher>` : ''}
              <button id="after" type="button">After launcher</button>
              <script type="module" src="/support-entry.js"></script>
            </body>
          </html>`,
      });
      return;
    }
    if (url.pathname === '/support-entry.js') {
      await route.fulfill({
        contentType: 'application/javascript; charset=utf-8',
        body: launcherSource,
      });
      return;
    }
    if (url.pathname === '/api/support/handoff' && request.method() === 'POST') {
      await route.fulfill({
        json: {
          launch_url: `https://support.example.test/start#handoff=${'a'.repeat(64)}`,
          expires_at: new Date(Date.now() + 120_000).toISOString(),
        },
      });
      return;
    }
    await route.fulfill({ status: 404, body: 'not found' });
  });

  await page.context().route(/^https:\/\/support\.example\.test\//, async route => {
    const request = route.request();
    requests.push({ method: request.method(), pathname: new URL(request.url()).pathname });
    await route.fulfill({
      contentType: 'text/html; charset=utf-8',
      body: `<!doctype html>
        <html lang="en"><body><main><h1>Canonical support portal</h1></main>
          ${portalReady ? `<script>parent.postMessage({type:'minco.support.ready'}, 'https://app.example.test')</script>` : ''}
        </body></html>`,
    });
  });

  await page.goto('https://app.example.test/orders/opaque?token=secret#notes');
  const floating = page.locator('#floating');
  await expect(floating.getByRole('button', { name: 'Get support' })).toBeVisible();
  return { floating, requests };
}

test('supports keyboard launch, trapped focus, Escape close and focus restoration', async ({ page }) => {
  const { floating } = await loadLauncher(page);
  const launcher = floating.getByRole('button', { name: 'Get support' });
  await expect(launcher).toHaveAttribute('aria-haspopup', 'dialog');
  await expect(launcher).toHaveAttribute('aria-expanded', 'false');

  await launcher.focus();
  await page.keyboard.press('Enter');
  const dialog = floating.getByRole('dialog', { name: 'Get support' });
  await expect(dialog).toBeVisible();
  await expect(launcher).toHaveAttribute('aria-expanded', 'true');
  const close = dialog.getByRole('button', { name: 'Close support' });
  await expect(close).toBeFocused();

  await page.keyboard.press('Shift+Tab');
  await expect(dialog.locator('iframe')).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(close).toBeFocused();
  await expect(page.locator('#before')).not.toBeFocused();
  await expect(page.locator('#after')).not.toBeFocused();

  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(launcher).toHaveAttribute('aria-expanded', 'false');
  await expect(launcher).toBeFocused();
});

test('uses mobile full-screen layout without horizontal overflow at 200 percent zoom', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const { floating } = await loadLauncher(page);
  await floating.getByRole('button', { name: 'Get support' }).click();
  const dialog = floating.getByRole('dialog');
  await expect(dialog).toBeVisible();
  expect(await dialog.boundingBox()).toEqual({ x: 0, y: 0, width: 390, height: 844 });

  await page.evaluate(() => {
    document.documentElement.style.zoom = '2';
  });
  const overflow = await page.evaluate(() => ({
    clientWidth: document.documentElement.clientWidth,
    scrollWidth: document.documentElement.scrollWidth,
  }));
  expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.clientWidth);
});

test('honors reduced motion and stays visible on light and dark host pages', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce', colorScheme: 'dark' });
  const { floating } = await loadLauncher(page, { hostTheme: 'dark' });
  const launcher = floating.getByRole('button', { name: 'Get support' });
  await expect(launcher).toBeVisible();
  await launcher.click();
  const motion = await floating.getByRole('dialog').evaluate(element => {
    const computed = getComputedStyle(element);
    return { animation: computed.animationName, transition: computed.transitionDuration };
  });
  expect(motion).toEqual({ animation: 'none', transition: '0s' });

  await page.evaluate(() => {
    document.body.style.background = '#fff';
  });
  await expect(floating.getByRole('button', { name: 'Close support' })).toBeVisible();
});

test('retains an accessible iframe fallback link when readiness is not confirmed', async ({ page }) => {
  const { floating } = await loadLauncher(page, { portalReady: false });
  await floating.getByRole('button', { name: 'Get support' }).click();
  const fallback = floating.getByRole('link', { name: 'Open support in a new tab' });
  await expect(fallback).toBeVisible({ timeout: 3_000 });
  await expect(fallback).toHaveAttribute('rel', 'noopener noreferrer');
  await expect(fallback).toHaveAttribute('href', /^https:\/\/support\.example\.test\/start#handoff=/);
});

test('supports floating and inline launch modes without inheriting host layout', async ({ page }) => {
  const { floating } = await loadLauncher(page, { includeInline: true });
  const inline = page.locator('#inline');
  await expect(inline.getByRole('button', { name: 'Inline support' })).toBeVisible();
  expect(await floating.evaluate(element => getComputedStyle(element).position)).toBe('fixed');
  expect(await inline.evaluate(element => getComputedStyle(element).position)).toBe('relative');
});

test('rejects forged portal messages and accepts the closed close command', async ({ page }) => {
  const { floating } = await loadLauncher(page);
  await floating.getByRole('button', { name: 'Get support' }).click();
  const dialog = floating.getByRole('dialog');
  await expect(dialog).toBeVisible();

  await page.evaluate(() => {
    window.postMessage({ type: 'minco.support.close', extra: true }, 'https://app.example.test');
  });
  await expect(dialog).toBeVisible();
  await floating.locator('iframe').contentFrame().locator('body').evaluate(() => {
    parent.postMessage({ type: 'minco.support.close' }, 'https://app.example.test');
  });
  await expect(dialog).toHaveCount(0);
});

test('reserves a no-opener tab synchronously before the handoff completes', async ({ page }) => {
  const { floating } = await loadLauncher(page);
  await floating.evaluate(element => element.setAttribute('target', 'tab'));
  const popupPromise = page.waitForEvent('popup');
  await floating.getByRole('button', { name: 'Get support' }).click();
  const popup = await popupPromise;
  await popup.waitForURL(/^https:\/\/support\.example\.test\/start#handoff=/);
  expect(await popup.evaluate(() => window.opener === null)).toBe(true);
});
