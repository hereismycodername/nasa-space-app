import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { cargoPath, isWindows, root } from './tooling.mjs'

// Exercise the actual shared API without using the working database or port.
const directory = await mkdtemp(join(tmpdir(), 'space-health-assessment-'))
const examples = process.argv.includes('--write-examples')
let server
let stopped
let sequence = 0
let base
const sessionId = `be08-acceptance-${Date.now()}`
const units = { heartRate: 'bpm', systolicPressure: 'mmHg', diastolicPressure: 'mmHg', temperature: '°C', sleepHours: 'h', mood: 'score' }

async function build() {
  const child = spawn(cargoPath(), ['build', '--locked', '--bin', 'space-health-server', '--message-format=json'], { cwd: root, stdio: ['ignore', 'pipe', 'inherit'] })
  let output = ''
  child.stdout.on('data', chunk => { output += chunk })
  await new Promise((resolve, reject) => {
    child.once('error', reject)
    child.once('exit', code => code === 0 ? resolve() : reject(new Error(`Cargo exited ${code}`)))
  })
  for (const line of output.split('\n')) {
    try {
      const message = JSON.parse(line)
      if (message.reason === 'compiler-artifact' && message.target?.name === 'space-health-server' && message.executable) return message.executable
    } catch { /* Cargo progress is not a compiler artifact. */ }
  }
  throw new Error('Cargo did not report a server binary')
}

async function start(binary) {
  const reservation = createServer()
  await new Promise((resolve, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', resolve) })
  const { port } = reservation.address()
  await new Promise(resolve => reservation.close(resolve))
  base = `http://127.0.0.1:${port}`
  server = spawn(binary, [], {
    cwd: root,
    env: { ...process.env, DATABASE_URL: `sqlite://${join(directory, 'health.db')}`, BIND_ADDR: `127.0.0.1:${port}`, SHUTDOWN_STDIN: isWindows ? '1' : '0' },
    stdio: [isWindows ? 'pipe' : 'ignore', 'pipe', 'inherit'],
  })
  server.stdin?.on('error', () => {})
  let output = ''
  let launchError
  server.stdout.on('data', chunk => { output = `${output}${chunk}`.slice(-8192) })
  server.once('error', error => { launchError = error })
  stopped = new Promise(resolve => server.once('exit', resolve))
  for (let n = 0; n < 100; n++) {
    if (launchError) throw launchError
    if (server.exitCode !== null || server.signalCode) throw new Error(`Acceptance server stopped: ${output}`)
    if (output.includes(`Space Health: ${base}\n`)) {
      const response = await fetch(`${base}/api/health`, { signal: AbortSignal.timeout(1000) })
      await response.body?.cancel()
      if (response.ok) return
    }
    await delay(50)
  }
  throw new Error('Acceptance server did not become ready')
}

async function stop() {
  if (!server?.pid || server.exitCode !== null || server.signalCode) return
  if (isWindows) server.stdin.end('shutdown\n')
  else server.kill('SIGTERM')
  const timer = setTimeout(() => server.kill('SIGKILL'), 5000)
  try { await stopped } finally { clearTimeout(timer) }
}

async function send(metric, value, measuredAt = new Date().toISOString(), payload) {
  payload ??= { deviceId: 'manual-01', sessionId, sequence: sequence++, metric, value, unit: units[metric], measuredAt }
  const response = await fetch(`${base}/api/measurements`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload), signal: AbortSignal.timeout(5000),
  })
  const saved = await response.json()
  assert.ok([200, 201].includes(response.status), JSON.stringify(saved))
  return { payload, saved, status: response.status }
}

async function assessment() {
  const response = await fetch(`${base}/api/assessments`, { signal: AbortSignal.timeout(5000) })
  assert.equal(response.status, 200)
  const value = await response.json()
  assert.equal(value.health.ruleSetVersion, 'be08-demo-1')
  assert.equal(value.health.demonstrationOnly, true)
  assert.equal(value.health.rules.length, value.metrics.length)
  assert.equal(value.health.education.length, 5)
  assert.ok(value.health.emergencyGuidance.length > 0)
  for (const rule of value.health.rules) {
    assert.ok(rule.flightContext && rule.suggestedAction && rule.explanation)
    assert.ok(rule.sourceUrls.every(url => url.startsWith('https://')))
    if (rule.status !== 'notEvaluated') assert.ok(rule.sourceUrls.length, 'configured rules must link their sources')
    const quality = value.metrics.find(row => row.metric === rule.metric).quality
    if (['noAlert', 'attention', 'critical'].includes(rule.status)) {
      assert.equal(quality.status, 'fresh')
      assert.deepEqual(rule.measurementIds, [quality.measurementId])
      assert.ok(Date.parse(rule.validUntil) >= Date.parse(value.evaluatedAt))
    } else {
      assert.deepEqual(rule.measurementIds, [])
      assert.equal(rule.validUntil, null)
    }
  }
  return value
}

async function example(name, value) {
  if (examples) await writeFile(join(root, 'docs/examples', `assessment.rules-${name}.response.json`), `${JSON.stringify(value, null, 2)}\n`)
}

try {
  await start(await build())
  let value = await assessment()
  assert.equal(value.health.status, 'insufficientData')
  assert.equal(value.health.evaluatedMetricCount, 0)
  await send('heartRate', 200, new Date(Date.now() - 3600_000).toISOString())
  await send('sleepHours', 3, null)
  value = await assessment()
  assert.equal(value.health.status, 'insufficientData', 'stale and unknown-time records cannot establish a current alarm or normal state')
  const healthy = { heartRate: 72, systolicPressure: 118, diastolicPressure: 76, temperature: 36.7, sleepHours: 7.5, mood: 4 }
  for (const [metric, number] of Object.entries(healthy)) await send(metric, number)
  value = await assessment()
  const uncovered = value.health.rules.filter(rule => !Object.hasOwn(units, rule.metric))
  assert.ok(uncovered.every(rule => rule.status === 'notEvaluated'), 'new metrics without configured rules stay neutral')
  assert.equal(value.health.status, uncovered.length ? 'insufficientData' : 'noAlert')
  assert.ok(value.health.rules.filter(rule => Object.hasOwn(units, rule.metric)).every(rule => rule.status === 'noAlert'))
  assert.equal(value.health.evaluatedMetricCount, 6)
  await example('normal', value)

  await send('sleepHours', 4.5)
  await send('mood', 2)
  const pulse = await send('heartRate', 110)
  value = await assessment()
  assert.equal(value.health.status, 'attention')
  assert.equal(value.health.rules.find(rule => rule.metric === 'sleepHours').status, 'attention')
  await example('attention', value)

  const severe = await send('systolicPressure', 190)
  value = await assessment()
  assert.equal(value.health.status, 'critical')
  const pressure = value.health.rules.find(rule => rule.metric === 'systolicPressure')
  assert.equal(pressure.escalation, 'urgent')
  assert.deepEqual(pressure.measurementIds, [severe.saved.id])
  await example('critical', value)
  const retry = await send(null, null, null, severe.payload)
  assert.equal(retry.status, 200)
  assert.deepEqual(retry.saved, severe.saved)
  await send('heartRate', 70, null)
  value = await assessment()
  assert.equal(value.health.status, 'critical')
  assert.deepEqual(value.health.rules.find(rule => rule.metric === 'heartRate').measurementIds, [pulse.saved.id], 'new unknown-time receipt must not replace known evidence')

  for (const [metric, number] of Object.entries(healthy)) await send(metric, number)
  await send('temperature', 34, new Date(Date.now() + 120_000).toISOString())
  value = await assessment()
  assert.equal(value.health.status, 'insufficientData')
  assert.equal(value.health.rules.find(rule => rule.metric === 'temperature').status, 'insufficientData', 'future-time hypothermia sample is not a current diagnosis')
  console.log('PASS: BE-08 empty/stale/unknown/future guards, normal → attention → critical, evidence, flight context, actions, escalation and idempotent replay.')
} finally {
  await stop()
  await rm(directory, { recursive: true, force: true })
}
