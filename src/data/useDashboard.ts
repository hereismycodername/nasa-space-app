import { useCallback, useEffect, useRef, useState } from 'react'
import type { DailyTask, HealthAssessment, LatestMeasurements } from '../shared/types'
import { getAssessment, getLatestMeasurements, getTasks } from './measurements'

export interface DashboardState {
  latest?: LatestMeasurements
  assessment?: HealthAssessment
  tasks?: DailyTask[]
  connected: boolean
  loading: boolean
  error: string
  lastUpdated?: Date
}

const initialState: DashboardState = { connected: false, loading: true, error: '' }
const pollInterval = 5000

function describeError(error: unknown) {
  if (error instanceof Error && error.name === 'TimeoutError') return 'Сервер не ответил за 5 секунд.'
  return error instanceof Error
    ? `Не удалось обновить данные: ${error.message}`
    : 'Не удалось обновить данные. Проверьте соединение с сервером.'
}

/** One cancellable polling lifecycle, shared by all dashboard consumers through the hook. */
export function startDashboardPolling(onChange: (state: DashboardState) => void) {
  let state = initialState
  let disposed = false
  let running = false
  let pendingRefresh = false
  let controller: AbortController | undefined
  let timer: ReturnType<typeof setTimeout> | undefined

  function publish(update: Partial<DashboardState>) {
    if (disposed) return
    state = { ...state, ...update }
    onChange(state)
  }

  function clearTimer() {
    clearTimeout(timer)
    timer = undefined
  }

  async function poll() {
    if (disposed || document.hidden || running) return
    clearTimer()
    running = true
    const requestController = new AbortController()
    controller = requestController
    publish({ loading: true })
    const requests = [
      getLatestMeasurements(requestController.signal),
      getAssessment(requestController.signal),
      getTasks(requestController.signal),
    ] as const

    try {
      const [latest, assessment, tasks] = await Promise.all(requests)
      if (!requestController.signal.aborted) {
        publish({ latest, assessment, tasks, connected: true, loading: false, error: '', lastUpdated: new Date() })
      }
    } catch (error) {
      const cancelled = requestController.signal.aborted
      // A failed request cancels its siblings, then waits for their cleanup before another cycle.
      requestController.abort()
      await Promise.allSettled(requests)
      if (!cancelled && !document.hidden) {
        publish({ connected: false, loading: false, error: describeError(error) })
      }
    } finally {
      running = false
      controller = undefined
      if (!disposed && !document.hidden) {
        if (pendingRefresh) {
          pendingRefresh = false
          void poll()
        } else {
          timer = setTimeout(() => void poll(), pollInterval)
        }
      }
    }
  }

  function refresh() {
    clearTimer()
    if (disposed || document.hidden) return
    if (running) {
      pendingRefresh = true
      controller?.abort()
    } else {
      void poll()
    }
  }

  function onVisibilityChange() {
    if (document.hidden) {
      clearTimer()
      pendingRefresh = false
      controller?.abort()
      publish({ loading: false })
    } else {
      refresh()
    }
  }

  document.addEventListener('visibilitychange', onVisibilityChange)
  if (document.hidden) publish({ loading: false })
  else void poll()

  return {
    refresh,
    dispose() {
      disposed = true
      clearTimer()
      controller?.abort()
      document.removeEventListener('visibilitychange', onVisibilityChange)
    },
  }
}

export function useDashboard() {
  const [state, setState] = useState<DashboardState>(initialState)
  const polling = useRef<ReturnType<typeof startDashboardPolling> | undefined>(undefined)
  useEffect(() => {
    const lifecycle = startDashboardPolling(setState)
    polling.current = lifecycle
    return () => {
      lifecycle.dispose()
      polling.current = undefined
    }
  }, [])
  const refresh = useCallback(() => polling.current?.refresh(), [])
  return { ...state, refresh }
}
