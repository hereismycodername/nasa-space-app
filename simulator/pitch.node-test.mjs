import assert from 'node:assert/strict'
import { test } from 'node:test'
import { runPitch, deliverMeasurement } from './pitch.mjs'
import { PITCH_METRICS, PITCH_STAGES, pitchConfig, stageDuration, stageReadings } from './pitch-scenario.mjs'

const known = () => new Response(JSON.stringify({ metrics: PITCH_METRICS.map(({ metric }) => ({ metric })) }))
const accepted = payload => new Response(JSON.stringify({ ...payload, id: `id-${payload.sequence}`, source: 'simulator' }), { status: 201 })

test('pitch configuration is strict and preserves the current API origin', () => {
  const config = pitchConfig(['--api-url', 'http://localhost:8082/api/', '--cycles', '0', '--manual'], {})
  assert.equal(config.base, 'http://localhost:8082')
  assert.equal(config.stageSeconds, 30)
  assert.equal(config.interval, 2000)
  assert.equal(config.cycles, 0)
  assert.equal(config.manual, true)
  for (const args of [
    ['--unknown'], ['extra'], ['--stage-seconds', '0'], ['--stage-seconds', '1.5'],
    ['--interval-ms', '99'], ['--cycles', '-1'], ['--cycles', '1', '--cycles', '2'],
    ['--stage-seconds', '1', '--interval-ms', '2000'],
    ['--api-url', 'http://user:pass@host'],
  ]) assert.throws(() => pitchConfig(args, {}))
  assert.throws(() => pitchConfig([], { SIM_REQUEST_TIMEOUT_MS: '0' }))
  assert.equal(stageDuration('baseline', 1), 1)
  assert.equal(stageDuration('stale', 1), 30)
  assert.equal(pitchConfig(['--stage-seconds', '1', '--interval-ms', '500'], {}).stageSeconds, 1)
})

test('pitch stages stay within ingestion bounds and interpolate sensor data only', () => {
  const units = Object.fromEntries(PITCH_METRICS.map(row => [row.metric, row.unit]))
  const bounded = { sleepHours: [0, 24], mood: [1, 5], spo2: [0, 100], visionBlur: [0, 10], exerciseMinutes: [0, 1440], headache: [0, 10], backPain: [0, 10] }
  for (const stage of PITCH_STAGES) for (const elapsed of [0, 1, 4, 8, 29, 120]) {
    const frame = stageReadings(stage.id, elapsed, 'baseline')
    assert.equal(frame.length, stage.id === 'stale' ? 0 : 12)
    for (const { metric, value, unit } of frame) {
      assert.equal(unit, units[metric])
      assert.ok(Number.isFinite(value))
      if (bounded[metric]) assert.ok(value >= bounded[metric][0] && value <= bounded[metric][1])
      else assert.ok(value > 0)
    }
  }
  const first = stageReadings('critical', 0, 'baseline')
  const plateau = stageReadings('critical', 8, 'baseline')
  assert.ok(first.find(row => row.metric === 'systolicPressure').value < 120)
  assert.ok(plateau.find(row => row.metric === 'systolicPressure').value > 180)
  assert.equal(first.find(row => row.metric === 'headache').value, 6)
  assert.equal(stageReadings('recovery', 10, 'critical').find(row => row.metric === 'mood').value, 4)
  assert.throws(() => stageReadings('unknown', 0))
  assert.throws(() => stageReadings('baseline', NaN))
})

test('one complete accelerated run publishes phase snapshots, live sensor frames, and a real 30-second stale interval', async () => {
  const config = pitchConfig(['--stage-seconds', '2', '--interval-ms', '1000'], {})
  let clock = 0
  const rows = []
  const logs = []
  const result = await runPitch(config, {
    subscribeControls: false, now: () => clock, wallNow: () => new Date(1_700_000_000_000 + clock).toISOString(),
    log: value => logs.push({ clock, value }), wait: async ms => { clock += ms },
    fetch: async (_url, options) => {
      if (!options.method) return known()
      const row = JSON.parse(options.body)
      rows.push({ clock, ...row })
      return accepted(row)
    },
  })
  assert.equal(result.completedCycles, 1)
  const stages = logs.filter(row => row.value.startsWith('[pitch] stage='))
  assert.deepEqual(stages.map(row => row.value.match(/stage=(\w+)/)[1]), PITCH_STAGES.map(row => row.id))
  const staleAt = stages.at(-1).clock
  assert.ok(clock - staleAt >= 30_000)
  assert.ok(rows.every(row => row.clock < staleAt), 'stale must not post even a synthetic empty/zero sample')
  for (const { metric, sensor } of PITCH_METRICS) assert.equal(rows.filter(row => row.metric === metric).length, sensor ? 8 : 4)
  assert.deepEqual(rows.map(row => row.sequence), rows.map((_, i) => i))
  assert.equal(new Set(rows.map(row => row.sessionId)).size, 1)
  assert.ok(rows[0].sessionId.startsWith('pitch-'))
  for (const row of rows) assert.equal(Date.parse(row.measuredAt), 1_700_000_000_000 + row.clock)
  assert.equal(result.confirmed, rows.length)
})

test('manual scenes and pause control automatic transitions without stopping the current stream', async () => {
  const config = pitchConfig(['--manual', '--stage-seconds', '2', '--interval-ms', '1000'], {})
  let clock = 0
  let command
  const stages = []
  const rows = []
  const result = await runPitch(config, {
    subscribeControls: callback => { command = callback; return () => { command = undefined } },
    now: () => clock, log: value => { if (value.startsWith('[pitch] stage=')) stages.push(value.match(/stage=(\w+)/)[1]) },
    wait: async ms => {
      clock += ms
      if (clock === 3000) command('3')
      if (clock === 5000) command('5')
      if (clock === 8000) command('q')
    },
    fetch: async (_url, options) => {
      if (!options.method) return known()
      const row = JSON.parse(options.body); rows.push({ clock, ...row }); return accepted(row)
    },
  })
  assert.deepEqual(stages, ['baseline', 'critical', 'stale'])
  assert.ok(rows.some(row => row.clock === 2000), 'paused transitions still publish sensor samples')
  assert.ok(rows.every(row => row.clock < 5000))
  assert.equal(result.completedCycles, 0)
  assert.equal(command, undefined, 'input subscription cleaned up')
})

test('a slow confirmation cannot skip the critical scene before its values reach the API', async () => {
  let clock = 0
  let slow = false
  let scene
  const criticalPressure = []
  await runPitch(pitchConfig(['--stage-seconds', '3', '--interval-ms', '1000'], {}), {
    subscribeControls: false, now: () => clock, wait: async ms => { clock += ms },
    log: line => { if (line.startsWith('[pitch] stage=')) scene = line.match(/stage=(\w+)/)[1] },
    fetch: async (_url, options) => {
      if (!options.method) return known()
      const row = JSON.parse(options.body)
      if (scene === 'critical' && !slow) { clock += 60_000; slow = true }
      if (scene === 'critical' && row.metric === 'systolicPressure') criticalPressure.push(row.value)
      return accepted(row)
    },
  })
  assert.ok(criticalPressure.some(value => value > 180), JSON.stringify(criticalPressure))
  assert.equal(criticalPressure.length, 3)
})

test('the shortest accepted scene still publishes the target critical and recovery values', async () => {
  let clock = 0
  let scene
  const pressures = { critical: [], recovery: [] }
  await runPitch(pitchConfig(['--stage-seconds', '1', '--interval-ms', '500'], {}), {
    subscribeControls: false, now: () => clock, wait: async ms => { clock += ms },
    log: line => { if (line.startsWith('[pitch] stage=')) scene = line.match(/stage=(\w+)/)[1] },
    fetch: async (_url, options) => {
      if (!options.method) return known()
      const row = JSON.parse(options.body)
      if (row.metric === 'systolicPressure' && pressures[scene]) pressures[scene].push(row.value)
      return accepted(row)
    },
  })
  assert.ok(pressures.critical.some(value => value > 180), JSON.stringify(pressures))
  assert.ok(pressures.recovery.some(value => value < 120), JSON.stringify(pressures))
})

test('an overslept timer cannot advance a scene until its plateau was acknowledged', async () => {
  let clock = 0
  let scene
  const pressures = { critical: [], recovery: [] }
  await runPitch(pitchConfig(['--stage-seconds', '1', '--interval-ms', '500'], {}), {
    subscribeControls: false, now: () => clock, wait: async () => { clock += 3000 },
    log: line => { if (line.startsWith('[pitch] stage=')) scene = line.match(/stage=(\w+)/)[1] },
    fetch: async (_url, options) => {
      if (!options.method) return known()
      const row = JSON.parse(options.body)
      if (row.metric === 'systolicPressure' && pressures[scene]) pressures[scene].push(row.value)
      return accepted(row)
    },
  })
  assert.ok(pressures.critical.some(value => value > 180), JSON.stringify(pressures))
  assert.ok(pressures.recovery.some(value => value < 120), JSON.stringify(pressures))
})

test('a lost acknowledgement retries the identical original payload before sending later samples', async () => {
  const payload = { deviceId: 'simulator-01', sessionId: 'pitch-test', sequence: 8, metric: 'heartRate', unit: 'bpm', value: 72, measuredAt: '2026-11-14T21:00:00Z' }
  const bodies = []
  const result = await deliverMeasurement(payload, {
    base: 'http://localhost:8082', retry: 100, timeout: 500, signal: new AbortController().signal,
    log: () => {}, wait: async () => {},
    fetch: async (_url, options) => {
      bodies.push(options.body)
      if (bodies.length === 1) throw new TypeError('response lost after server commit')
      return new Response(JSON.stringify({ ...payload, id: 'persisted-id', source: 'simulator' }), { status: 200 })
    },
  })
  assert.equal(result, true)
  assert.equal(bodies.length, 2)
  assert.equal(bodies[0], bodies[1])
})

test('legacy capabilities, permanent API rejection and mismatched acknowledgements stop without false success', async () => {
  let posts = 0
  await assert.rejects(runPitch(pitchConfig([], {}), {
    subscribeControls: false, log: () => {}, fetch: async (_url, options) => {
      if (options.method) posts++
      return new Response(JSON.stringify({ metrics: PITCH_METRICS.slice(0, 6) }))
    },
  }), /12 метриками/)
  assert.equal(posts, 0)
  const payload = { metric: 'heartRate', value: 72 }
  for (const response of [new Response('UNKNOWN_DEVICE', { status: 400 }), new Response('{}'), new Response('not json'), new Response('', { status: 202 })]) {
    let calls = 0
    await assert.rejects(deliverMeasurement(payload, {
      base: 'http://localhost:8082', retry: 100, timeout: 500, signal: new AbortController().signal,
      log: () => {}, wait: async () => { throw Error('must not retry') },
      fetch: async () => { calls++; return response },
    }))
    assert.equal(calls, 1)
  }
})

test('abort cancels a pending request, retry sleep and scene interval without reporting unconfirmed writes', async () => {
  for (const phase of ['request', 'retry', 'interval']) {
    const shutdown = new AbortController()
    let posted = 0
    let scheduled = false
    const scheduleStop = () => { if (!scheduled) { scheduled = true; setTimeout(() => shutdown.abort(), 10) } }
    const wait = async (_ms, signal) => {
      scheduleStop()
      await new Promise((_, reject) => {
        if (signal.aborted) reject(new DOMException('Stopped', 'AbortError'))
        else signal.addEventListener('abort', () => reject(new DOMException('Stopped', 'AbortError')), { once: true })
      })
    }
    const result = await runPitch(pitchConfig([], {}), {
      signal: shutdown.signal, subscribeControls: false, log: () => {}, wait,
      fetch: async (_url, options) => {
        if (!options.method) return known()
        posted++
        if (phase === 'request') {
          scheduleStop()
          await new Promise((_, reject) => options.signal.addEventListener('abort', () => reject(new DOMException('Stopped', 'AbortError')), { once: true }))
        }
        if (phase === 'retry') return new Response('', { status: 503 })
        return accepted(JSON.parse(options.body))
      },
    })
    assert.equal(result.confirmed, phase === 'interval' ? 12 : 0)
    assert.equal(posted, phase === 'interval' ? 12 : 1)
  }
})
