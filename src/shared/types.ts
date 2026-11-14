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
export interface MeasurementPage {
  items: Measurement[]
  hasMore: boolean
}
export interface HistoryParams {
  metric?: Metric
  from?: string
  to?: string
  limit?: number
}
export interface LatestMeasurementsResponse {
  metrics: {
    metric: Metric
    unit: Unit
    latestKnownMeasurement: Measurement | null
    latestUnknownTimeMeasurement: Measurement | null
  }[]
}
export interface SeriesParams {
  metric: Metric
  from: string
  to: string
  maxPoints?: number
}
export interface SeriesResponse {
  metric: Metric
  unit: Unit
  from: string
  to: string
  bucketSeconds: number
  aggregation: 'mean' | 'last'
  timeBasis: 'measuredAt'
  excludedUnknownTimeCount: number
  points: {
    start: string
    value: number | null
    min: number | null
    max: number | null
    count: number
  }[]
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
