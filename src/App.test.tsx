import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'
import { MetricOverview } from './App'
import type { DataQuality, LatestMeasurements, LatestMetric, Measurement } from './shared/types'

const empty: LatestMeasurements = { astronautId: 'demo-astronaut', evaluatedAt: '2026-11-14T08:00:00Z', metrics: [] }
const fresh: DataQuality = { status: 'fresh', measurementId: 'known-sleep', freshForSeconds: 86400, ageSeconds: 86400, clockSkew: false }

function snapshot(measurement: NonNullable<LatestMetric['latestKnownMeasurement']>, quality: DataQuality, received: Measurement = measurement, receivedQuality = quality): LatestMeasurements {
  return { ...empty, metrics: [{ metric: measurement.metric, latestMeasurement: received, latestKnownMeasurement: measurement, latestReceivedMeasurement: received, quality, latestReceivedQuality: receivedQuality }] }
}

const known: NonNullable<LatestMetric['latestKnownMeasurement']> = {
  id: 'known-sleep', astronautId: 'demo-astronaut', deviceId: 'manual-01',
  sessionId: 'check-in', sequence: 0, source: 'manual',
  metric: 'sleepHours', value: 7.5, unit: 'h',
  measuredAt: '2026-11-13T08:00:00Z', receivedAt: '2026-11-13T08:00:01Z', timeBasis: 'measuredAt',
}

it('shows a rare known measurement separately from a newer receipt without measurement time', () => {
  const unknown: Measurement = {
    ...known, id: 'unknown-sleep', value: 6, measuredAt: null,
    receivedAt: '2026-11-14T08:00:00Z', timeBasis: 'receivedAt',
  }
  const latest = snapshot(known, fresh, unknown, { ...fresh, status: 'unknownTime', measurementId: unknown.id, ageSeconds: null })
  const html = renderToStaticMarkup(<MetricOverview latest={latest} connected />)
  expect(html.replace(/<[^>]+>/g, '')).toContain('7,5 h')
  expect(html).toContain('6 h')
  expect(html).toContain('Время измерения неизвестно; свежесть не подтверждена.')
  expect(html).toContain('quality-fresh')
  expect(html).toContain('quality-unknownTime')
  expect(html.match(/<article/g)).toHaveLength(6)
})

it('filters the overview to the selected body system without inventing unavailable metrics', () => {
  const heart = renderToStaticMarkup(<MetricOverview selectedSystem="heart" latest={empty} />)
  expect(heart.match(/<article/g)).toHaveLength(3)
  expect(heart).toContain('Пульс')
  expect(heart).not.toContain('Самочувствие')
  const lungs = renderToStaticMarkup(<MetricOverview selectedSystem="lungs" />)
  expect(lungs).toContain('Показатели пока не подключены')
  expect(lungs).not.toContain('<article')
})

it('shows missing values explicitly for all metrics when the latest response is empty', () => {
  const html = renderToStaticMarkup(<MetricOverview latest={empty} />)
  expect(html.match(/Нет данных/g)).toHaveLength(6)
  expect(html).not.toContain('Измерено')
})

it('keeps per-metric freshness independent and expires a cached result between server polls', () => {
  const pulse: typeof known = { ...known, id: 'pulse', metric: 'heartRate', unit: 'bpm', value: 72 }
  const latest: LatestMeasurements = { ...empty, metrics: [
    ...snapshot(pulse, { ...fresh, measurementId: pulse.id, freshForSeconds: 15, ageSeconds: 14 }).metrics,
    ...snapshot(known, { ...fresh, ageSeconds: 3600 }).metrics,
  ] }
  const html = renderToStaticMarkup(<MetricOverview latest={latest} connected elapsedSeconds={2} />)
  const cards = html.match(/<article[\s\S]*?<\/article>/g)!
  expect(cards[0]).toContain('Данные устарели')
  expect(cards[0]).toContain('Демо-срок: 15 с')
  expect(cards[4]).toContain('Свежее измерение')
  expect(cards[4]).toContain('Демо-срок: 1 сут.')
})

it('does not let a delayed last delivery replace the known reading or its quality', () => {
  const delayed: typeof known = { ...known, id: 'delayed', value: 4, measuredAt: '2026-11-10T08:00:00Z', receivedAt: empty.evaluatedAt }
  const latest = snapshot(known, fresh, delayed, { ...fresh, status: 'stale', measurementId: delayed.id, ageSeconds: 345600 })
  const html = renderToStaticMarkup(<MetricOverview latest={latest} connected />)
  const text = html.replace(/<[^>]+>/g, '')
  expect(text).toContain('7,5 h')
  expect(text).toContain('Последняя доставка: 4 h')
  expect(text).toContain('Свежее измерение')
  expect(text).toContain('Данные устарели')
})

it('identifies future measurement timestamps as clock skew without a fresh claim', () => {
  const latest = snapshot(known, { ...fresh, status: 'clockSkew', ageSeconds: null, clockSkew: true })
  const html = renderToStaticMarkup(<MetricOverview latest={latest} connected elapsedSeconds={864000} />)
  expect(html).toContain('Расхождение часов')
  expect(html).not.toContain('quality-fresh')
})

it('preserves the reading offline but labels previous freshness as unconfirmed', () => {
  const html = renderToStaticMarkup(<MetricOverview latest={snapshot(known, fresh)} connected={false} />)
  const text = html.replace(/<[^>]+>/g, '')
  expect(text).toContain('7,5 h')
  expect(text).toContain('Свежесть не подтверждена')
  expect(text).not.toContain('Свежее измерение')
  expect(html).toContain('Последняя оценка сервера: Свежее измерение')
  expect(html).not.toContain('quality-fresh')
})
