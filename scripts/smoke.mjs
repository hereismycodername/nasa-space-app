import { spawn } from 'node:child_process'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import assert from 'node:assert/strict'

const directory = await mkdtemp(join(tmpdir(), 'space-health-smoke-'))
const base = 'http://127.0.0.1:18080'
let server
async function stop() {
  if (server && server.exitCode === null) {
    await new Promise(resolve => { server.once('exit', resolve); server.kill('SIGINT') })
  }
}
async function start() {
  server = spawn('./target/debug/space-health-server', [], {
    env: { ...process.env, BIND_ADDR: '127.0.0.1:18080', DATABASE_URL: `sqlite://${directory}/smoke.db` },
    stdio: 'inherit',
  })
  server.on('error', error => console.error(error))
  for (let attempt = 0; attempt < 50; attempt++) {
    if (server.exitCode !== null) throw new Error('Server exited before becoming ready')
    try {
      if ((await fetch(`${base}/api/health`)).ok) return
    } catch { /* wait for listener */ }
    await delay(100)
  }
  throw new Error('Server did not become ready')
}
try {
  await start()
  await new Promise((resolve, reject) => {
    const simulator = spawn(process.execPath, ['simulator/sensors.mjs'], {
      env: { ...process.env, API_URL: base, SIM_COUNT: '3', SIM_INTERVAL_MS: '100' }, stdio: 'inherit',
    })
    const timeout = setTimeout(() => { simulator.kill(); reject(new Error('Simulator timed out')) }, 15000)
    simulator.on('error', error => { clearTimeout(timeout); reject(error) })
    simulator.on('exit', code => {
      clearTimeout(timeout)
      if (code === 0) resolve()
      else reject(new Error(`Simulator exited ${code}`))
    })
  })
  const history = await (await fetch(`${base}/api/measurements`)).json()
  assert.equal(history.items.length, 3)
  assert.ok(history.items.every(row => row.source === 'simulator'))
  const page = await fetch(base)
  assert.equal(page.status, 200)
  assert.match(await page.text(), /Space Health/)
  await stop()
  await start()
  const persisted = await (await fetch(`${base}/api/measurements`)).json()
  assert.deepEqual(persisted, history)
  console.log('PASS: simulator → API → SQLite → history; static UI; persistence after restart')
} finally {
  await stop()
  await rm(directory, { recursive: true, force: true })
}
