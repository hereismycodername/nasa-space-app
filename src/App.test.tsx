import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'
import { MetricOverview } from './App'
import type { LatestMeasurements, LatestMetric, Measurement } from './shared/types'

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
  const latest: LatestMeasurements = {
    astronautId: 'demo-astronaut',
    metrics: [{ metric: 'sleepHours', latestMeasurement: unknown, latestKnownMeasurement: known, latestReceivedMeasurement: unknown }],
  }
  const html = renderToStaticMarkup(<MetricOverview latest={latest} />)
  expect(html.replace(/<[^>]+>/g, '')).toContain('7,5 h')
  expect(html).toContain('6 h')
  expect(html).toContain('Время измерения неизвестно; свежесть не подтверждена.')
  expect(html.match(/<article/g)).toHaveLength(6)
})

it('filters the overview to the selected body system without inventing unavailable metrics', () => {
  const heart = renderToStaticMarkup(<MetricOverview selectedSystem="heart" latest={{ astronautId: 'demo-astronaut', metrics: [] }} />)
  expect(heart.match(/<article/g)).toHaveLength(3)
  expect(heart).toContain('Пульс')
  expect(heart).not.toContain('Самочувствие')
  const lungs = renderToStaticMarkup(<MetricOverview selectedSystem="lungs" />)
  expect(lungs).toContain('Показатели пока не подключены')
  expect(lungs).not.toContain('<article')
})

it('shows missing values explicitly for all metrics when the latest response is empty', () => {
  const html = renderToStaticMarkup(<MetricOverview latest={{ astronautId: 'demo-astronaut', metrics: [] }} />)
  expect(html.match(/Нет данных/g)).toHaveLength(6)
  expect(html).not.toContain('Измерено')
})
