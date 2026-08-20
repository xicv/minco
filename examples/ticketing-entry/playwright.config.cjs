const path = require('node:path');
const { defineConfig, devices } = require('@playwright/test');

const artifactRoot = path.resolve(__dirname, '../../target/minco/ticketing-entry-browser');

module.exports = defineConfig({
  testDir: './tests/browser',
  fullyParallel: true,
  forbidOnly: true,
  retries: 0,
  timeout: 15_000,
  expect: { timeout: 5_000 },
  outputDir: path.join(artifactRoot, 'test-results'),
  preserveOutput: 'always',
  reporter: [
    [process.env.CI ? 'github' : 'line'],
    ['html', { outputFolder: path.join(artifactRoot, 'html-report'), open: 'never' }],
    ['junit', { outputFile: path.join(artifactRoot, 'junit.xml') }],
  ],
  use: {
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
    video: 'retain-on-failure',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
  ],
});
