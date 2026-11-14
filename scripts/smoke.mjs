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
  assert.equal(history.hasMore, false)
  assert.equal(history.nextCursor, null)
  assert.ok(history.items.every(row => row.source === 'simulator'))
  const latest = await (await fetch(`${base}/api/measurements/latest`)).json()
  assert.equal(latest.metrics.length, 6)
  assert.equal(latest.metrics[0].latestKnownMeasurement.id, history.items[0].id)
  assert.ok(latest.metrics.slice(1).every(row => row.latestMeasurement === null))
  assert.ok(Number.isFinite(Date.parse(latest.evaluatedAt)))
  assert.equal(latest.metrics[0].quality.measurementId, history.items[0].id)
  assert.equal(latest.metrics[0].quality.status, 'fresh')
  assert.equal(latest.metrics[0].quality.freshForSeconds, 15)
  assert.ok(latest.metrics.slice(1).every(row => row.quality.status === 'missing'))
  const assessment = await (await fetch(`${base}/api/assessments`)).json()
  assert.equal(assessment.metrics.length, 6)
  assert.equal(assessment.metrics[0].quality.measurementId, latest.metrics[0].quality.measurementId)
  assert.equal(assessment.metrics[0].quality.status, latest.metrics[0].quality.status)
  const timestamps = history.items.map(row => Date.parse(row.measuredAt))
  const filters = new URLSearchParams({
    metric: 'heartRate',
    from: new Date(Math.min(...timestamps)).toISOString(),
    to: new Date(Math.max(...timestamps) + 1).toISOString(),
    maxPoints: '2',
  })
  const seriesResponse = await fetch(`${base}/api/measurements/series?${filters}`)
  assert.equal(seriesResponse.status, 200)
  const series = await seriesResponse.json()
  assert.equal(series.aggregation, 'mean')
  assert.equal(series.unit, 'bpm')
  assert.equal(series.timeBasis, 'measuredAt')
  assert.ok(series.points.length <= 2)
  assert.equal(series.points.reduce((count, point) => count + point.count, 0), 3)
  assert.equal(series.points[0].startAt, filters.get('from'))
  assert.equal(series.points.at(-1).endAt, filters.get('to'))
  assert.ok(series.points.every(point => point.count === 0
    ? point.value === null
    : Number.isFinite(point.value) && point.min <= point.value && point.value <= point.max))
  const page = await fetch(base)
  assert.equal(page.status, 200)
  assert.match(await page.text(), /Space Health/)
  await stop()
  await start()
  const persisted = await (await fetch(`${base}/api/measurements`)).json()
  assert.deepEqual(persisted, history)
  const persistedSeries = await (await fetch(`${base}/api/measurements/series?${filters}`)).json()
  assert.deepEqual(persistedSeries, series)
  const persistedLatest = await (await fetch(`${base}/api/measurements/latest`)).json()
  assert.equal(persistedLatest.metrics[0].quality.measurementId, latest.metrics[0].quality.measurementId)
  assert.ok(persistedLatest.metrics[0].quality.ageSeconds >= latest.metrics[0].quality.ageSeconds)
  console.log('PASS: simulator → API → SQLite → history/latest/series + per-metric quality; static UI; persistence after restart')
} finally {
  await stop()
  await rm(directory, { recursive: true, force: true })
}
