import { useMemo, useSyncExternalStore } from 'react'
import type { HealthAssessment } from '../../shared/types'
import { nextHealthExpiry } from './health'

export function createHealthClock(assessment?: HealthAssessment, observedAt?: number, receivedAt?: number) {
  let elapsed = observedAt === undefined ? 0 : Math.max(0, (receivedAt ?? observedAt) - observedAt) / 1000
  return {
    getSnapshot: () => elapsed,
    subscribe(onChange: () => void) {
      if (!assessment || observedAt === undefined) return () => {}
      let timer: ReturnType<typeof setTimeout> | undefined
      function update() {
        clearTimeout(timer)
        if (document.hidden || !assessment || observedAt === undefined) return
        const next = Math.max(elapsed, (performance.now() - observedAt) / 1000)
        if (next !== elapsed) { elapsed = next; onChange() }
        const delay = nextHealthExpiry(assessment, elapsed)
        if (delay !== undefined) timer = setTimeout(update, delay)
      }
      document.addEventListener('visibilitychange', update)
      update()
      return () => { clearTimeout(timer); document.removeEventListener('visibilitychange', update) }
    },
  }
}

export function useHealthElapsed(assessment?: HealthAssessment, observedAt?: number, receivedAt?: number) {
  const clock = useMemo(() => createHealthClock(assessment, observedAt, receivedAt), [assessment, observedAt, receivedAt])
  return useSyncExternalStore(clock.subscribe, clock.getSnapshot, clock.getSnapshot)
}
