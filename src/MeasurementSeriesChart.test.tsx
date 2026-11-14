import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'
import { MeasurementSeriesChart } from './MeasurementSeriesChart'
import { buildSeriesChart, getHoveredSeriesPoint } from './data/seriesChart'
import type { MeasurementSeries, MeasurementSeriesPoint } from './shared/types'

function bucket(hour: number, value: number | null): MeasurementSeriesPoint {
  return {
    startAt: `2026-11-13T0${hour}:00:00.000Z`, endAt: `2026-11-13T0${hour + 1}:00:00.000Z`,
    value, min: value, max: value, count: value === null ? 0 : 1,
    measuredCount: value === null ? 0 : 1, receivedCount: 0,
    timeBasis: value === null ? null : 'measuredAt',
  }
}

const series: MeasurementSeries = {
  astronautId: 'demo-astronaut', metric: 'heartRate', unit: 'bpm', aggregation: 'mean',
  from: '2026-11-13T00:00:00.000Z', to: '2026-11-13T05:00:00.000Z', intervalMs: 3_600_000,
  timeBasis: 'measuredAt', points: [bucket(0, null), bucket(1, 70), bucket(2, null), bucket(3, 75), bucket(4, null)],
}

it('retains leading, interior and trailing gaps without zero-filling or removing buckets', () => {
  const chart = buildSeriesChart(series)
  expect(chart.points.map(point => point.value)).toEqual([null, 70, null, 75, null])
  expect(chart.points.map(point => point.time)).toEqual(series.points.map(point => Date.parse(point.startAt)))
  expect(chart.points[2]).toMatchObject({ count: 0, measuredCount: 0, receivedCount: 0, timeBasis: null })
})

it('covers the requested period including both endpoints even with sparse or absent observations', () => {
  for (const points of [series.points, [bucket(1, 70)], []]) {
    const chart = buildSeriesChart({ ...series, points })
    expect(chart.domain).toEqual([Date.parse(series.from), Date.parse(series.to)])
    expect(chart.ticks.at(0)).toBe(Date.parse(series.from))
    expect(chart.ticks.at(-1)).toBe(Date.parse(series.to))
  }
})

it('shows response period, aggregation, interval, unit and unknown measurement time explicitly', () => {
  const html = renderToStaticMarkup(<MeasurementSeriesChart series={{ ...series, timeBasis: 'mixed' }} />)
  expect(html).toContain('13.11.2026, 00:00')
  expect(html).toContain('13.11.2026, 05:00')
  expect(html).toContain('UTC')
  expect(html).toContain('Среднее за интервал')
  expect(html).toContain('Интервал: 1 ч · Единица: уд/мин')
  expect(html).toContain('неизвестным временем измерения')
  expect(html).toContain('сгруппированы по времени получения')
  expect(html).toContain('Их свежесть не подтверждена')
  expect(html).toContain('Разрывы графика обозначают интервалы без данных')
})

it('describes a discrete series as last values instead of an average', () => {
  const html = renderToStaticMarkup(<MeasurementSeriesChart series={{
    ...series, metric: 'sleepHours', unit: 'h', aggregation: 'last',
    points: [{ ...bucket(1, 7.5), min: null, max: null }],
  }} />)
  expect(html).toContain('Последнее значение за интервал')
  expect(html).toContain('Единица: ч')
  expect(html).not.toContain('Среднее')
  expect(html).not.toContain('неизвестным временем измерения')
})

it('distinguishes an empty received period from initial loading', () => {
  const empty = renderToStaticMarkup(<MeasurementSeriesChart series={{ ...series, timeBasis: null, points: [bucket(0, null)] }} />)
  expect(empty).toContain('За выбранный период нет данных')
  expect(empty).not.toContain('Загрузка')
  expect(empty).not.toContain('неизвестным временем измерения')
  expect(renderToStaticMarkup(<MeasurementSeriesChart />)).toContain('Загрузка истории за 7 дней')
})

it('uses the current value for a hovered dot when the server refreshes the same interval', () => {
  const hovered = { metric: series.metric, startAt: series.points[1].startAt }
  const refreshed = { ...series, points: [bucket(1, 83)] }
  expect(getHoveredSeriesPoint(refreshed, hovered)?.value).toBe(83)
})

it('releases dot selection when its interval disappears or becomes a gap', () => {
  const hovered = { metric: series.metric, startAt: series.points[1].startAt }
  expect(getHoveredSeriesPoint({ ...series, points: [bucket(2, 73)] }, hovered)).toBeUndefined()
  expect(getHoveredSeriesPoint({ ...series, points: [bucket(1, null)] }, hovered)).toBeUndefined()
})

it('does not carry a hovered pulse value into another metric with the same interval', () => {
  const hovered = { metric: series.metric, startAt: series.points[1].startAt }
  expect(getHoveredSeriesPoint({ ...series, metric: 'mood' }, hovered)).toBeUndefined()
})
