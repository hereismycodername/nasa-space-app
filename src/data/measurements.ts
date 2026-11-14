import type { DailyTask, HealthAssessment, HistoryFilters, LatestMeasurements, Measurement, MeasurementHistory, MeasurementSeries, NewMeasurement, SeriesFilters } from '../shared/types'
import { parseAssessment } from './assessmentSchema'

/** HTTP failure with the server's stable error code and optional input field. */
export class ApiError extends Error {
  readonly status: number
  readonly code?: string
  readonly field?: string

  constructor(status: number, message: string, code?: string, field?: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.code = code
    this.field = field
  }
}

async function readApiError(response: Response): Promise<ApiError> {
  const text = await response.text()
  let payload: unknown
  try {
    payload = JSON.parse(text)
  } catch {
    // Proxies or older servers may return plain text instead of problem JSON.
  }

  const fallback = text.trim() || `API ${response.status}: ${response.statusText || 'Request failed'}`
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) {
    return new ApiError(response.status, fallback)
  }

  const problem = payload as Record<string, unknown>
  const message = [problem.message, problem.detail, problem.error]
    .find((value): value is string => typeof value === 'string' && value.length > 0)
  return new ApiError(
    response.status,
    message ?? fallback,
    typeof problem.code === 'string' ? problem.code : undefined,
    typeof problem.field === 'string' ? problem.field : undefined,
  )
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const timeout = AbortSignal.timeout(5000)
  const signal = init?.signal ? AbortSignal.any([init.signal, timeout]) : timeout
  const response = await fetch(`/api${path}`, { ...init, signal })
  if (!response.ok) throw await readApiError(response)
  return response.json() as Promise<T>
}

export function getHistory(filters: HistoryFilters = {}, signal?: AbortSignal) {
  const query = new URLSearchParams()
  for (const key of ['metric', 'from', 'to', 'limit', 'cursor'] as const) {
    const value = filters[key]
    if (value !== undefined) query.set(key, String(value))
  }
  return request<MeasurementHistory>(`/measurements${query.size ? `?${query}` : ''}`, { signal })
}
export const getLatestMeasurements = (signal?: AbortSignal) => request<LatestMeasurements>('/measurements/latest', { signal })
export function getMeasurementSeries(filters: SeriesFilters, signal?: AbortSignal) {
  const query = new URLSearchParams()
  for (const key of ['metric', 'from', 'to', 'maxPoints'] as const) {
    const value = filters[key]
    if (value !== undefined) query.set(key, String(value))
  }
  return request<MeasurementSeries>(`/measurements/series?${query}`, { signal })
}
export const getAssessment = async (signal?: AbortSignal): Promise<HealthAssessment> => parseAssessment(await request<unknown>('/assessments', { signal }))
export const getTasks = (signal?: AbortSignal) => request<DailyTask[]>('/tasks', { signal })
export const saveMeasurement = (measurement: NewMeasurement) => request<Measurement>('/measurements', {
  method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(measurement),
})
export const updateTask = (id: string, completed: boolean) => request<DailyTask>(`/tasks/${encodeURIComponent(id)}`, {
  method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ completed }),
})
