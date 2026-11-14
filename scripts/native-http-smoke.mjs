import { spawn } from 'node:child_process'
import { mkdtemp, rm } from 'node:fs/promises'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { cargoPath, root } from './tooling.mjs'

// Keep acceptance data and listeners separate from an already running user demo.
const directory = await mkdtemp(join(tmpdir(), 'space-health-native-'))
const cargo = cargoPath()
let server
async function build(manifest, kind, name) {
  const child = spawn(cargo, ['build', '--locked', '--manifest-path', manifest, kind, name, '--message-format=json'], { cwd: root, stdio: ['ignore', 'pipe', 'inherit'] })
  let executable
  let pending = ''
  child.stdout.on('data', chunk => {
    pending += chunk
    let newline
    while ((newline = pending.indexOf('\n')) >= 0) {
      const line = pending.slice(0, newline)
      pending = pending.slice(newline + 1)
      try {
        const result = JSON.parse(line)
        if (result.reason === 'compiler-artifact' && result.target?.name === name && result.executable) executable = result.executable
        if (result.reason === 'compiler-message' && result.message?.rendered) process.stderr.write(result.message.rendered)
      } catch { /* Cargo progress remains on stderr. */ }
    }
  })
  await done(child)
  if (!executable) throw new Error(`No Cargo artifact for ${name}`)
  return executable
}
function done(child) {
  return new Promise((resolve, reject) => {
    child.once('error', reject)
    child.once('exit', (code, signal) => code === 0 ? resolve() : reject(new Error(`Process failed: ${code ?? signal}`)))
  })
}
async function freePort() {
  const listener = createServer()
  await new Promise((resolve, reject) => { listener.once('error', reject); listener.listen(0, '127.0.0.1', resolve) })
  const { port } = listener.address()
  await new Promise(resolve => listener.close(resolve))
  return port
}
async function stop() {
  if (!server || server.exitCode !== null || server.signalCode) return
  const exited = new Promise(resolve => server.once('exit', resolve))
  server.kill('SIGTERM')
  const timeout = setTimeout(() => server.kill('SIGKILL'), 5000)
  try { await exited } finally { clearTimeout(timeout) }
}
try {
  const binary = await build('server/Cargo.toml', '--bin', 'space-health-server')
  const probe = await build('ui/Cargo.toml', '--example', 'shared_api_smoke')
  const port = await freePort()
  const base = `http://127.0.0.1:${port}`
  server = spawn(binary, [], { cwd: root, env: { ...process.env, BIND_ADDR: `127.0.0.1:${port}`, DATABASE_URL: `sqlite://${join(directory, 'shared.db')}` }, stdio: 'inherit' })
  let launchError
  server.once('error', error => { launchError = error })
  let ready = false
  for (let i = 0; i < 100; i++) {
    if (launchError) throw launchError
    if (server.exitCode !== null) throw new Error('Acceptance API exited before readiness')
    try { ready = (await fetch(`${base}/api/health`, { signal: AbortSignal.timeout(500) })).ok } catch { /* startup */ }
    if (ready) break
    await delay(100)
  }
  if (!ready) throw new Error('Acceptance API did not become ready')
  const child = spawn(probe, [base], { cwd: root, stdio: 'inherit' })
  const timeout = setTimeout(() => child.kill('SIGKILL'), 120_000)
  try { await done(child) } finally { clearTimeout(timeout) }
} finally {
  await stop()
  await rm(directory, { recursive: true, force: true })
}
