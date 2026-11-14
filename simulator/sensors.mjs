import { randomUUID } from 'node:crypto'
import { setTimeout as delay } from 'node:timers/promises'
import { simulatorConfig } from './config.mjs'

const shutdown = new AbortController()
const stop = () => shutdown.abort()
process.once('SIGINT', stop)
process.once('SIGTERM', stop)
if (process.env.SHUTDOWN_STDIN === '1') {
  process.stdin.on('data', stop)
  process.stdin.on('end', stop)
}
try {
  const { base, interval, count, retry, timeout } = simulatorConfig()
  const sessionId = randomUUID()
  console.log(`Synthetic sensor → ${base}/api/measurements; session ${sessionId}; Ctrl+C stops`)
  for (let sequence = 0; !shutdown.signal.aborted && (count === 0 || sequence < count); sequence++) {
    const body = {
      deviceId: 'simulator-01', sessionId, sequence,
      metric: 'heartRate', value: Math.round(72 + 8 * Math.sin(sequence / 5)),
      unit: 'bpm', measuredAt: new Date().toISOString(),
    }
    // Keep the same payload and sequence during retries: ingestion is idempotent.
    while (!shutdown.signal.aborted) {
      try {
        const response = await fetch(`${base}/api/measurements`, {
          method: 'POST', headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body), signal: AbortSignal.any([shutdown.signal, AbortSignal.timeout(timeout)]),
        })
        if (!response.ok) {
          if (response.status < 500 && response.status !== 429) {
            const detail = await response.text().catch(() => '(response body unavailable)')
            throw new Error(`API rejected measurement ${sequence}: HTTP ${response.status}: ${detail.slice(0, 1000)}`, { cause: 'fatal' })
          }
          await response.body?.cancel()
          throw new Error(`Server error ${response.status}`)
        }
        if (![200, 201].includes(response.status)) throw new Error(`Unexpected HTTP ${response.status}; measurement is unconfirmed. Check API_URL.`, { cause: 'fatal' })
        let saved
        try { saved = await response.json() } catch (error) {
          if (error instanceof SyntaxError) throw new Error('API returned invalid measurement JSON; sample is unconfirmed. Check API_URL.', { cause: 'fatal' })
          throw error
        }
        if (!saved || typeof saved !== 'object' || typeof saved.id !== 'string' || !saved.id
          || saved.source !== 'simulator' || Object.keys(body).some(key => saved[key] !== body[key])) {
          throw new Error('API did not confirm this simulator measurement; sample is unconfirmed. Check API_URL and device registration.', { cause: 'fatal' })
        }
        console.log(`${sequence}: ${body.value} bpm [simulator]`)
        break
      } catch (error) {
        if (shutdown.signal.aborted) break
        if (error.cause === 'fatal') throw error
        console.error(`Delivery ${sequence} unconfirmed; retrying the SAME payload in ${retry} ms: ${error.message}`)
        await delay(retry, undefined, { signal: shutdown.signal }).catch(error => { if (!shutdown.signal.aborted) throw error })
      }
    }
    if (!shutdown.signal.aborted && (count === 0 || sequence + 1 < count)) {
      await delay(interval, undefined, { signal: shutdown.signal }).catch(error => { if (!shutdown.signal.aborted) throw error })
    }
  }
} catch (error) {
  console.error(`Simulator: ${error.message}`)
  process.exitCode = 1
} finally {
  process.off('SIGINT', stop)
  process.off('SIGTERM', stop)
  process.stdin.pause()
  if (shutdown.signal.aborted) console.log('Simulator stopped. Unconfirmed sample was kept only in memory; use sensor_bridge.py for a durable outbox.')
}
