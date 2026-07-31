import { defineConfig, devices } from '@playwright/test'

const productionBaseURL = process.env.MINCO_DOCS_BASE_URL
const baseURL = productionBaseURL ?? 'http://127.0.0.1:4173/minco/'

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
        command: 'npm run build && npm run preview -- --host 127.0.0.1 --port 4173',
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
