import { afterEach, expect, it, vi } from 'vitest'
import { ApiError, getHistory, saveMeasurement, updateTask } from './measurements'
import type { NewMeasurement } from '../shared/types'

const input: NewMeasurement = {
  deviceId: 'manual-01', sessionId: 'test', sequence: 0,
  metric: 'heartRate', value: 70, unit: 'bpm',
}

afterEach(() => vi.unstubAllGlobals())

it('reads history from the relative server API', async () => {
  const fetchMock = vi.fn().mockResolvedValue(new Response('[]', { status: 200 }))
  vi.stubGlobal('fetch', fetchMock)
  expect(await getHistory()).toEqual([])
  expect(fetchMock.mock.calls[0][0]).toBe('/api/measurements')
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
