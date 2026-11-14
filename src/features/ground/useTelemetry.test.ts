import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { getMeasurementSeries } from '../../data/measurements'
import { buildSeriesChart } from '../../data/seriesChart'
import type { MeasurementSeries, SeriesFilters } from '../../shared/types'
import { startTelemetryPolling, telemetryWindow } from './useTelemetry'
import type { TelemetryState } from './useTelemetry'

vi.mock('../../data/measurements', () => ({ getMeasurementSeries: vi.fn() }))
let page: EventTarget & { hidden: boolean }
let dispose: (() => void) | undefined
const series = (filters: SeriesFilters): MeasurementSeries => ({
  astronautId: 'demo-astronaut', metric: filters.metric, unit: filters.metric === 'sleepHours' ? 'h' : 'bpm', aggregation: 'mean', from: filters.from, to: filters.to, intervalMs: 900000, timeBasis: 'measuredAt', points: [],
})
beforeEach(() => {
  vi.useFakeTimers()
  vi.resetAllMocks()
  vi.setSystemTime(new Date('2026-11-14T12:00:00Z'))
  page = Object.assign(new EventTarget(), { hidden: false })
  vi.stubGlobal('document', page)
  vi.mocked(getMeasurementSeries).mockImplementation(async filters => series(filters))
})
afterEach(() => { dispose?.(); dispose = undefined; vi.useRealTimers(); vi.unstubAllGlobals() })

it('uses one UTC window for every channel and retains the same axis when their first samples differ', async () => {
  const onChange = vi.fn<(state: TelemetryState) => void>()
  dispose = startTelemetryPolling(['heartRate', 'sleepHours', 'heartRate'], 7, onChange)
  await vi.advanceTimersByTimeAsync(0)
  expect(getMeasurementSeries).toHaveBeenCalledTimes(2)
  const [pulse, sleep] = vi.mocked(getMeasurementSeries).mock.calls.map(call => call[0])
  expect(pulse).toMatchObject(telemetryWindow(7))
  expect(sleep).toMatchObject(telemetryWindow(7))
  const snapshot = onChange.mock.lastCall![0]
  expect(Object.keys(snapshot.series)).toHaveLength(2)
  const makePoint = (startAt: string) => ({ startAt, endAt: startAt, value: 70, min: 70, max: 70, count: 1, measuredCount: 1, receivedCount: 0, timeBasis: 'measuredAt' as const })
  const early = buildSeriesChart({ ...snapshot.series.heartRate!, points: [makePoint('2026-11-08T00:00:00Z')] })
  const late = buildSeriesChart({ ...snapshot.series.sleepHours!, points: [makePoint('2026-11-13T00:00:00Z')] })
  expect(early.domain).toEqual(late.domain)
  expect(early.ticks).toEqual(late.ticks)
  expect(early.domain).toEqual([Date.parse(pulse.from), Date.parse(pulse.to)])
})

it('publishes an entire batch together and does not overlap polls while a channel is slow', async () => {
  let finish!: (value: MeasurementSeries) => void
  vi.mocked(getMeasurementSeries).mockImplementationOnce(filters => new Promise(resolve => { finish = () => resolve(series(filters)) }))
  const onChange = vi.fn<(state: TelemetryState) => void>()
  dispose = startTelemetryPolling(['heartRate', 'sleepHours'], 1, onChange)
  await vi.advanceTimersByTimeAsync(20000)
  expect(getMeasurementSeries).toHaveBeenCalledTimes(2)
  expect(onChange.mock.lastCall![0].window).toBeUndefined()
  finish(series({ metric: 'heartRate', ...telemetryWindow(1) }))
  await vi.advanceTimersByTimeAsync(0)
  expect(Object.keys(onChange.mock.lastCall![0].series)).toHaveLength(2)
  await vi.advanceTimersByTimeAsync(15000)
  expect(getMeasurementSeries).toHaveBeenCalledTimes(4)
})

it('pauses hidden pages, resumes one batch, and disposes listeners and timers', async () => {
  const onChange = vi.fn()
  dispose = startTelemetryPolling(['heartRate'], 1, onChange)
  await vi.advanceTimersByTimeAsync(0)
  page.hidden = true
  page.dispatchEvent(new Event('visibilitychange'))
  await vi.advanceTimersByTimeAsync(60000)
  expect(getMeasurementSeries).toHaveBeenCalledTimes(1)
  page.hidden = false
  page.dispatchEvent(new Event('visibilitychange'))
  await vi.advanceTimersByTimeAsync(0)
  expect(getMeasurementSeries).toHaveBeenCalledTimes(2)
  dispose()
  page.dispatchEvent(new Event('visibilitychange'))
  await vi.advanceTimersByTimeAsync(60000)
  expect(getMeasurementSeries).toHaveBeenCalledTimes(2)
})

it('retains the last complete history on failure without claiming a successful refresh', async () => {
  const onChange = vi.fn<(state: TelemetryState) => void>()
  dispose = startTelemetryPolling(['heartRate'], 30, onChange)
  await vi.advanceTimersByTimeAsync(0)
  const saved = onChange.mock.lastCall![0]
  vi.mocked(getMeasurementSeries).mockRejectedValueOnce(new Error('offline'))
  await vi.advanceTimersByTimeAsync(15000)
  expect(onChange.mock.lastCall![0]).toMatchObject({ series: saved.series, window: saved.window, loading: false })
  expect(onChange.mock.lastCall![0].error).toContain('Не удалось')
})

it('does not request data for an empty selection', async () => {
  dispose = startTelemetryPolling([], 1, vi.fn())
  await vi.advanceTimersByTimeAsync(60000)
  expect(getMeasurementSeries).not.toHaveBeenCalled()
})
