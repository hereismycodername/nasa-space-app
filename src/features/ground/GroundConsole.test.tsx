import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'
import GroundConsole, { ObservationLog } from './GroundConsole'
import type { LatestMeasurements } from '../../shared/types'

const latest: LatestMeasurements = { astronautId: 'demo-astronaut', evaluatedAt: '2026-11-14T12:00:00Z', metrics: [] }

it('offers all six channels, the shared range controls and an honest current observation summary', () => {
  const html = renderToStaticMarkup(<GroundConsole latest={latest} connected elapsedSeconds={0} refreshVersion={0} selectedSystem={null} onSystemSelect={() => {}} onOpenMetric={() => {}} channels={['heartRate', 'temperature', 'sleepHours']} onChannelsChange={() => {}} days={1} onDaysChange={() => {}} />)
  expect(html.match(/type="checkbox"/g)).toHaveLength(6)
  expect(html).toContain('24 часа')
  expect(html).toContain('7 дней')
  expect(html).toContain('30 дней')
  expect(html).toContain('Общая временная шкала · UTC')
  expect(html).toContain('история событий не ведётся')
  expect(html).not.toContain('Acknowledge')
  expect(html).not.toContain('Uplink')
})

it('keeps unknown delivery quality separate and suppresses freshness claims offline', () => {
  const sample = { id: 'known', astronautId: 'demo-astronaut', deviceId: 'manual-01', sessionId: 'test', sequence: 0, source: 'manual' as const, metric: 'heartRate' as const, value: 72, unit: 'bpm' as const, measuredAt: '2026-11-14T12:00:00Z', receivedAt: '2026-11-14T12:00:01Z', timeBasis: 'measuredAt' as const }
  const unknown = { ...sample, id: 'unknown', value: 80, measuredAt: null, timeBasis: 'receivedAt' as const }
  const snapshot: LatestMeasurements = { ...latest, metrics: [{ metric: 'heartRate', latestKnownMeasurement: sample, latestReceivedMeasurement: unknown, latestMeasurement: unknown,
    quality: { status: 'fresh', measurementId: sample.id, freshForSeconds: 15, ageSeconds: 0, clockSkew: false },
    latestReceivedQuality: { status: 'unknownTime', measurementId: unknown.id, freshForSeconds: 15, ageSeconds: null, clockSkew: false } }] }
  const online = renderToStaticMarkup(<ObservationLog latest={snapshot} connected elapsedSeconds={1} />)
  expect(online).toContain('72 bpm')
  expect(online).toContain('Последняя доставка: 80 bpm')
  expect(online).toContain('quality-unknownTime')
  const offline = renderToStaticMarkup(<ObservationLog latest={snapshot} connected={false} elapsedSeconds={1} />)
  expect(offline).not.toContain('quality-fresh')
  expect(offline).toContain('Свежесть не подтверждена')
})
