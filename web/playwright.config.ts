import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  globalSetup: './e2e/global-setup.ts',
  fullyParallel: false,
  reporter: 'list',
  use: {
    baseURL: 'http://127.0.0.1:4174',
    browserName: 'chromium',
    headless: true,
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'cargo run -- --db target/e2e.sqlite3 serve --port 4174',
    cwd: '..',
    url: 'http://127.0.0.1:4174/api/health',
    reuseExistingServer: false,
    timeout: 60_000,
  },
})
