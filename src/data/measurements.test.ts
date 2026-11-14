import { afterEach, expect, it, vi } from 'vitest'
import { ApiError, getAssessment, getHistory, getLatestMeasurements, getMeasurementSeries, getTasks, saveMeasurement, updateTask } from './measurements'
import type { Measurement, NewMeasurement } from '../shared/types'

const input: NewMeasurement = {
  deviceId: 'manual-01', sessionId: 'test', sequence: 0,
  metric: 'heartRate', value: 70, unit: 'bpm',
}

afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks() })

it('reads an empty history page from the relative server API', async () => {
  const page = { items: [], hasMore: false, nextCursor: null }
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(page), { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
  expect(await getHistory()).toEqual(page)
  expect(fetchMock.mock.calls[0][0]).toBe('/api/measurements')
})

it('encodes time offsets and the opaque continuation cursor without losing pagination fields', async () => {
  const record: Measurement = {
    ...input, id: 'history-id', astronautId: 'demo-astronaut', source: 'manual',
    measuredAt: null, receivedAt: '2026-11-14T08:00:00Z', timeBasis: 'receivedAt',
  }
  const page = { items: [record], hasMore: true, nextCursor: 'next:opaque/+=' }
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(page), { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
  const filters = {
    metric: 'heartRate' as const,
    from: '2026-11-13T13:00:00.123+05:00',
    to: '2026-11-14T13:00:00+05:00',
    limit: 60,
    cursor: 'current:opaque/+&=',
  }
  expect(await getHistory(filters)).toEqual(page)
  const url = new URL(fetchMock.mock.calls[0][0], 'http://localhost')
  expect(url.pathname).toBe('/api/measurements')
  expect(Object.fromEntries(url.searchParams)).toEqual({ ...filters, limit: '60' })
})

it('keeps known measurements, unknown-time receipts and missing metrics distinct', async () => {
  const known = {
    ...input, id: 'known', astronautId: 'demo-astronaut', source: 'manual',
    measuredAt: '2026-11-13T08:00:00Z', receivedAt: '2026-11-13T08:00:01Z', timeBasis: 'measuredAt',
  }
  const unknown = { ...known, id: 'unknown', measuredAt: null, receivedAt: '2026-11-14T08:00:00Z', timeBasis: 'receivedAt' }
  const overview = {
    astronautId: 'demo-astronaut',
    metrics: [
      { metric: 'heartRate', latestMeasurement: unknown, latestKnownMeasurement: known, latestReceivedMeasurement: unknown },
      { metric: 'sleepHours', latestMeasurement: null, latestKnownMeasurement: null, latestReceivedMeasurement: null },
    ],
  }
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(overview), { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
  expect(await getLatestMeasurements()).toEqual(overview)
  expect(fetchMock.mock.calls[0][0]).toBe('/api/measurements/latest')
})

it('encodes the full series period and point budget while preserving missing buckets and provenance', async () => {
  const filters = {
    metric: 'heartRate' as const,
    from: '2026-11-07T13:00:00.123+05:00',
    to: '2026-11-14T13:00:00.123+05:00',
    maxPoints: 168,
  }
  const series = {
    astronautId: 'demo-astronaut', metric: 'heartRate', unit: 'bpm', aggregation: 'mean',
    from: '2026-11-07T08:00:00.123Z', to: '2026-11-14T08:00:00.123Z', intervalMs: 3_600_000,
    timeBasis: 'receivedAt', points: [
      { startAt: '2026-11-07T08:00:00.123Z', endAt: '2026-11-07T09:00:00.123Z', value: null, min: null, max: null, count: 0, measuredCount: 0, receivedCount: 0, timeBasis: null },
      { startAt: '2026-11-07T09:00:00.123Z', endAt: '2026-11-07T10:00:00.123Z', value: 70, min: 70, max: 70, count: 1, measuredCount: 0, receivedCount: 1, timeBasis: 'receivedAt' },
    ],
  }
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(series), { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
  expect(await getMeasurementSeries(filters)).toEqual(series)
  const url = new URL(fetchMock.mock.calls[0][0], 'http://localhost')
  expect(url.pathname).toBe('/api/measurements/series')
  expect(Object.fromEntries(url.searchParams)).toEqual({ ...filters, maxPoints: '168' })
})

it('leaves the series point budget to the server default when omitted', async () => {
  const fetchMock = vi.fn().mockResolvedValue(new Response('{}', { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
  await getMeasurementSeries({ metric: 'sleepHours', from: '2026-11-13T00:00:00Z', to: '2026-11-14T00:00:00Z' })
  const url = new URL(fetchMock.mock.calls[0][0], 'http://localhost')
  expect(url.searchParams.has('maxPoints')).toBe(false)
  expect(url.searchParams.get('metric')).toBe('sleepHours')
})

it('surfaces a latest-values API failure instead of returning an empty overview', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
    code: 'DATABASE_ERROR', message: 'Database operation failed',
  }), { status: 500 })))
  await expect(getLatestMeasurements()).rejects.toMatchObject({ status: 500, code: 'DATABASE_ERROR' })
})

it('preserves the caller’s message identity when saving', async () => {
  const saved = { ...input, id: 'saved-id', astronautId: 'demo-astronaut', source: 'manual', receivedAt: '2026-11-14T08:00:00Z', measuredAt: null, timeBasis: 'receivedAt' }
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(saved), { status: 201 }))
  vi.stubGlobal('fetch', fetchMock)
  expect(await saveMeasurement(input)).toEqual(saved)
  expect(fetchMock).toHaveBeenCalledWith('/api/measurements', expect.objectContaining({
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input),
  }))
})

it('exposes problem details as a typed API error', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
    type: 'urn:space-health:error:validation-error', title: 'Bad Request', status: 400,
    detail: 'Invalid unit or value', code: 'VALIDATION_ERROR',
    message: 'Invalid unit or value', field: 'unit', error: 'Invalid unit or value',
  }), { status: 400, headers: { 'Content-Type': 'application/problem+json' } })))

  const result = saveMeasurement({ ...input, unit: 'h' })
  await expect(result).rejects.toBeInstanceOf(ApiError)
  await expect(result).rejects.toMatchObject({
    name: 'ApiError', status: 400, code: 'VALIDATION_ERROR', field: 'unit', message: 'Invalid unit or value',
  })
})

it('uses the HTTP status rather than trusting a mismatching problem status', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
    status: 200, detail: 'Service temporarily unavailable',
  }), { status: 503 })))
  await expect(getHistory()).rejects.toMatchObject({ status: 503, message: 'Service temporarily unavailable' })
})

it('supports the previous JSON error response', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: 'Unknown deviceId' }), { status: 400 })))
  await expect(saveMeasurement(input)).rejects.toMatchObject({
    status: 400, message: 'Unknown deviceId', code: undefined, field: undefined,
  })
})

it('does not silently accept an unsuccessful plain-text save', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('unknown device', { status: 400 })))
  await expect(saveMeasurement(input)).rejects.toMatchObject({ status: 400, message: 'unknown device' })
})

it('provides a useful fallback when the error body is empty', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('', { status: 502, statusText: 'Bad Gateway' })))
  await expect(getHistory()).rejects.toThrow('API 502: Bad Gateway')
})

it('preserves network failures instead of turning them into HTTP errors', async () => {
  const networkError = new TypeError('Failed to fetch')
  vi.stubGlobal('fetch', vi.fn().mockRejectedValue(networkError))
  await expect(getHistory()).rejects.toBe(networkError)
})

it('keeps the task update payload and encodes the task ID', async () => {
  const task = { id: 'daily/check', title: 'Check', completedAt: null }
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(task), { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
  expect(await updateTask(task.id, false)).toEqual(task)
  expect(fetchMock).toHaveBeenCalledWith('/api/tasks/daily%2Fcheck', expect.objectContaining({
    method: 'PATCH', body: '{"completed":false}',
  }))
})

it('propagates caller cancellation through every read without dropping the deadline', async () => {
  const caller = new AbortController()
  const timeout = new AbortController()
  const deadline = vi.spyOn(AbortSignal, 'timeout').mockReturnValue(timeout.signal)
  const signals: AbortSignal[] = []
  vi.stubGlobal('fetch', vi.fn((_url: string, init: RequestInit) => {
    const signal = init.signal as AbortSignal
    signals.push(signal)
    return new Promise<Response>((_resolve, reject) => {
      signal.addEventListener('abort', () => reject(signal.reason), { once: true })
    })
  }))
  const reads = [
    getHistory({}, caller.signal), getLatestMeasurements(caller.signal), getAssessment(caller.signal), getTasks(caller.signal),
    getMeasurementSeries({ metric: 'heartRate', from: '2026-11-07T00:00:00Z', to: '2026-11-14T00:00:00Z' }, caller.signal),
  ]
  const result = Promise.allSettled(reads)
  const reason = new DOMException('View closed', 'AbortError')
  caller.abort(reason)
  expect(await result).toEqual(reads.map(() => ({ status: 'rejected', reason })))
  expect(signals.every(signal => signal.aborted)).toBe(true)
  expect(deadline).toHaveBeenCalledTimes(5)
  expect(deadline).toHaveBeenCalledWith(5000)
})

it('still times out a read when its caller has not aborted', async () => {
  const caller = new AbortController()
  const timeout = new AbortController()
  vi.spyOn(AbortSignal, 'timeout').mockReturnValue(timeout.signal)
  vi.stubGlobal('fetch', vi.fn((_url: string, init: RequestInit) => new Promise<Response>((_resolve, reject) => {
    const signal = init.signal as AbortSignal
    signal.addEventListener('abort', () => reject(signal.reason), { once: true })
  })))
  const request = getTasks(caller.signal)
  const result = expect(request).rejects.toMatchObject({ name: 'TimeoutError' })
  timeout.abort(new DOMException('Request timed out', 'TimeoutError'))
  await result
  expect(caller.signal.aborted).toBe(false)
})
