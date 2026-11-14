export type Metric = 'heartRate' | 'systolicPressure' | 'diastolicPressure' | 'temperature' | 'sleepHours' | 'mood'
export type Unit = 'bpm' | 'mmHg' | '°C' | 'h' | 'score'
export interface NewMeasurement {
  deviceId: string
  sessionId: string
  sequence: number
  metric: Metric
  value: number
  unit: Unit
  measuredAt?: string | null
}
export interface Measurement extends Omit<NewMeasurement, 'measuredAt'> {
  id: string
  astronautId: string
  measuredAt: string | null
  receivedAt: string
  timeBasis: 'measuredAt' | 'receivedAt'
  source: 'manual' | 'simulator' | 'sensor'
}
export interface HistoryFilters {
  metric?: Metric
  from?: string
  to?: string
  limit?: number
  cursor?: string
}
export interface MeasurementHistory {
  items: Measurement[]
  hasMore: boolean
  nextCursor: string | null
}
export interface SeriesFilters {
  metric: Metric
  from: string
  to: string
  maxPoints?: number
}
export type SeriesTimeBasis = 'measuredAt' | 'receivedAt' | 'mixed' | null
export interface MeasurementSeriesPoint {
  startAt: string
  endAt: string
  value: number | null
  min: number | null
  max: number | null
  count: number
  measuredCount: number
  receivedCount: number
  timeBasis: SeriesTimeBasis
}
export interface MeasurementSeries {
  astronautId: string
  metric: Metric
  unit: Unit
  aggregation: 'mean' | 'last'
  from: string
  to: string
  intervalMs: number
  timeBasis: SeriesTimeBasis
  points: MeasurementSeriesPoint[]
}
export interface LatestMetric {
  metric: Metric
  latestMeasurement: Measurement | null
  latestKnownMeasurement: (Measurement & { measuredAt: string; timeBasis: 'measuredAt' }) | null
  latestReceivedMeasurement: Measurement | null
}
export interface LatestMeasurements {
  astronautId: string
  metrics: LatestMetric[]
}
export interface HealthAssessment {
  status: 'no-data' | 'stale' | 'not-evaluated' | 'unknown-time'
  explanation: string
}
export interface DailyTask {
  id: string
  title: string
  completedAt: string | null
}
