import { useMemo, useSyncExternalStore } from 'react'
import type { LatestMeasurements } from '../../shared/types'
import { nextFreshnessExpiry } from './freshness'

export function createFreshnessClock(latest?: LatestMeasurements, observedAt?: number, receivedAt?: number) {
  // The request-start anchor includes transport time; client calendar changes cannot refresh data.
  let elapsed = observedAt === undefined ? 0 : Math.max(0, (receivedAt ?? observedAt) - observedAt) / 1000
  return {
    getSnapshot: () => elapsed,
    subscribe(onChange: () => void) {
      if (!latest || observedAt === undefined) return () => {}
      let timer: ReturnType<typeof setTimeout> | undefined
      function update() {
        clearTimeout(timer)
        if (document.hidden || !latest || observedAt === undefined) return
        const nextElapsed = Math.max(elapsed, (performance.now() - observedAt) / 1000)
        if (nextElapsed !== elapsed) { elapsed = nextElapsed; onChange() }
        const delay = nextFreshnessExpiry(latest, elapsed)
        if (delay !== undefined) timer = setTimeout(update, delay)
      }
      document.addEventListener('visibilitychange', update)
      update()
      return () => { clearTimeout(timer); document.removeEventListener('visibilitychange', update) }
    },
  }
}

export function useFreshnessElapsed(latest?: LatestMeasurements, observedAt?: number, receivedAt?: number) {
  const clock = useMemo(() => createFreshnessClock(latest, observedAt, receivedAt), [latest, observedAt, receivedAt])
  return useSyncExternalStore(clock.subscribe, clock.getSnapshot, clock.getSnapshot)
}
