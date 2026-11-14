import { describe, expect, it, vi } from 'vitest'
import { ApiError } from '../../data/measurements'
import type { Measurement, NewMeasurement } from '../../shared/types'
import { createMeasurementSaver, isUnconfirmedSave, localDateTime, parseCheckIn } from './submission'
import type { CheckInDraft, MeasurementInput } from './submission'

const draft: CheckInDraft = { metric: 'heartRate', value: '70', localTime: '2026-11-14T08:30:00', unknownTime: false }
const input: MeasurementInput = { metric: 'heartRate', value: 70, unit: 'bpm', measuredAt: '2026-11-14T03:30:00.000Z' }
const result = (payload: NewMeasurement): Measurement => ({
  ...payload, measuredAt: payload.measuredAt ?? null, id: 'saved', astronautId: 'demo-astronaut',
  receivedAt: '2026-11-14T03:31:00Z', timeBasis: payload.measuredAt ? 'measuredAt' : 'receivedAt', source: 'manual',
})

describe('check-in input', () => {
  it('uses the metric unit and converts local calendar time to its UTC instant', () => {
    expect(parseCheckIn({ ...draft, metric: 'temperature', value: '36,6' })).toEqual({
      metric: 'temperature', value: 36.6, unit: '°C', measuredAt: new Date(2026, 10, 14, 8, 30, 0).toISOString(),
    })
    expect(localDateTime(new Date(2026, 10, 14, 8, 30, 0))).toBe(draft.localTime)
  })

  it('keeps an explicitly unknown measurement time absent rather than inventing now', () => {
    expect(parseCheckIn({ ...draft, localTime: '', unknownTime: true }).measuredAt).toBeNull()
  })

  it.each(['', ' ', 'NaN', 'Infinity', '-Infinity', 'abc', '0', '-1'])('rejects an invalid continuous reading %s', value => {
    expect(() => parseCheckIn({ ...draft, value })).toThrow()
  })

  it('accepts valid endpoints for daily scales and rejects out-of-range values', () => {
    expect(parseCheckIn({ ...draft, metric: 'sleepHours', value: '0' }).value).toBe(0)
    expect(parseCheckIn({ ...draft, metric: 'sleepHours', value: '24' }).value).toBe(24)
    expect(parseCheckIn({ ...draft, metric: 'mood', value: '1' }).value).toBe(1)
    expect(parseCheckIn({ ...draft, metric: 'mood', value: '5' }).value).toBe(5)
    for (const [metric, value] of [['sleepHours', '24.1'], ['sleepHours', '-0.1'], ['mood', '0'], ['mood', '5.1']] as const) {
      expect(() => parseCheckIn({ ...draft, metric, value })).toThrow()
    }
  })

  it.each(['', 'invalid', '2026-02-31T08:30:00', '2026-11-14T25:00:00'])('rejects invalid or normalized local time %s', localTime => {
    expect(() => parseCheckIn({ ...draft, localTime })).toThrow('дату и время')
  })
})

describe('confirmed saves and safe retries', () => {
  it('reuses the exact original payload after a lost response', async () => {
    const save = vi.fn<(payload: NewMeasurement) => Promise<Measurement>>()
      .mockRejectedValueOnce(new TypeError('Lost reply'))
      .mockImplementationOnce(async payload => result(payload))
    const newSession = vi.fn(() => 'session-1')
    const submit = createMeasurementSaver(save, newSession)
    await expect(submit(input)).rejects.toThrow('Lost reply')
    const saved = await submit({ ...input })
    expect(save.mock.calls[1][0]).toBe(save.mock.calls[0][0])
    expect(saved.sessionId).toBe('session-1')
    expect(newSession).toHaveBeenCalledTimes(1)
  })

  it('uses a new key when the unconfirmed payload changes', async () => {
    const save = vi.fn<(payload: NewMeasurement) => Promise<Measurement>>()
      .mockRejectedValueOnce(new TypeError('Lost reply'))
      .mockImplementationOnce(async payload => result(payload))
    let sequence = 0
    const submit = createMeasurementSaver(save, () => `session-${++sequence}`)
    await expect(submit(input)).rejects.toThrow()
    const saved = await submit({ ...input, value: 71 })
    expect(saved.sessionId).toBe('session-2')
    expect(saved.value).toBe(71)
    expect(save.mock.calls[0][0].value).toBe(70)
  })

  it('does not send or confirm a second request while the first one is pending', async () => {
    let complete!: (value: Measurement) => void
    const save = vi.fn<(payload: NewMeasurement) => Promise<Measurement>>()
      .mockImplementation(() => new Promise(resolve => { complete = resolve }))
    const submit = createMeasurementSaver(save, () => 'pending-session')
    const first = submit(input)
    expect(submit({ ...input })).toBe(first)
    await expect(submit({ ...input, value: 99 })).rejects.toThrow('завершения')
    expect(save).toHaveBeenCalledTimes(1)
    complete(result(save.mock.calls[0][0]))
    expect((await first).value).toBe(70)
  })

  it('starts a new recording after a successful confirmed save', async () => {
    const save = vi.fn(async (payload: NewMeasurement) => result(payload))
    let sequence = 0
    const submit = createMeasurementSaver(save, () => `session-${++sequence}`)
    expect((await submit(input)).sessionId).toBe('session-1')
    expect((await submit(input)).sessionId).toBe('session-2')
  })

  it('distinguishes validation rejection from an uncertain transport or server failure', () => {
    expect(isUnconfirmedSave(new ApiError(400, 'value invalid'))).toBe(false)
    expect(isUnconfirmedSave(new ApiError(500, 'database unavailable'))).toBe(true)
    expect(isUnconfirmedSave(new TypeError('Failed to fetch'))).toBe(true)
  })
})
