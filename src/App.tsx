import { NasaCatalogPanel } from './features/nasa/NasaContext'
import './features/dashboard/crew-layout.css'
import { lazy, Suspense, useCallback, useState } from 'react'
import { ArrowUpRight, Check, ChevronRight, CircleHelp, Database, Plus, Radio, RefreshCw, WifiOff } from 'lucide-react'
import { useDashboard } from './data/useDashboard'
import { AnatomyPanel } from './features/anatomy/AnatomyPanel'
import { CheckInPanel } from './features/check-in/CheckInPanel'
import { SystemCheckInActions } from './features/check-in/SystemCheckInActions'
import { TaskPanel } from './features/tasks/TaskPanel'
import { METRICS, SYSTEMS, formatUnit, formatTime, formatValue, supportedMetricConfigs } from './features/dashboard/metrics'
import { QualityBadge } from './features/dashboard/QualityBadge'
import { formatFreshnessWindow } from './features/dashboard/freshness'
import { useFreshnessElapsed } from './features/dashboard/useFreshnessElapsed'
import { useHealthElapsed } from './features/dashboard/useHealthElapsed'
import { FlightEducationPanel, HealthBadge, HealthPanel, MetricHealth } from './features/dashboard/HealthPanel'
import { metricHealth, systemHealth } from './features/dashboard/health'
import type { HealthContext } from './features/dashboard/health'
import type { BodySystem } from './features/dashboard/metrics'
import type { LatestMeasurements, Metric } from './shared/types'
import type { TelemetryRange } from './features/ground/useTelemetry'

const TrendPanel = lazy(() => import('./features/dashboard/TrendPanel'))
const GroundConsole = lazy(() => import('./features/ground/GroundConsole'))
type Tab = 'overview' | 'trends' | 'system' | 'nasa'
type Role = 'crew' | 'ground'
const sourceNames = { manual: 'Ручной ввод', simulator: 'Симулятор', sensor: 'Датчик' }

export function MetricOverview({ latest, selectedSystem = null, onSelect, onAdd, connected = false, elapsedSeconds = 0, healthContext }: {
  latest?: LatestMeasurements
  selectedSystem?: BodySystem | null
  onSelect?: (metric: Metric) => void
  onAdd?: (metric: Metric) => void
  connected?: boolean
  elapsedSeconds?: number
  healthContext?: HealthContext
}) {
  const metrics = supportedMetricConfigs(latest).filter(metric => !selectedSystem || metric.system === selectedSystem)
  if (!metrics.length) return <div className="empty-state">
    <CircleHelp size={24} aria-hidden="true" />
    <h3>{latest ? 'Показатели пока не подключены' : 'Получаем список показателей'}</h3>
    <p>{latest ? 'Сервер пока не поддерживает ввод для этой области. Выберите другую область на схеме.' : 'Дождитесь ответа сервера, чтобы увидеть доступные измерения.'}</p>
  </div>
  return <div className="metric-grid">
    {metrics.map(config => {
      const entry = latest?.metrics.find(row => row.metric === config.id)
      const known = entry?.latestKnownMeasurement
      const received = entry?.latestReceivedMeasurement
      const unknown = received?.measuredAt === null ? received : undefined
      const context = healthContext ?? { latest, connected, latestElapsedSeconds: elapsedSeconds }
      const health = metricHealth(config.id, context)
      return <article className={`metric-card ${known ? 'has-reading' : ''} metric-health-${health.status}`} key={config.id}>
        <div className="metric-card-heading"><h3>{config.label}</h3><span className="metric-mark" aria-hidden="true" /></div>
        {known ? <>
          <p className="metric-value">{formatValue(known.value)} <small>{formatUnit(known.unit, config.id)}</small></p>
          <p className="note" title={known.measuredAt}>Измерено {formatTime(known.measuredAt)}</p>
          <p className="metric-source">{sourceNames[known.source]}</p>
        </> : <><p className="metric-value missing-value">— <small>{formatUnit(config.unit, config.id)}</small></p><p className="note">{latest ? (unknown ? 'Нет измерения с известным временем' : 'Нет данных') : 'Загрузка…'}</p><p className="metric-source">Источник не получен</p></>}
        <div className="metric-quality"><QualityBadge quality={entry?.quality} connected={connected} elapsedSeconds={elapsedSeconds} />
          {entry?.quality ? <span className="note">Демо-срок: {formatFreshnessWindow(entry.quality.freshForSeconds)}</span> : null}
        </div>
        <div className="metric-delivery">{unknown ? <div className="unknown-time"><p>
          Получено {formatTime(unknown.receivedAt)}: {formatValue(unknown.value)} {formatUnit(unknown.unit, config.id)}.
          {' '}Время измерения неизвестно; свежесть не подтверждена.
        </p>{known ? <QualityBadge quality={entry?.latestReceivedQuality} connected={connected} elapsedSeconds={elapsedSeconds} /> : null}</div> : null}
        {received && known && received.id !== known.id && !unknown ? <div className="received-quality">
          <p>Последняя доставка: {formatValue(received.value)} {formatUnit(received.unit, config.id)}, получено {formatTime(received.receivedAt)}.</p>
          <QualityBadge quality={entry?.latestReceivedQuality} connected={connected} elapsedSeconds={elapsedSeconds} />
        </div> : null}</div>
        <MetricHealth metric={config.id} context={context} onRecord={onAdd ? () => onAdd(config.id) : undefined} onHistory={onSelect ? () => onSelect(config.id) : undefined} />
        {onAdd ? <button type="button" className="metric-link" onClick={() => onAdd(config.id)}>{config.quickInputLabel ?? 'Внести измерение'} <Plus size={15} aria-hidden="true" /></button> : null}
        {onSelect ? <button className="metric-link" onClick={() => onSelect(config.id)} aria-label={`Динамика: ${config.label}`}>
          Динамика <ArrowUpRight size={15} aria-hidden="true" />
        </button> : null}
      </article>
    })}
  </div>
}

function CoverageGauge({ count, total }: { count: number; total: number }) {
  return <div className="coverage-gauge" role="img" aria-label={`Данные есть для ${count} из ${total} показателей`}>
    <span className="coverage-value" aria-hidden="true">{count}<small> / {total}</small></span>
    <div className="coverage-segments" aria-hidden="true">
      {Array.from({ length: total }, (_, index) => <span key={index} data-filled={index < count} />)}
    </div>
  </div>
}

export default function App() {
  const { latest, assessment, tasks: taskList, connected, loading: polling, lastUpdated, qualityObservedAt, qualityReceivedAt, assessmentObservedAt, assessmentReceivedAt, previousHealthAlerts, refresh } = useDashboard()
  const tasks = taskList?.tasks
  const freshnessElapsed = useFreshnessElapsed(latest, qualityObservedAt, qualityReceivedAt)
  const healthElapsed = useHealthElapsed(assessment, assessmentObservedAt, assessmentReceivedAt)
  const healthContext: HealthContext = { assessment, latest, connected, updating: polling, elapsedSeconds: healthElapsed, latestElapsedSeconds: freshnessElapsed, previousHealthAlerts }
  const healthStatuses = systemHealth(healthContext)
  const loading = polling && !lastUpdated
  const [role, setRole] = useState<Role>('crew')
  const [tab, setTab] = useState<Tab>('overview')
  const [selectedSystem, setSelectedSystem] = useState<BodySystem | null>(null)
  const [metric, setMetric] = useState<Metric>('heartRate')
  const [checkIn, setCheckIn] = useState(false)
  const [formMetric, setFormMetric] = useState<Metric>()
  const [refreshVersion, setRefreshVersion] = useState(0)
  const [groundChannels, setGroundChannels] = useState<Metric[]>(['heartRate', 'temperature', 'sleepHours'])
  const [groundDays, setGroundDays] = useState<TelemetryRange>(1)
  const selectSystem = useCallback((id: BodySystem | null) => {
    setSelectedSystem(id)
    setTab('overview')
    setCheckIn(false)
  }, [])
  const entries = latest?.metrics ?? []
  const supported = supportedMetricConfigs(latest)
  const supportedIds = supported.map(item => item.id)
  const knownCount = entries.filter(entry => entry.latestKnownMeasurement).length
  const unknownEntries = entries.filter(entry => entry.latestReceivedMeasurement?.measuredAt === null)
  const availableSystems = SYSTEMS.filter(system => METRICS.some(config => config.system === system.id &&
    entries.some(entry => entry.metric === config.id && entry.latestMeasurement))).map(system => system.id)
  const selected = SYSTEMS.find(system => system.id === selectedSystem)
  const completedTasks = tasks?.filter(task => task.completedAt).length ?? 0
  const sourceSet = [...new Set(entries.flatMap(entry => entry.latestReceivedMeasurement ? [sourceNames[entry.latestReceivedMeasurement.source]] : []))]
  function openMetric(id: Metric) { if (supportedIds.includes(id)) { setMetric(id); setTab('trends'); setCheckIn(false) } }
  function afterSave() { refresh(); setRefreshVersion(value => value + 1) }
  function changeTab(next: Tab) { setTab(next); setCheckIn(false) }
  function openCheckIn() {
    if (!supported.length) return
    setFormMetric(current => current && supportedIds.includes(current) ? current : supportedIds.includes(metric) ? metric : supported[0].id)
    setCheckIn(true)
  }
  function openMetricCheckIn(id: Metric) { if (supportedIds.includes(id)) { setFormMetric(id); setCheckIn(true) } }

  return <div className={`app-shell ${role === 'ground' ? 'ground-mode' : 'crew-mode'}`}>
    <a className="skip-link" href="#main-content">Перейти к содержимому</a>
    <header className="topbar">
      <a className="brand" href="#main-content" aria-label="AstroCheck">ASTROCHECK</a>
      <div className="mission-context"><span className="context-dot" /> Демо-миссия <span className="context-divider">/</span> Один астронавт</div>
      <div className={`connection-status ${connected ? 'connected' : 'disconnected'}`} role="status">
        {connected ? <Radio size={14} aria-hidden="true" /> : <WifiOff size={14} aria-hidden="true" />}
        {loading ? 'Подключение…' : connected ? 'Связь установлена' : 'Нет связи'}
      </div>
      <div className="role-switch" aria-label="Режим просмотра">
        <button aria-pressed={role === 'crew'} onClick={() => setRole('crew')}>Экипаж<span>Наблюдение</span></button>
        <button aria-pressed={role === 'ground'} onClick={() => setRole('ground')}>Земля<span>Наземная команда</span></button>
      </div>
      <button className="primary-button header-checkin" onClick={openCheckIn} disabled={!supported.length}><Plus size={16} aria-hidden="true" /> Внести измерение</button>
    </header>
    <div className="workspace">
      {role === 'crew' ? <aside className="anatomy-sidebar" aria-label="Карта организма">
        <AnatomyPanel selected={selectedSystem} onSelect={selectSystem} availableSystems={availableSystems} healthStatuses={healthStatuses} />
        <div className="anatomy-footnote"><span className="legend-dot" /> Есть измерения <span className="legend-dot empty" /> Пока нет показателей</div>
      </aside> : null}
      <main id="main-content" className="main-content" tabIndex={-1}>
        <nav className="page-tabs" aria-label="Разделы">
          <button aria-current={tab === 'overview' && !checkIn ? 'page' : undefined} onClick={() => changeTab('overview')}>{role === 'crew' ? 'Обзор здоровья' : 'Наблюдение экипажа'}</button>
          <button aria-current={tab === 'trends' && !checkIn ? 'page' : undefined} onClick={() => changeTab('trends')}>Динамика</button>
          <button aria-current={tab === 'system' && !checkIn ? 'page' : undefined} onClick={() => changeTab('system')}>Система и связь</button>
          <button aria-current={tab === 'nasa' && !checkIn ? 'page' : undefined} onClick={() => changeTab('nasa')}>Источники NASA / CSA</button>
        </nav>
        {!loading && !connected ? <div className="connection-banner" role="alert"><WifiOff size={18} aria-hidden="true" /><div><strong>Связь с сервером потеряна</strong><p>{lastUpdated ? `Показаны данные, полученные ${formatTime(lastUpdated.toISOString())}. Свежесть не подтверждена.` : 'Проверьте подключение и запущен ли сервер.'}</p></div><button className="secondary-button" onClick={refresh}>Повторить</button></div> : null}
        {formMetric ? <div hidden={!checkIn}><CheckInPanel key={formMetric} initialMetric={formMetric} supportedMetrics={supportedIds} onSaved={afterSave} onClose={() => setCheckIn(false)} /></div> : null}
        {!checkIn ? <>
          <div className="page-heading"><div><p className="breadcrumb"><button onClick={() => { changeTab('overview'); setSelectedSystem(null) }}>{role === 'crew' ? 'Экипаж' : 'Наземная команда'}</button> <ChevronRight size={13} aria-hidden="true" /> <span>Демо-астронавт</span></p>
            <h1>{tab === 'nasa' ? 'Научная основа наблюдений' : tab === 'trends' ? 'Динамика показателей' : tab === 'system' ? 'Система и связь' : role === 'crew' ? 'Обзор состояния' : 'Мониторинг экипажа'}</h1>
            <p className="heading-description">{tab === 'nasa' ? 'Риски и исследования NASA и CSA, связи с показателями и первоисточники.' : tab === 'overview' ? 'Измерения, самочувствие и задачи в одном пространстве.' : tab === 'trends' ? 'Полная история за выбранный период. Пропуски остаются видимыми.' : 'Состояние подключения и доступные функции мониторинга.'}</p></div>
            <button className="icon-button" aria-label="Обновить данные" title="Обновить данные" onClick={() => { refresh(); setRefreshVersion(value => value + 1) }}><RefreshCw size={17} aria-hidden="true" /></button>
          </div>
          {tab === 'nasa' ? <NasaCatalogPanel /> : tab === 'trends' ? <><Suspense fallback={<div className="panel loading-panel trend-placeholder">Загрузка графика…</div>}><TrendPanel metric={metric} supportedMetrics={supportedIds} onMetricChange={setMetric} refreshVersion={refreshVersion} onAdd={() => openMetricCheckIn(metric)} /></Suspense><section className="panel"><h2>{METRICS.find(item => item.id === metric)?.label}: оценка последнего измерения</h2><MetricHealth metric={metric} context={healthContext} onRecord={() => openMetricCheckIn(metric)} onHistory={() => document.querySelector(".trend-panel")?.scrollIntoView({ behavior: "smooth" })} /></section></> : tab === 'system' ? <>
            <div className="panel system-panel"><h2><Database size={18} aria-hidden="true" /> Состояние подключения</h2>
              <dl className="system-facts"><div><dt>Сервер</dt><dd>{loading ? 'Проверка…' : connected ? 'Доступен' : 'Недоступен'}</dd></div><div><dt>Последнее обновление</dt><dd>{lastUpdated ? formatTime(lastUpdated.toISOString()) : 'Ещё не получено'}</dd></div><div><dt>Хранение измерений</dt><dd>Локальная база данных</dd></div><div><dt>Источники</dt><dd>{sourceSet.join(', ') || 'Измерения пока не получены'}</dd></div></dl>
              <p className="note">Обновление каждые 5 секунд при открытой вкладке. После потери связи предыдущие данные остаются на экране.</p>
            </div>
            <div className="panel"><h2>Качество последнего сообщения</h2><p>{assessment?.explanation ?? 'Оценка ещё не получена.'}</p><p className="note">Совместимая сводка одного последнего принятого сообщения. Качество данных и оценка показателей по правилам отображаются отдельно.</p>{assessment ? <p className="note">Оценено сервером {formatTime(assessment.evaluatedAt)}.{!connected ? ' Это предыдущая оценка; актуальность не подтверждена.' : ''}</p> : null}</div>
            <HealthPanel context={healthContext} />
            <div className="panel"><h2>Возможности прототипа</h2><p>Один профиль, {supported.length} поддерживаемых сервером показателей, история, задачи и объяснимые демо-правила. Для показателей без правил выводится «Не оценено». Передача сообщений экипажу и на Землю не подключена. Пороговые правила не валидированы для космического полёта и не заменяют индивидуальный медицинский план.</p></div>
          </> : role === 'ground' ? <>
            <Suspense fallback={<div className="loading-panel">Загрузка наземной консоли…</div>}><GroundConsole latest={latest} connected={connected} elapsedSeconds={freshnessElapsed} healthContext={healthContext} refreshVersion={refreshVersion} selectedSystem={selectedSystem} onSystemSelect={selectSystem} onOpenMetric={openMetric} onAddMetric={openMetricCheckIn} channels={groundChannels} onChannelsChange={setGroundChannels} days={groundDays} onDaysChange={setGroundDays} /></Suspense>
            <TaskPanel tasks={taskList} connected={connected} onChanged={refresh} />
          </> : <>
            <div className="summary-grid">
              <section className="panel coverage-panel"><h2>Полнота наблюдений</h2><div className="coverage-content"><CoverageGauge count={knownCount} total={supported.length} /><div><strong>{loading ? 'Получаем измерения' : supported.length && knownCount === supported.length ? 'Все показатели записаны' : `Без записи с известным временем: ${supported.length - knownCount}`}</strong><p>С известным временем измерения</p><span className="neutral-tag">Не оценка здоровья</span></div></div></section>
              <section className="panel session-panel"><h2>Сеанс наблюдения</h2><p className="session-title">Демо-миссия</p><dl><div><dt>Источники</dt><dd>{sourceSet.join(', ') || 'Ожидание данных'}</dd></div><div><dt>Обновлено</dt><dd>{lastUpdated ? formatTime(lastUpdated.toISOString()) : '—'}</dd></div></dl><span className={`status-tag ${connected ? 'online' : ''}`}>{connected ? 'Мониторинг подключён' : loading ? 'Подключение' : 'Нет связи'}</span></section>
              <section className="panel routine-panel"><h2>Задачи</h2><div className="routine-value">{completedTasks}<small> / {tasks?.length ?? '—'}</small><Check size={22} aria-hidden="true" /></div><p className="note">Отмечено выполненными</p><div className="task-track"><span style={{ width: `${tasks?.length ? completedTasks / tasks.length * 100 : 0}%` }} /></div></section>
            </div>
            <div className="overview-columns">
              <section className="panel systems-panel"><div className="section-heading"><h2>Системы организма</h2><span className="note">Демо-оценка показателей</span></div><div className="system-list system-health-list">{SYSTEMS.map(system => {
                const configs = supported.filter(config => config.system === system.id)
                const count = configs.filter(config => entries.some(entry => entry.metric === config.id && entry.latestKnownMeasurement)).length
                return <button className="system-row" key={system.id} onClick={() => setSelectedSystem(current => current === system.id ? null : system.id)} aria-pressed={selectedSystem === system.id}>
                  <span>{system.label}</span>
                  {configs.length ? <HealthBadge status={healthStatuses[system.id] ?? 'notEvaluated'} /> : <span className="system-unavailable">Не подключено</span>}
                  <span className="system-count">{configs.length ? `${count}/${configs.length}` : '—'}</span>
                </button>
              })}</div><p className="note">Выберите область на схеме, чтобы отфильтровать показатели.</p></section>
              <HealthPanel context={healthContext} compact />
            </div>
            {unknownEntries.length ? <p className="data-notice note">Время последней доставки неизвестно: {unknownEntries.map(entry => METRICS.find(config => config.id === entry.metric)?.label).join(', ')}. Свежесть не подтверждена.</p> : null}
            <div className="section-heading metrics-heading"><div><h2>{selected?.label ?? 'Все показатели'}</h2>{selected ? <p className="note">{selected.description}</p> : null}</div>{selectedSystem ? <button className="text-button" onClick={() => setSelectedSystem(null)}>Показать все</button> : <span className="note">{supported.length} показателей</span>}</div>
            <SystemCheckInActions system={selectedSystem} onAdd={openMetricCheckIn} supportedMetrics={supportedIds} />
            {latest ? <p className="quality-context note">Оценка сервера: {formatTime(latest.evaluatedAt)}. Сроки различаются по показателям; время доставки не подтверждает свежесть.{!connected ? ' Показан предыдущий ответ; свежесть не подтверждена.' : ''}</p> : null}
            <MetricOverview latest={latest} selectedSystem={selectedSystem} onSelect={openMetric} onAdd={openMetricCheckIn} connected={connected} elapsedSeconds={freshnessElapsed} healthContext={healthContext} />
            <TaskPanel tasks={taskList} connected={connected} onChanged={refresh} />
          </>}
          {assessment?.health && tab !== 'trends' ? <FlightEducationPanel education={assessment.health.education} /> : null}
        </> : null}
        <footer className="page-footer"><CircleHelp size={14} aria-hidden="true" /><p>Демонстрационный прототип. Правила не являются диагнозом или медицинским протоколом NASA. Показания симулятора синтетические.</p></footer>
      </main>
    </div>
    <div className="statusbar"><span><span className={`context-dot ${connected ? '' : 'offline'}`} /> {connected ? 'Сервер подключён' : loading ? 'Подключение' : 'Сервер недоступен'}</span><span>Показатели {knownCount}/{supported.length}</span><span>Задачи {completedTasks}/{tasks?.length ?? '—'}</span><span className="statusbar-end">AstroCheck · Space Health</span></div>
  </div>
}
