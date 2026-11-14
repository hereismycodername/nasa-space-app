import type { MeasurementSeries, Metric } from '../shared/types'

export interface SeriesHover {
  metric: Metric
  startAt: string
}

/** Resolve against the current response so polling never leaves a stale tooltip value. */
export function getHoveredSeriesPoint(series: MeasurementSeries, hovered: SeriesHover | null) {
  return hovered?.metric === series.metric
    ? series.points.find(point => point.startAt === hovered.startAt && point.count > 0)
    : undefined
}

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
