import { defineConfig } from '@playwright/test'
import { resolve } from 'node:path'

const e2eDatabase = process.env.CODEXPT_E2E_DATABASE ?? resolve(import.meta.dirname, `../target/e2e-${process.pid}-${Date.now()}.sqlite3`)

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  reporter: 'list',
  use: {
    baseURL: 'http://127.0.0.1:4174',
    browserName: 'chromium',
    headless: true,
    trace: 'retain-on-failure',
  },
  webServer: {
    command: `cargo run -- --db "${e2eDatabase}" serve --port 4174`,
    cwd: '..',
    url: 'http://127.0.0.1:4174/api/health',
    reuseExistingServer: false,
    timeout: 60_000,
  },
})
