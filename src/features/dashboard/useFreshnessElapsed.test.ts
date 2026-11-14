import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { LatestMeasurements } from '../../shared/types'
import { currentFreshness } from './freshness'
import { createFreshnessClock } from './useFreshnessElapsed'

const quality = { status: 'fresh' as const, measurementId: 'pulse', ageSeconds: 14, freshForSeconds: 15, clockSkew: false }
const latest: LatestMeasurements = { astronautId: 'demo-astronaut', evaluatedAt: '2026-11-14T08:00:00Z', metrics: [
  { metric: 'heartRate', latestMeasurement: null, latestKnownMeasurement: null, latestReceivedMeasurement: null, quality, latestReceivedQuality: quality },
] }
let page: EventTarget & { hidden: boolean }
let unsubscribe: (() => void) | undefined

beforeEach(() => {
  vi.useFakeTimers()
  page = Object.assign(new EventTarget(), { hidden: false })
  vi.stubGlobal('document', page)
})
afterEach(() => {
  unsubscribe?.()
  unsubscribe = undefined
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

it('expires the displayed result without waiting for another API poll, then stops ticking', async () => {
  const started = performance.now()
  const clock = createFreshnessClock(latest, started, started)
  const onChange = vi.fn()
  unsubscribe = clock.subscribe(onChange)
  expect(currentFreshness(quality, clock.getSnapshot())).toBe('fresh')
  await vi.advanceTimersByTimeAsync(1001)
  expect(currentFreshness(quality, clock.getSnapshot())).toBe('stale')
  expect(onChange).toHaveBeenCalledTimes(1)
  await vi.advanceTimersByTimeAsync(60000)
  expect(onChange).toHaveBeenCalledTimes(1)
})

it('accounts for delayed responses before the first subscription or render', async () => {
  const started = performance.now()
  await vi.advanceTimersByTimeAsync(2000)
  const clock = createFreshnessClock(latest, started, performance.now())
  expect(currentFreshness(quality, clock.getSnapshot())).toBe('stale')
})

it('rechecks elapsed time on tab return and removes timers and listeners on cleanup', async () => {
  const started = performance.now()
  const clock = createFreshnessClock(latest, started, started)
  const onChange = vi.fn()
  unsubscribe = clock.subscribe(onChange)
  page.hidden = true
  page.dispatchEvent(new Event('visibilitychange'))
  await vi.advanceTimersByTimeAsync(60000)
  expect(onChange).not.toHaveBeenCalled()
  page.hidden = false
  page.dispatchEvent(new Event('visibilitychange'))
  expect(currentFreshness(quality, clock.getSnapshot())).toBe('stale')
  expect(onChange).toHaveBeenCalledTimes(1)
  unsubscribe()
  await vi.advanceTimersByTimeAsync(60000)
  page.dispatchEvent(new Event('visibilitychange'))
  expect(onChange).toHaveBeenCalledTimes(1)
})
