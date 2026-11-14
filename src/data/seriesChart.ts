import type { MeasurementSeries } from '../shared/types'

export function buildSeriesChart(series: MeasurementSeries) {
  const from = Date.parse(series.from)
  const to = Date.parse(series.to)
  return {
    domain: [from, to] as [number, number],
    ticks: Array.from({ length: 5 }, (_, index) => from + (to - from) * index / 4),
    // Keep empty buckets in the data so Recharts leaves real gaps between observations.
    points: series.points.map(point => ({ ...point, time: Date.parse(point.startAt) })),
  }
}
