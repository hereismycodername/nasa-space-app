import { useEffect, useRef, useState } from 'react'
import { getMeasurementSeries } from '../../data/measurements'
import type { MeasurementSeries, Metric } from '../../shared/types'

export type TelemetryRange = 1 | 7 | 30
export interface TelemetryWindow { from: string; to: string; maxPoints: number }
export interface TelemetryState {
  series: Partial<Record<Metric, MeasurementSeries>>
  window?: TelemetryWindow
  loading: boolean
  error: string
}
const empty: TelemetryState = { series: {}, loading: true, error: '' }

export function telemetryWindow(days: TelemetryRange, now = Date.now()): TelemetryWindow {
  return { from: new Date(now - days * 86_400_000).toISOString(), to: new Date(now).toISOString(), maxPoints: days === 1 ? 96 : 168 }
}

/** One bounded, cancellable batch gives every channel exactly the same time domain. */
export function startTelemetryPolling(metrics: readonly Metric[], days: TelemetryRange, onChange: (state: TelemetryState) => void, previous: TelemetryState = empty) {
  let state = previous.window && Date.parse(previous.window.to) - Date.parse(previous.window.from) === days * 86_400_000 ? previous : empty
  let disposed = false
  let running = false
  let pending = false
  let controller: AbortController | undefined
  let timer: ReturnType<typeof setTimeout> | undefined
  const channels = [...new Set(metrics)]
  function publish(update: Partial<TelemetryState>) { if (!disposed) { state = { ...state, ...update }; onChange(state) } }
  async function load() {
    if (disposed || running || document.hidden || !channels.length) return
    clearTimeout(timer)
    running = true
    const request = new AbortController()
    controller = request
    const window = telemetryWindow(days)
    publish({ loading: true })
    const reads = channels.map(metric => getMeasurementSeries({ metric, ...window }, request.signal))
    try {
      const results = await Promise.all(reads)
      if (!request.signal.aborted) publish({ window, series: Object.fromEntries(results.map(series => [series.metric, series])), loading: false, error: '' })
    } catch {
      const cancelled = request.signal.aborted
      request.abort()
      await Promise.allSettled(reads)
      if (!cancelled) publish({ loading: false, error: 'Не удалось обновить телеметрию. Проверьте связь с сервером.' })
    } finally {
      running = false
      controller = undefined
      if (!disposed && !document.hidden) {
        if (pending) { pending = false; void load() }
        else timer = setTimeout(() => void load(), 15_000)
      }
    }
  }
  function visibility() {
    clearTimeout(timer)
    if (document.hidden) { pending = false; controller?.abort(); publish({ loading: false }) }
    else if (running) pending = true
    else void load()
  }
  document.addEventListener('visibilitychange', visibility)
  if (!channels.length) publish({ series: {}, loading: false, error: '' })
  else void load()
  return () => { disposed = true; clearTimeout(timer); controller?.abort(); document.removeEventListener('visibilitychange', visibility) }
}

export function useTelemetry(metrics: readonly Metric[], days: TelemetryRange, refreshVersion: number) {
  const [state, setState] = useState<TelemetryState>(empty)
  const snapshot = useRef(state)
  const channels = [...new Set(metrics)].sort().join(',')
  useEffect(() => startTelemetryPolling(channels ? channels.split(',') as Metric[] : [], days, next => {
    snapshot.current = next
    setState(next)
  }, snapshot.current), [channels, days, refreshVersion])
  // Keep the prior complete response only for the same requested period; never relabel old data.
  const matches = state.window && Date.parse(state.window.to) - Date.parse(state.window.from) === days * 86_400_000
  return matches ? state : { ...state, series: {}, window: undefined }
}
