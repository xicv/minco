import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  testMatch: 'aws-live.spec.mjs',
  outputDir: './node_modules/.cache/playwright-aws-test-results',
  timeout: 30_000,
  workers: 1,
  use: {
    browserName: 'chromium',
    headless: true,
  },
});
