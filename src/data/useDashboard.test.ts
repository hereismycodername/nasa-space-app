import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { getAssessment, getLatestMeasurements, getTasks } from './measurements'
import { startDashboardPolling } from './useDashboard'
import type { DashboardState } from './useDashboard'
import type { DailyTask, HealthAssessment, LatestMeasurements } from '../shared/types'
import { healthFixture } from '../features/dashboard/health.fixture'

vi.mock('./measurements', () => ({
  getLatestMeasurements: vi.fn(), getAssessment: vi.fn(), getTasks: vi.fn(),
}))

const latest: LatestMeasurements = { astronautId: 'demo-astronaut', evaluatedAt: '2026-11-14T08:00:00Z', metrics: [] }
const assessment: HealthAssessment = { status: 'no-data', explanation: 'Нет измерений', astronautId: 'demo-astronaut', evaluatedAt: latest.evaluatedAt, metrics: [] }
const tasks: DailyTask[] = [{ id: 'daily-check', title: 'Заполнить измерения', completedAt: null }]
let page: EventTarget & { hidden: boolean }
let lifecycle: ReturnType<typeof startDashboardPolling> | undefined

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise
    reject = rejectPromise
  })
  return { promise, resolve, reject }
}

function successfulReads() {
  vi.mocked(getLatestMeasurements).mockResolvedValue(latest)
  vi.mocked(getAssessment).mockResolvedValue(assessment)
  vi.mocked(getTasks).mockResolvedValue(tasks)
}

function pendingUntilAborted<T>(signal?: AbortSignal): Promise<T> {
  return new Promise((_resolve, reject) => {
    signal?.addEventListener('abort', () => reject(signal.reason), { once: true })
  })
}

beforeEach(() => {
  vi.useFakeTimers()
  vi.resetAllMocks()
  page = Object.assign(new EventTarget(), { hidden: false })
  vi.stubGlobal('document', page)
  successfulReads()
})

afterEach(() => {
  lifecycle?.dispose()
  lifecycle = undefined
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

it('starts reads in parallel, publishes one complete snapshot and polls only after completion', async () => {
  const latestRequest = deferred<LatestMeasurements>()
  const assessmentRequest = deferred<HealthAssessment>()
  const tasksRequest = deferred<DailyTask[]>()
  vi.mocked(getLatestMeasurements).mockReturnValueOnce(latestRequest.promise)
  vi.mocked(getAssessment).mockReturnValueOnce(assessmentRequest.promise)
  vi.mocked(getTasks).mockReturnValueOnce(tasksRequest.promise)
  const onChange = vi.fn<(state: DashboardState) => void>()
  const requestStartedAt = performance.now()
  lifecycle = startDashboardPolling(onChange)

  expect(getLatestMeasurements).toHaveBeenCalledTimes(1)
  expect(getAssessment).toHaveBeenCalledTimes(1)
  expect(getTasks).toHaveBeenCalledTimes(1)
  await vi.advanceTimersByTimeAsync(10000)
  expect(getLatestMeasurements).toHaveBeenCalledTimes(1)
  latestRequest.resolve(latest)
  await vi.advanceTimersByTimeAsync(0)
  expect(onChange.mock.lastCall?.[0].latest).toBeUndefined()
  assessmentRequest.resolve(assessment)
  tasksRequest.resolve(tasks)
  await vi.advanceTimersByTimeAsync(0)
  expect(onChange.mock.lastCall?.[0]).toEqual({ latest, assessment, tasks, connected: true, loading: false, error: '', lastUpdated: new Date(), qualityObservedAt: requestStartedAt, qualityReceivedAt: performance.now(), assessmentObservedAt: requestStartedAt, assessmentReceivedAt: performance.now(), previousHealthAlerts: {} })
  // Slow requests must not reset the age of the server's freshness decision on arrival.
  expect(performance.now() - onChange.mock.lastCall![0].qualityObservedAt!).toBe(10000)

  await vi.advanceTimersByTimeAsync(4999)
  expect(getLatestMeasurements).toHaveBeenCalledTimes(1)
  await vi.advanceTimersByTimeAsync(1)
  expect(getLatestMeasurements).toHaveBeenCalledTimes(2)
  expect(onChange.mock.lastCall![0].qualityObservedAt).toBe(requestStartedAt)
  expect(onChange.mock.lastCall![0].qualityReceivedAt).toBe(performance.now())
  expect(onChange.mock.lastCall![0].assessmentObservedAt).toBe(requestStartedAt)
})

it('ages assessments independently when latest advances and an older cached assessment returns', async () => {
  const onChange = vi.fn<(state: DashboardState) => void>()
  lifecycle = startDashboardPolling(onChange)
  await vi.advanceTimersByTimeAsync(0)
  const original = onChange.mock.lastCall![0]
  vi.mocked(getLatestMeasurements).mockResolvedValueOnce({ ...latest, evaluatedAt: '2026-11-14T08:00:05Z' })
  await vi.advanceTimersByTimeAsync(5000)
  const updated = onChange.mock.lastCall![0]
  expect(updated.qualityObservedAt).toBeGreaterThan(original.qualityObservedAt!)
  expect(updated.assessmentObservedAt).toBe(original.assessmentObservedAt)
  vi.mocked(getAssessment).mockResolvedValueOnce({ ...assessment, evaluatedAt: '2026-11-14T07:59:55Z' })
  await vi.advanceTimersByTimeAsync(5000)
  expect(onChange.mock.lastCall![0].assessmentObservedAt).toBe(original.assessmentObservedAt! - 5000)
})

it('retains an expired alert as historical until a later evaluated result replaces it', async () => {
  const fixture = healthFixture()
  fixture.assessment.health!.rules[0].status = 'critical'
  vi.mocked(getAssessment).mockResolvedValue(fixture.assessment)
  const onChange = vi.fn<(state: DashboardState) => void>()
  lifecycle = startDashboardPolling(onChange)
  await vi.advanceTimersByTimeAsync(0)
  const expired = structuredClone(fixture.assessment)
  Object.assign(expired.health!.rules[0], { status: 'insufficientData', measurementIds: [], validUntil: null })
  expired.evaluatedAt = '2026-11-15T08:00:20Z'
  vi.mocked(getAssessment).mockResolvedValue(expired)
  await vi.advanceTimersByTimeAsync(5000)
  expect(onChange.mock.lastCall![0].previousHealthAlerts?.heartRate).toMatchObject({ rule: { status: 'critical' }, evaluatedAt: fixture.assessment.evaluatedAt })
  const recovered = structuredClone(fixture.assessment)
  recovered.health!.rules[0].status = 'noAlert'
  recovered.evaluatedAt = '2026-11-15T08:00:25Z'
  vi.mocked(getAssessment).mockResolvedValue(recovered)
  await vi.advanceTimersByTimeAsync(5000)
  expect(onChange.mock.lastCall![0].previousHealthAlerts?.heartRate).toBeUndefined()
})

it('keeps the last complete snapshot on failure and refreshes immediately after a mutation', async () => {
  const onChange = vi.fn<(state: DashboardState) => void>()
  lifecycle = startDashboardPolling(onChange)
  await vi.advanceTimersByTimeAsync(0)
  const saved = onChange.mock.lastCall![0]
  vi.mocked(getTasks).mockRejectedValueOnce(new Error('Database operation failed'))
  await vi.advanceTimersByTimeAsync(5000)
  const failed = onChange.mock.lastCall![0]
  expect(failed).toMatchObject({ latest, assessment, tasks, connected: false, loading: false, lastUpdated: saved.lastUpdated, qualityObservedAt: saved.qualityObservedAt })
  expect(failed.latest).toBe(saved.latest)
  expect(failed.error).toContain('Database operation failed')

  lifecycle.refresh()
  expect(getLatestMeasurements).toHaveBeenCalledTimes(3)
  await vi.advanceTimersByTimeAsync(0)
  expect(onChange.mock.lastCall![0]).toMatchObject({ connected: true, loading: false, error: '' })
})

it('aborts pending work and removes polling and visibility updates on disposal', async () => {
  vi.mocked(getLatestMeasurements).mockImplementation(pendingUntilAborted)
  vi.mocked(getAssessment).mockImplementation(pendingUntilAborted)
  vi.mocked(getTasks).mockImplementation(pendingUntilAborted)
  const onChange = vi.fn<(state: DashboardState) => void>()
  lifecycle = startDashboardPolling(onChange)
  const signal = vi.mocked(getLatestMeasurements).mock.calls[0][0]
  const changeCount = onChange.mock.calls.length
  lifecycle.dispose()
  expect(signal?.aborted).toBe(true)
  await vi.advanceTimersByTimeAsync(60000)
  page.dispatchEvent(new Event('visibilitychange'))
  expect(getLatestMeasurements).toHaveBeenCalledTimes(1)
  expect(onChange).toHaveBeenCalledTimes(changeCount)
})

it('pauses hidden tabs, cancels in-flight reads and immediately resumes without an abort error', async () => {
  vi.mocked(getLatestMeasurements).mockImplementationOnce(pendingUntilAborted)
  vi.mocked(getAssessment).mockImplementationOnce(pendingUntilAborted)
  vi.mocked(getTasks).mockImplementationOnce(pendingUntilAborted)
  const onChange = vi.fn<(state: DashboardState) => void>()
  lifecycle = startDashboardPolling(onChange)
  page.hidden = true
  page.dispatchEvent(new Event('visibilitychange'))
  expect(vi.mocked(getLatestMeasurements).mock.calls[0][0]?.aborted).toBe(true)
  await vi.advanceTimersByTimeAsync(60000)
  lifecycle.refresh()
  expect(getLatestMeasurements).toHaveBeenCalledTimes(1)
  expect(onChange.mock.lastCall![0]).toMatchObject({ loading: false, error: '' })

  page.hidden = false
  page.dispatchEvent(new Event('visibilitychange'))
  expect(getLatestMeasurements).toHaveBeenCalledTimes(2)
  await vi.advanceTimersByTimeAsync(0)
  expect(onChange.mock.lastCall![0]).toMatchObject({ latest, connected: true, loading: false, error: '' })
})

it('coalesces repeated refreshes during a request and waits for cancellation before replacing it', async () => {
  vi.mocked(getLatestMeasurements).mockImplementationOnce(pendingUntilAborted)
  vi.mocked(getAssessment).mockImplementationOnce(pendingUntilAborted)
  vi.mocked(getTasks).mockImplementationOnce(pendingUntilAborted)
  const onChange = vi.fn<(state: DashboardState) => void>()
  lifecycle = startDashboardPolling(onChange)
  lifecycle.refresh()
  lifecycle.refresh()
  lifecycle.refresh()
  expect(getLatestMeasurements).toHaveBeenCalledTimes(1)
  expect(vi.mocked(getLatestMeasurements).mock.calls[0][0]?.aborted).toBe(true)
  await vi.advanceTimersByTimeAsync(0)
  expect(getLatestMeasurements).toHaveBeenCalledTimes(2)
  expect(onChange.mock.lastCall![0]).toMatchObject({ connected: true, error: '' })
  expect(onChange.mock.calls.filter(([state]) => state.connected)).toHaveLength(1)
})
