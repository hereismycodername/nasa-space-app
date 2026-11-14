import { randomUUID } from 'node:crypto'
import { setTimeout as delay } from 'node:timers/promises'

const base = process.env.API_URL ?? 'http://127.0.0.1:8080'
const interval = Number(process.env.SIM_INTERVAL_MS ?? 2000)
const count = Number(process.env.SIM_COUNT ?? 0)
if (!Number.isInteger(interval) || interval < 100 || !Number.isInteger(count) || count < 0) {
  throw new Error('SIM_INTERVAL_MS must be an integer >= 100; SIM_COUNT must be >= 0')
}
const sessionId = randomUUID()
let running = true
process.on('SIGINT', () => { running = false })
process.on('SIGTERM', () => { running = false })
console.log(`Synthetic sensor → ${base}; session ${sessionId}`)
for (let sequence = 0; running && (count === 0 || sequence < count); sequence++) {
  const body = {
    deviceId: 'simulator-01', sessionId, sequence,
    metric: 'heartRate', value: Math.round(72 + 8 * Math.sin(sequence / 5)),
    unit: 'bpm', measuredAt: new Date().toISOString(),
  }
  // Keep the same payload and sequence during retries: ingestion is idempotent.
  while (running) {
    try {
      const response = await fetch(`${base}/api/measurements`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body), signal: AbortSignal.timeout(5000),
      })
      if (!response.ok) {
        if (response.status < 500) throw new Error(`Fatal API error ${response.status}: ${await response.text()}`, { cause: 'fatal' })
        throw new Error(`Server error ${response.status}`)
      }
      console.log(`${sequence}: ${body.value} bpm [simulator]`)
      break
    } catch (error) {
      if (error.cause === 'fatal') throw error
      console.error(`Connection unavailable, retrying: ${error.message}`)
      if (running) await delay(interval)
    }
  }
  if (running && (count === 0 || sequence + 1 < count)) await delay(interval)
}
