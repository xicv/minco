import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './tests',
  outputDir: './node_modules/.cache/playwright-test-results',
  timeout: 60_000,
  expect: { timeout: 15_000 },
  workers: 1,
  fullyParallel: false,
  forbidOnly: true,
  reporter: [['line']],
  use: {
    headless: true,
    screenshot: 'off',
    trace: 'off',
    video: 'off',
  },
})
