import type { DailyTask, HealthAssessment, HistoryParams, LatestMeasurementsResponse, Measurement, MeasurementPage, NewMeasurement } from '../shared/types'

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
  const response = await fetch(`/api${path}`, { ...init, signal: AbortSignal.timeout(5000) })
  if (!response.ok) throw await readApiError(response)
  return response.json() as Promise<T>
}

export const getHistory = (params: HistoryParams = {}) => {
  const query = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined) query.set(key, String(value))
  }
  const suffix = query.size ? `?${query}` : ''
  return request<MeasurementPage>(`/measurements${suffix}`)
}
export const getLatest = () => request<LatestMeasurementsResponse>('/measurements/latest')
export const getAssessment = () => request<HealthAssessment>('/assessments')
export const getTasks = () => request<DailyTask[]>('/tasks')
export const saveMeasurement = (measurement: NewMeasurement) => request<Measurement>('/measurements', {
  method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(measurement),
})
export const updateTask = (id: string, completed: boolean) => request<DailyTask>(`/tasks/${encodeURIComponent(id)}`, {
  method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ completed }),
})
