import { metricConfig } from '../dashboard/metrics'
import { MeasurementSeriesChart } from '../../MeasurementSeriesChart'
import type { MeasurementSeries, Metric } from '../../shared/types'

export default function TelemetryCharts({ metric, series }: { metric: Metric; series: MeasurementSeries }) {
  return <div aria-label={`График ${metricConfig(metric).label}`}><MeasurementSeriesChart series={series} compact syncId="ground-telemetry" /></div>
}
