import { rm } from 'node:fs/promises'
import { spawn } from 'node:child_process'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const webDirectory = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const database = resolve(webDirectory, `../target/e2e-${process.pid}-${Date.now()}.sqlite3`)
const playwrightCli = resolve(webDirectory, 'node_modules/@playwright/test/cli.js')

let exitCode = 1
try {
  exitCode = await new Promise((resolveExit, reject) => {
    const child = spawn(process.execPath, [playwrightCli, 'test'], {
      cwd: webDirectory,
      env: { ...process.env, CODEXPT_E2E_DATABASE: database },
      stdio: 'inherit',
    })
    child.once('error', reject)
    child.once('exit', (code, signal) => {
      if (signal) reject(new Error(`Playwright exited from signal ${signal}`))
      else resolveExit(code ?? 1)
    })
  })
} catch (error) {
  console.error(error)
} finally {
  for (const path of [database, `${database}-shm`, `${database}-wal`]) {
    await rm(path, { force: true })
  }
}

process.exitCode = exitCode
