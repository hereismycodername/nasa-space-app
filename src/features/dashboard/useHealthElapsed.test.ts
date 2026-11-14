import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createHealthClock } from './useHealthElapsed'
import { metricHealth } from './health'
import { healthFixture } from './health.fixture'

let page: EventTarget & { hidden: boolean }
let unsubscribe: (() => void) | undefined
beforeEach(() => {
  vi.useFakeTimers()
  page = Object.assign(new EventTarget(), { hidden: false })
  vi.stubGlobal('document', page)
})
afterEach(() => { unsubscribe?.(); unsubscribe = undefined; vi.useRealTimers(); vi.unstubAllGlobals() })

it('expires at validUntil while a request is pending, accounting for transport time', async () => {
  const context = healthFixture()
  context.assessment.health!.rules[0].validUntil = '2026-11-15T08:00:03Z'
  const started = performance.now()
  await vi.advanceTimersByTimeAsync(2000)
  const clock = createHealthClock(context.assessment, started, performance.now())
  const onChange = vi.fn()
  unsubscribe = clock.subscribe(onChange)
  expect(metricHealth('heartRate', { ...context, elapsedSeconds: clock.getSnapshot() }).current).toBe(true)
  await vi.advanceTimersByTimeAsync(1001)
  expect(metricHealth('heartRate', { ...context, elapsedSeconds: clock.getSnapshot() }).current).toBe(false)
  expect(onChange).toHaveBeenCalledTimes(1)
})

it('withdraws cached results immediately on returning from a hidden tab and stops after final expiry', async () => {
  const context = healthFixture()
  const clock = createHealthClock(context.assessment, performance.now(), performance.now())
  const onChange = vi.fn()
  unsubscribe = clock.subscribe(onChange)
  page.hidden = true
  page.dispatchEvent(new Event('visibilitychange'))
  await vi.advanceTimersByTimeAsync(60000)
  expect(onChange).not.toHaveBeenCalled()
  page.hidden = false
  page.dispatchEvent(new Event('visibilitychange'))
  expect(metricHealth('heartRate', { ...context, elapsedSeconds: clock.getSnapshot() }).current).toBe(false)
  expect(onChange).toHaveBeenCalledTimes(1)
  await vi.advanceTimersByTimeAsync(60000)
  expect(onChange).toHaveBeenCalledTimes(1)
})
