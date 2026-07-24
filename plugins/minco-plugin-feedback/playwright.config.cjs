const path = require('node:path');
const { defineConfig, devices } = require('@playwright/test');

const artifactRoot = path.resolve(__dirname, '../../target/minco/feedback-browser');

module.exports = defineConfig({
  testDir: './tests/browser',
  fullyParallel: true,
  forbidOnly: true,
  timeout: 15_000,
  expect: {
    timeout: 5_000,
  },
  outputDir: path.join(artifactRoot, 'test-results'),
  preserveOutput: 'always',
  reporter: [
    ['line'],
    ['html', { outputFolder: path.join(artifactRoot, 'html-report'), open: 'never' }],
    ['junit', { outputFile: path.join(artifactRoot, 'junit.xml') }],
  ],
  use: {
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
    video: 'retain-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'firefox',
      use: { ...devices['Desktop Firefox'] },
    },
  ],
});
