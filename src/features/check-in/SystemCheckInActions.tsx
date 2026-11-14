import { METRICS } from '../dashboard/metrics'
import type { BodySystem } from '../dashboard/metrics'
import type { Metric } from '../../shared/types'

/** Body selection chooses a context; recording remains an explicit user action. */
export function SystemCheckInActions({ system, onAdd, supportedMetrics = [] }: { system: BodySystem | null; onAdd: (metric: Metric) => void; supportedMetrics?: readonly Metric[] }) {
  if (!system) return null
  const metrics = METRICS.filter(metric => metric.system === system)
  return <div className="system-checkin-actions" aria-label="Запись для выбранной области">
    <p className="note">Внести измерение или отметить симптом:</p>
    <div>{metrics.toSorted((a, b) => Number(Boolean(b.quickInputLabel)) - Number(Boolean(a.quickInputLabel))).map(metric => <button key={metric.id} type="button" className="secondary-button" disabled={!supportedMetrics.includes(metric.id)} onClick={() => { if (supportedMetrics.includes(metric.id)) onAdd(metric.id) }}>{metric.quickInputLabel ?? `Внести: ${metric.label}`}</button>)}</div>
    {metrics.some(metric => !supportedMetrics.includes(metric.id)) ? <p className="note">Сервер пока не поддерживает ввод для отключённых показателей.</p> : null}
  </div>
}
