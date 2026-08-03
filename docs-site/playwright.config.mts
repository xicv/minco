import { defineConfig, devices } from '@playwright/test'

const productionBaseURL = process.env.MINCO_DOCS_BASE_URL
const docsPort = Number(process.env.MINCO_DOCS_PORT ?? '41731')
if (!Number.isInteger(docsPort) || docsPort < 1024 || docsPort > 65535) {
  throw new Error('MINCO_DOCS_PORT must be an integer from 1024 through 65535')
}
const baseURL = productionBaseURL ?? `http://127.0.0.1:${docsPort}/minco/`

export default defineConfig({
  testDir: './tests',
  outputDir: '../target/minco/docs-browser/results',
  reporter: [
    ['list'],
    ['html', { outputFolder: '../target/minco/docs-browser/report', open: 'never' }]
  ],
  use: {
    baseURL,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure'
  },
  webServer: productionBaseURL
    ? undefined
    : {
        command: `npm run build && npm run preview -- --host 127.0.0.1 --port ${docsPort} --strictPort`,
        url: baseURL,
        reuseExistingServer: false,
        timeout: 120_000
      },
  projects: [
    {
      name: 'desktop-chromium',
      use: { ...devices['Desktop Chrome'] }
    },
    {
      name: 'mobile-chromium',
      use: { ...devices['Pixel 7'] }
    }
  ]
})
