// Send one or more synthetic measurements to the shared API, as the crew member's sensors would.
// For demos: provoke a rule result and watch it appear in the crew view, on the ground view and in React.
//
//   node scripts/send-measurement.mjs systolicPressure=195 diastolicPressure=125
//   API_URL=http://127.0.0.1:18080 node scripts/send-measurement.mjs temperature=34.6
//   node scripts/send-measurement.mjs --api http://127.0.0.1:18080 --device manual-01 heartRate=128
//
// Synthetic test data only; measuredAt is the current time, so the server evaluates it as fresh.

import { randomUUID } from 'node:crypto'

const UNITS = {
  heartRate: 'bpm', systolicPressure: 'mmHg', diastolicPressure: 'mmHg', temperature: '°C',
  sleepHours: 'h', mood: 'score', spo2: '%', glucose: 'mg/dL', visionBlur: 'score',
  exerciseMinutes: 'min', headache: 'score', backPain: 'score',
}
const DEVICES = ['simulator-01', 'manual-01']

function usage(message) {
  if (message) console.error(`Error: ${message}\n`)
  console.error('Usage: node scripts/send-measurement.mjs [--api URL] [--device simulator-01|manual-01] metric=value ...')
  console.error(`Metrics: ${Object.keys(UNITS).join(', ')}`)
  process.exit(message ? 2 : 0)
}

let api = process.env.API_URL ?? 'http://127.0.0.1:8080'
let device = 'simulator-01'
const pairs = []
const args = process.argv.slice(2)
for (let i = 0; i < args.length; i++) {
  const arg = args[i]
  if (arg === '--help' || arg === '-h') usage()
  else if (arg === '--api') api = args[++i] ?? usage('--api needs a URL')
  else if (arg === '--device') device = args[++i] ?? usage('--device needs an id')
  else {
    const [metric, raw] = arg.split('=')
    if (!(metric in UNITS)) usage(`unknown metric "${metric}"`)
    const value = Number(String(raw ?? '').replace(',', '.'))
    if (raw === undefined || raw.trim() === '' || !Number.isFinite(value)) usage(`"${arg}" needs a number, e.g. ${metric}=120`)
    pairs.push({ metric, value })
  }
}
if (!pairs.length) usage('give at least one metric=value')
if (!DEVICES.includes(device)) usage(`device must be one of ${DEVICES.join(', ')}`)
const base = new URL(api).origin

const sessionId = randomUUID()
let failed = false
for (const [index, { metric, value }] of pairs.entries()) {
  const body = {
    deviceId: device, sessionId, sequence: index + 1,
    metric, value, unit: UNITS[metric], measuredAt: new Date().toISOString(),
  }
  try {
    const response = await fetch(`${base}/api/measurements`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body),
      signal: AbortSignal.timeout(5000),
    })
    const text = await response.text()
    if (response.ok) console.log(`${response.status} saved  ${metric} = ${value} ${UNITS[metric]}  (${device})`)
    else { failed = true; console.error(`${response.status} rejected  ${metric} = ${value}: ${text}`) }
  } catch (error) {
    failed = true
    console.error(`cannot reach ${base}: ${error.message}`)
    break
  }
}
if (!failed) console.log(`Health guidance: ${base}/api/assessments`)
process.exit(failed ? 1 : 0)
