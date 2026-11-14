// Run after npm run build and cargo build --locked --bins.
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { once } from 'node:events'
import { mkdtemp, rm } from 'node:fs/promises'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { root } from './tooling.mjs'

const directory = await mkdtemp(join(tmpdir(), 'space-health-demo-test-'))
const reservation = createServer()
reservation.listen(0, '127.0.0.1')
await once(reservation, 'listening')
const port = reservation.address().port
await new Promise(resolve => reservation.close(resolve))
const base = `http://127.0.0.1:${port}`
let active
function launch(args, extra = {}) {
  active = spawn(process.execPath, ['scripts/run.mjs', '--no-build', ...args], {
    cwd: root, env: { ...process.env, BIND_ADDR: `127.0.0.1:${port}`, DATABASE_URL: `sqlite://${directory}/demo.db`, SIM_INTERVAL_MS: '100', SHUTDOWN_STDIN: '1', ...extra }, stdio: ['pipe', 'inherit', 'inherit'],
  })
  active.done = once(active, 'exit')
  return active
}
async function finish(child) {
  return Promise.race([child.done, delay(20_000, undefined, { ref: false }).then(() => { throw new Error('Demo process timed out') })])
}
try {
  const finite = launch([], { SIM_COUNT: '3' })
  assert.equal((await finish(finite))[0], 0)
  await assert.rejects(fetch(`${base}/api/health`, { signal: AbortSignal.timeout(500) }))
  const restarted = launch(['--no-simulator'])
  let ready = false
  for (let n = 0; n < 100; n++) {
    assert.equal(restarted.exitCode, null, 'Restart exited early')
    try { ready = (await fetch(`${base}/api/health`, { signal: AbortSignal.timeout(500) })).ok } catch { /* startup */ }
    if (ready) break
    await delay(100)
  }
  assert.ok(ready, 'Restart became ready')
  const history = await (await fetch(`${base}/api/measurements`)).json()
  assert.equal(history.items.length, 3)
  assert.ok(history.items.every(row => row.source === 'simulator'))
  assert.equal((await fetch(base)).status, 200)
  if (process.platform === 'win32') restarted.stdin.end('shutdown\n')
  else restarted.kill('SIGINT')
  assert.equal((await finish(restarted))[0], 0)
  await assert.rejects(fetch(`${base}/api/health`, { signal: AbortSignal.timeout(500) }))
  console.log('PASS: demo finite run, child cleanup, persisted samples, restart, UI and Ctrl+C cleanup')
} finally {
  if (active && active.exitCode === null && !active.signalCode) { active.stdin.end('shutdown\n'); await finish(active) }
  await rm(directory, { recursive: true, force: true })
}
