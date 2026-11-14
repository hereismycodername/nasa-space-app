import { lazy, Suspense, useCallback, useState } from 'react'
import { ChevronDown, Circle, Minus, RefreshCw } from 'lucide-react'
import { AnatomyPanel } from '../anatomy/AnatomyPanel'
import { METRICS, SYSTEMS, formatTime, formatValue } from '../dashboard/metrics'
import type { BodySystem } from '../dashboard/metrics'
import { QualityBadge } from '../dashboard/QualityBadge'
import { currentFreshness, freshnessLabels } from '../dashboard/freshness'
import type { LatestMeasurements, Metric } from '../../shared/types'
import { useTelemetry } from './useTelemetry'
import type { TelemetryRange } from './useTelemetry'
import './ground.css'

const TelemetryCharts = lazy(() => import('./TelemetryCharts'))
const utc = new Intl.DateTimeFormat('ru-RU', { timeZone: 'UTC', day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit' })
const sources = { manual: 'Ручной ввод', simulator: 'Симулятор', sensor: 'Датчик' }

export function ObservationLog({ latest, connected, elapsedSeconds }: { latest?: LatestMeasurements; connected: boolean; elapsedSeconds: number }) {
  return <section className="observation-log" aria-label="Текущие наблюдения">
    <div className="section-heading"><h2>Текущие наблюдения</h2><span className="note">{latest ? formatTime(latest.evaluatedAt) : 'Ожидание данных'}</span></div>
    <p className="note">Состояние шести показателей в последнем ответе сервера. Это текущая сводка, история событий не ведётся.</p>
    <div className="observation-list">{METRICS.map(config => {
      const entry = latest?.metrics.find(row => row.metric === config.id)
      const known = entry?.latestKnownMeasurement
      const received = entry?.latestReceivedMeasurement
      const main = known ?? received
      return <div className="observation-row" key={config.id}>
        <div><strong>{config.label}</strong><span>{main ? `${formatValue(main.value)} ${main.unit}` : 'Нет данных'}</span></div>
        <QualityBadge quality={entry?.quality} connected={connected} elapsedSeconds={elapsedSeconds} />
        <p>{main?.measuredAt ? `Измерено ${formatTime(main.measuredAt)}` : main ? `Получено ${formatTime(main.receivedAt)}; время измерения неизвестно.` : 'Измерение ещё не получено.'}{main ? ` · ${sources[main.source]}` : ''}</p>
        {known && received && known.id !== received.id ? <div className="observation-receipt"><span>Последняя доставка: {formatValue(received.value)} {received.unit}</span><QualityBadge quality={entry?.latestReceivedQuality} connected={connected} elapsedSeconds={elapsedSeconds} /></div> : null}
      </div>
    })}</div>
  </section>
}

export default function GroundConsole({ latest, connected, elapsedSeconds, refreshVersion, selectedSystem, onSystemSelect, onOpenMetric, channels, onChannelsChange, days, onDaysChange }: {
  latest?: LatestMeasurements
  connected: boolean
  elapsedSeconds: number
  refreshVersion: number
  selectedSystem: BodySystem | null
  onSystemSelect: (system: BodySystem | null) => void
  onOpenMetric: (metric: Metric) => void
  channels: Metric[]
  onChannelsChange: (channels: Metric[]) => void
  days: TelemetryRange
  onDaysChange: (days: TelemetryRange) => void
}) {
  const [retry, setRetry] = useState(0)
  const telemetry = useTelemetry(channels, days, refreshVersion + retry)
  const entries = latest?.metrics ?? []
  const available = SYSTEMS.filter(system => entries.some(entry => entry.latestMeasurement && METRICS.some(config => config.system === system.id && config.id === entry.metric))).map(system => system.id)
  const knownCount = entries.filter(entry => entry.latestKnownMeasurement).length
  const freshCount = connected ? entries.filter(entry => currentFreshness(entry.quality, elapsedSeconds) === 'fresh').length : 0
  const addSystem = useCallback((system: BodySystem | null) => {
    onSystemSelect(system)
    if (system) onChannelsChange([...new Set([...channels, ...METRICS.filter(config => config.system === system).map(config => config.id)])])
  }, [onSystemSelect, onChannelsChange, channels])
  function toggle(metric: Metric) { onChannelsChange(channels.includes(metric) ? channels.filter(id => id !== metric) : [...channels, metric]) }
  return <div className="ground-console">
    <aside className="object-browser" aria-label="Объекты наблюдения">
      <div className="section-heading"><h2>Объекты</h2><span className="note">{channels.length} / 6</span></div>
      <div className="object-profile"><ChevronDown size={14} aria-hidden="true" /><span>Демо-астронавт<small>Единственный профиль</small></span></div>
      <div className="object-systems">{SYSTEMS.map(system => {
        const metrics = METRICS.filter(config => config.system === system.id)
        return metrics.length ? <details key={system.id} open className={selectedSystem === system.id ? 'object-system-selected' : ''}>
          <summary>{system.label}<span>{metrics.length}</span></summary>
          <div>{metrics.map(config => {
            const entry = entries.find(row => row.metric === config.id)
            const reading = entry?.latestKnownMeasurement ?? entry?.latestReceivedMeasurement
            const status = entry?.quality ? currentFreshness(entry.quality, elapsedSeconds) : 'missing'
            return <label className={`object-metric ${channels.includes(config.id) ? 'object-watched' : ''}`} key={config.id} title={connected ? freshnessLabels[status] : 'Свежесть не подтверждена'}>
              <input type="checkbox" checked={channels.includes(config.id)} onChange={() => toggle(config.id)} aria-label={`График: ${config.label}`} />
              <span className={`object-dot ${connected ? `object-dot-${status}` : ''}`} /><span>{config.label}</span><small>{reading ? formatValue(reading.value) : '—'}<em>{config.unit}</em></small>
            </label>
          })}</div>
        </details> : <div className="object-unavailable" key={system.id}><span>{system.label}</span><small>Не подключено</small></div>
      })}</div>
      <p className="note object-help">Отметьте показатели для сравнения. Цвет означает качество данных.</p>
      <div className="object-actions"><button className="text-button" onClick={() => onChannelsChange(METRICS.map(config => config.id))}>Выбрать все</button><button className="text-button" onClick={() => onChannelsChange([])}>Очистить</button></div>
    </aside>

    <div className="ground-center">
      <div className="ground-profile-strip"><Circle size={10} aria-hidden="true" /><strong>Демо-астронавт</strong><span>{knownCount} / 6 с известным временем</span><span>{connected ? `Свежесть: ${freshCount} / 6` : 'Свежесть не подтверждена'}</span></div>
      <section className="telemetry-section" aria-label="Телеметрия экипажа">
        <div className="time-conductor"><div><h2>Телеметрия</h2><p>Общая временная шкала · UTC</p></div><div className="period-switch" aria-label="Период телеметрии">{([1, 7, 30] as const).map(period => <button key={period} aria-pressed={days === period} onClick={() => onDaysChange(period)}>{period === 1 ? '24 часа' : `${period} дней`}</button>)}</div><button className="icon-button" onClick={() => setRetry(value => value + 1)} aria-label="Обновить телеметрию"><RefreshCw size={15} aria-hidden="true" /></button></div>
        {telemetry.window ? <p className="telemetry-period">{utc.format(new Date(telemetry.window.from))} — {utc.format(new Date(telemetry.window.to))} UTC{telemetry.loading ? ' · обновление…' : ''}</p> : null}
        {telemetry.error ? <p className="inline-error" role="alert">{telemetry.error}{telemetry.window ? ' Показана предыдущая история.' : ''}</p> : null}
        {!channels.length ? <div className="empty-state"><h3>Выберите показатели</h3><p>Отметьте один или несколько показателей в списке объектов.</p></div> : null}
        <div className="telemetry-stack">{channels.map(metric => {
          const config = METRICS.find(row => row.id === metric)!
          const entry = entries.find(row => row.metric === metric)
          const reading = entry?.latestKnownMeasurement ?? entry?.latestReceivedMeasurement
          const series = telemetry.series[metric]
          return <section className="telemetry-channel" key={metric} aria-label={`Телеметрия: ${config.label}`}>
            <header><button className="telemetry-title" onClick={() => onOpenMetric(metric)}>{config.label}</button><span className="telemetry-value">{reading ? formatValue(reading.value) : '—'} <small>{config.unit}</small></span><QualityBadge quality={entry?.quality} connected={connected} elapsedSeconds={elapsedSeconds} /><button className="remove-channel" aria-label={`Убрать график: ${config.label}`} onClick={() => toggle(metric)}><Minus size={14} aria-hidden="true" /></button></header>
            {series ? <Suspense fallback={<div className="telemetry-loading">Загрузка графика…</div>}><TelemetryCharts metric={metric} series={series} /></Suspense> : <div className="telemetry-loading">{telemetry.error ? 'История недоступна' : 'Загрузка истории…'}</div>}
          </section>
        })}</div>
        <p className="note telemetry-footnote">Разрывы сохраняют интервалы без данных. Обновление каждые 15 секунд, пока вкладка открыта.</p>
      </section>
      <ObservationLog latest={latest} connected={connected} elapsedSeconds={elapsedSeconds} />
    </div>

    <aside className="ground-subject" aria-label="Выбранный профиль">
      <div className="subject-heading"><h2>Демо-астронавт</h2><span>Выбранный профиль</span></div>
      <AnatomyPanel selected={selectedSystem} onSelect={addSystem} availableSystems={available} />
      <p className="note">Выберите область, чтобы добавить её показатели на графики.</p>
      <section className="subject-quality"><h3>Качество данных</h3><dl><div><dt>Связь с API</dt><dd>{connected ? 'Установлена' : 'Недоступна'}</dd></div><div><dt>Свежие измерения</dt><dd>{connected ? `${freshCount} / 6` : 'Не подтверждено'}</dd></div><div><dt>Оценка сервера</dt><dd>{latest ? formatTime(latest.evaluatedAt) : 'Ожидание'}</dd></div></dl><p className="note">Свежесть измерения не является медицинским заключением.</p></section>
    </aside>
  </div>
}
