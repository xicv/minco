import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  outputDir: "./node_modules/.cache/playwright-test-results",
  timeout: 10_000,
  fullyParallel: false,
  workers: 1,
  use: {
    browserName: "chromium",
    headless: true,
  },
  webServer: {
    command: "cargo run --locked --manifest-path ../rust/Cargo.toml",
    url: "http://127.0.0.1:3210/",
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
