import { rmSync } from 'node:fs'
import { resolve } from 'node:path'
import type { FullConfig } from '@playwright/test'

export default function globalSetup(_config: FullConfig) {
  const database = resolve(import.meta.dirname, '../../target/e2e.sqlite3')
  for (const path of [database, `${database}-shm`, `${database}-wal`]) {
    rmSync(path, { force: true })
  }
}
