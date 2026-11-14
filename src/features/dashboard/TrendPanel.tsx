import { useEffect, useState } from 'react'
import { Plus, RefreshCw } from 'lucide-react'
import { getMeasurementSeries } from '../../data/measurements'
import { MeasurementSeriesChart } from '../../MeasurementSeriesChart'
import type { MeasurementSeries, Metric } from '../../shared/types'
import { METRICS, metricConfig } from './metrics'

export default function TrendPanel({ metric, onMetricChange, refreshVersion, onAdd }: {
  metric: Metric; onMetricChange: (metric: Metric) => void; refreshVersion: number; onAdd: () => void
}) {
  const [days, setDays] = useState(7)
  const [series, setSeries] = useState<MeasurementSeries>()
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [retry, setRetry] = useState(0)
  useEffect(() => {
    let disposed = false
    let controller: AbortController | undefined
    let timer: ReturnType<typeof setTimeout> | undefined
    let inFlight = false
    let pendingRefresh = false
    async function load() {
      if (disposed || inFlight || document.hidden) return
      clearTimeout(timer)
      controller = new AbortController()
      inFlight = true
      setLoading(true)
      setError('')
      const to = new Date()
      try {
        const result = await getMeasurementSeries({ metric, from: new Date(to.getTime() - days * 86_400_000).toISOString(), to: to.toISOString(), maxPoints: days === 1 ? 96 : 168 }, controller.signal)
        if (!disposed) setSeries(result)
      } catch {
        if (!disposed && !controller.signal.aborted) setError('Не удалось загрузить историю. Проверьте связь с сервером.')
      } finally {
        inFlight = false
        if (!disposed) {
          setLoading(false)
          if (!document.hidden) {
            if (pendingRefresh) { pendingRefresh = false; void load() }
            else timer = setTimeout(() => void load(), 15_000)
          }
        }
      }
    }
    function onVisibility() {
      if (document.hidden) { pendingRefresh = false; clearTimeout(timer); controller?.abort() }
      else if (inFlight) pendingRefresh = true
      else void load()
    }
    document.addEventListener('visibilitychange', onVisibility)
    void load()
    return () => { disposed = true; controller?.abort(); clearTimeout(timer); document.removeEventListener('visibilitychange', onVisibility) }
  }, [metric, days, refreshVersion, retry])
  const config = metricConfig(metric)
  // Changing filters never displays a prior metric under the newly selected label.
  const current = series?.metric === metric && Date.parse(series.to) - Date.parse(series.from) === days * 86_400_000 ? series : undefined
  const points = current?.points.filter(point => point.count > 0) ?? []
  const total = points.reduce((sum, point) => sum + point.count, 0)
  const measured = points.reduce((sum, point) => sum + point.measuredCount, 0)
  return <section className="panel trend-panel" aria-label="История измерений">
    <div className="trend-controls"><label className="metric-select">Показатель<select value={metric} onChange={event => onMetricChange(event.target.value as Metric)}>{METRICS.map(item => <option value={item.id} key={item.id}>{item.label}</option>)}</select></label><div className="period-switch" aria-label="Период истории">{[1, 7, 30].map(value => <button key={value} aria-pressed={days === value} onClick={() => setDays(value)}>{value === 1 ? '24 часа' : `${value} дней`}</button>)}</div></div>
    <div className="section-heading"><div><h2>{config.label}</h2><p className="note">{config.description}</p></div><button className="secondary-button" onClick={onAdd}><Plus size={15} aria-hidden="true" /> Измерение</button></div>
    {error ? <div className="inline-error" role="alert"><p>{error}{current ? ' Показан предыдущий ответ; актуальность не подтверждена.' : ''}</p><button className="secondary-button" onClick={() => setRetry(value => value + 1)}><RefreshCw size={14} aria-hidden="true" /> Повторить</button></div> : null}
    {current ? <><MeasurementSeriesChart series={current} /><div className="series-stats"><div><span>Измерений за период</span><strong>{total}</strong></div><div><span>С известным временем</span><strong>{measured}</strong></div><div><span>Интервалов с данными</span><strong>{points.length}<small> / {current.points.length}</small></strong></div></div></> : loading ? <div className="loading-panel" role="status">Загрузка истории…</div> : null}
  </section>
}
