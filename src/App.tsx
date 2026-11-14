import { lazy, Suspense, useCallback, useState } from 'react'
import { ArrowUpRight, Check, ChevronRight, CircleHelp, Database, Plus, Radio, RefreshCw, WifiOff } from 'lucide-react'
import { useDashboard } from './data/useDashboard'
import { AnatomyPanel } from './features/anatomy/AnatomyPanel'
import { CheckInPanel } from './features/check-in/CheckInPanel'
import { TaskPanel } from './features/tasks/TaskPanel'
import { METRICS, SYSTEMS, formatTime, formatValue } from './features/dashboard/metrics'
import { QualityBadge } from './features/dashboard/QualityBadge'
import { formatFreshnessWindow } from './features/dashboard/freshness'
import { useFreshnessElapsed } from './features/dashboard/useFreshnessElapsed'
import type { BodySystem } from './features/dashboard/metrics'
import type { LatestMeasurements, Metric } from './shared/types'
import type { TelemetryRange } from './features/ground/useTelemetry'

const TrendPanel = lazy(() => import('./features/dashboard/TrendPanel'))
const GroundConsole = lazy(() => import('./features/ground/GroundConsole'))
type Tab = 'overview' | 'trends' | 'system'
type Role = 'crew' | 'ground'
const sourceNames = { manual: 'Ручной ввод', simulator: 'Симулятор', sensor: 'Датчик' }

export function MetricOverview({ latest, selectedSystem = null, onSelect, connected = false, elapsedSeconds = 0 }: {
  latest?: LatestMeasurements
  selectedSystem?: BodySystem | null
  onSelect?: (metric: Metric) => void
  connected?: boolean
  elapsedSeconds?: number
}) {
  const metrics = METRICS.filter(metric => !selectedSystem || metric.system === selectedSystem)
  if (!metrics.length) return <div className="empty-state">
    <CircleHelp size={24} aria-hidden="true" />
    <h3>Показатели пока не подключены</h3>
    <p>Для этой системы измерения ещё не собираются. Выберите другую область на схеме.</p>
  </div>
  return <div className="metric-grid">
    {metrics.map(config => {
      const entry = latest?.metrics.find(row => row.metric === config.id)
      const known = entry?.latestKnownMeasurement
      const received = entry?.latestReceivedMeasurement
      const unknown = received?.measuredAt === null ? received : undefined
      return <article className={`metric-card ${known ? 'has-reading' : ''}`} key={config.id}>
        <div className="metric-card-heading"><h3>{config.label}</h3><span className="metric-mark" aria-hidden="true" /></div>
        {known ? <>
          <p className="metric-value">{formatValue(known.value)} <small>{known.unit}</small></p>
          <p className="note" title={known.measuredAt}>Измерено {formatTime(known.measuredAt)}</p>
          <p className="metric-source">{sourceNames[known.source]}</p>
        </> : <><p className="metric-value missing-value">— <small>{config.unit}</small></p><p className="note">{latest ? (unknown ? 'Нет измерения с известным временем' : 'Нет данных') : 'Загрузка…'}</p></>}
        <div className="metric-quality"><QualityBadge quality={entry?.quality} connected={connected} elapsedSeconds={elapsedSeconds} />
          {entry?.quality ? <span className="note">Демо-срок: {formatFreshnessWindow(entry.quality.freshForSeconds)}</span> : null}
        </div>
        {unknown ? <div className="unknown-time"><p>
          Получено {formatTime(unknown.receivedAt)}: {formatValue(unknown.value)} {unknown.unit}.
          {' '}Время измерения неизвестно; свежесть не подтверждена.
        </p>{known ? <QualityBadge quality={entry?.latestReceivedQuality} connected={connected} elapsedSeconds={elapsedSeconds} /> : null}</div> : null}
        {received && known && received.id !== known.id && !unknown ? <div className="received-quality">
          <p>Последняя доставка: {formatValue(received.value)} {received.unit}, получено {formatTime(received.receivedAt)}.</p>
          <QualityBadge quality={entry?.latestReceivedQuality} connected={connected} elapsedSeconds={elapsedSeconds} />
        </div> : null}
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
  const { latest, assessment, tasks, connected, loading: polling, lastUpdated, qualityObservedAt, qualityReceivedAt, refresh } = useDashboard()
  const freshnessElapsed = useFreshnessElapsed(latest, qualityObservedAt, qualityReceivedAt)
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
  const knownCount = entries.filter(entry => entry.latestKnownMeasurement).length
  const unknownEntries = entries.filter(entry => entry.latestReceivedMeasurement?.measuredAt === null)
  const availableSystems = SYSTEMS.filter(system => METRICS.some(config => config.system === system.id &&
    entries.some(entry => entry.metric === config.id && entry.latestMeasurement))).map(system => system.id)
  const selected = SYSTEMS.find(system => system.id === selectedSystem)
  const completedTasks = tasks?.filter(task => task.completedAt).length ?? 0
  const sourceSet = [...new Set(entries.flatMap(entry => entry.latestReceivedMeasurement ? [sourceNames[entry.latestReceivedMeasurement.source]] : []))]
  function openMetric(id: Metric) { setMetric(id); setTab('trends'); setCheckIn(false) }
  function afterSave() { refresh(); setRefreshVersion(value => value + 1) }
  function changeTab(next: Tab) { setTab(next); setCheckIn(false) }
  function openCheckIn() { setFormMetric(current => current ?? metric); setCheckIn(true) }

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
        <button aria-pressed={role === 'crew'} onClick={() => setRole('crew')}>Crew<span>Экипаж</span></button>
        <button aria-pressed={role === 'ground'} onClick={() => setRole('ground')}>Ground<span>Наземная команда</span></button>
      </div>
      <button className="primary-button header-checkin" onClick={openCheckIn}><Plus size={16} aria-hidden="true" /> Внести измерение</button>
    </header>
    <div className="workspace">
      {role === 'crew' ? <aside className="anatomy-sidebar" aria-label="Карта организма">
        <AnatomyPanel selected={selectedSystem} onSelect={selectSystem} availableSystems={availableSystems} />
        <div className="anatomy-footnote"><span className="legend-dot" /> Есть измерения <span className="legend-dot empty" /> Пока нет показателей</div>
      </aside> : null}
      <main id="main-content" className="main-content" tabIndex={-1}>
        <nav className="page-tabs" aria-label="Разделы">
          <button aria-current={tab === 'overview' && !checkIn ? 'page' : undefined} onClick={() => changeTab('overview')}>{role === 'crew' ? 'Обзор здоровья' : 'Наблюдение экипажа'}</button>
          <button aria-current={tab === 'trends' && !checkIn ? 'page' : undefined} onClick={() => changeTab('trends')}>Динамика</button>
          <button aria-current={tab === 'system' && !checkIn ? 'page' : undefined} onClick={() => changeTab('system')}>Система и связь</button>
        </nav>
        {!loading && !connected ? <div className="connection-banner" role="alert"><WifiOff size={18} aria-hidden="true" /><div><strong>Связь с сервером потеряна</strong><p>{lastUpdated ? `Показаны данные, полученные ${formatTime(lastUpdated.toISOString())}. Свежесть не подтверждена.` : 'Проверьте подключение и запущен ли сервер.'}</p></div><button className="secondary-button" onClick={refresh}>Повторить</button></div> : null}
        {formMetric ? <div hidden={!checkIn}><CheckInPanel initialMetric={formMetric} onSaved={afterSave} onClose={() => setCheckIn(false)} /></div> : null}
        {!checkIn ? <>
          <div className="page-heading"><div><p className="breadcrumb">{role === 'crew' ? 'Экипаж' : 'Наземная команда'} <ChevronRight size={13} aria-hidden="true" /> Демо-астронавт</p>
            <h1>{tab === 'trends' ? 'Динамика показателей' : tab === 'system' ? 'Система и связь' : role === 'crew' ? 'Обзор состояния' : 'Мониторинг экипажа'}</h1>
            <p className="heading-description">{tab === 'overview' ? 'Измерения, самочувствие и задачи в одном пространстве.' : tab === 'trends' ? 'Полная история за выбранный период. Пропуски остаются видимыми.' : 'Состояние подключения и доступные функции мониторинга.'}</p></div>
            <button className="icon-button" aria-label="Обновить данные" title="Обновить данные" onClick={() => { refresh(); setRefreshVersion(value => value + 1) }}><RefreshCw size={17} aria-hidden="true" /></button>
          </div>
          {tab === 'trends' ? <Suspense fallback={<div className="panel loading-panel">Загрузка графика…</div>}><TrendPanel metric={metric} onMetricChange={setMetric} refreshVersion={refreshVersion} onAdd={openCheckIn} /></Suspense> : tab === 'system' ? <>
            <div className="panel system-panel"><h2><Database size={18} aria-hidden="true" /> Состояние подключения</h2>
              <dl className="system-facts"><div><dt>Сервер</dt><dd>{loading ? 'Проверка…' : connected ? 'Доступен' : 'Недоступен'}</dd></div><div><dt>Последнее обновление</dt><dd>{lastUpdated ? formatTime(lastUpdated.toISOString()) : 'Ещё не получено'}</dd></div><div><dt>Хранение измерений</dt><dd>Локальная база данных</dd></div><div><dt>Источники</dt><dd>{sourceSet.join(', ') || 'Измерения пока не получены'}</dd></div></dl>
              <p className="note">Обновление каждые 5 секунд при открытой вкладке. После потери связи предыдущие данные остаются на экране.</p>
            </div>
            <div className="panel"><h2>Оценка последнего сообщения</h2><p>{assessment?.explanation ?? 'Оценка ещё не получена.'}</p><p className="note">Совместимая сводка одного последнего принятого сообщения. Свежесть каждого показателя отображается отдельно в обзоре; клинические правила пока не подключены.</p>{assessment ? <p className="note">Оценено сервером {formatTime(assessment.evaluatedAt)}.{!connected ? ' Это предыдущая оценка; актуальность не подтверждена.' : ''}</p> : null}</div>
            <div className="panel"><h2>Возможности прототипа</h2><p>Доступны один профиль, шесть показателей, история измерений и задачи. Поддержка нескольких членов экипажа, наземная передача сообщений и медицинские рекомендации находятся в разработке.</p></div>
          </> : role === 'ground' ? <>
            <Suspense fallback={<div className="loading-panel">Загрузка наземной консоли…</div>}><GroundConsole latest={latest} connected={connected} elapsedSeconds={freshnessElapsed} refreshVersion={refreshVersion} selectedSystem={selectedSystem} onSystemSelect={selectSystem} onOpenMetric={openMetric} channels={groundChannels} onChannelsChange={setGroundChannels} days={groundDays} onDaysChange={setGroundDays} /></Suspense>
            <TaskPanel tasks={tasks} connected={connected} onChanged={refresh} />
          </> : <>
            <div className="summary-grid">
              <section className="panel coverage-panel"><h2>Полнота наблюдений</h2><div className="coverage-content"><CoverageGauge count={knownCount} total={METRICS.length} /><div><strong>{loading ? 'Получаем измерения' : knownCount === 6 ? 'Все показатели записаны' : `Без записи с известным временем: ${METRICS.length - knownCount}`}</strong><p>С известным временем измерения</p><span className="neutral-tag">Не оценка здоровья</span></div></div></section>
              <section className="panel session-panel"><h2>Сеанс наблюдения</h2><p className="session-title">Демо-миссия</p><dl><div><dt>Источники</dt><dd>{sourceSet.join(', ') || 'Ожидание данных'}</dd></div><div><dt>Обновлено</dt><dd>{lastUpdated ? formatTime(lastUpdated.toISOString()) : '—'}</dd></div></dl><span className={`status-tag ${connected ? 'online' : ''}`}>{connected ? 'Мониторинг подключён' : loading ? 'Подключение' : 'Нет связи'}</span></section>
              <section className="panel routine-panel"><h2>Задачи</h2><div className="routine-value">{completedTasks}<small> / {tasks?.length ?? '—'}</small><Check size={22} aria-hidden="true" /></div><p className="note">Отмечено выполненными</p><div className="task-track"><span style={{ width: `${tasks?.length ? completedTasks / tasks.length * 100 : 0}%` }} /></div></section>
            </div>
            <div className="overview-columns">
              <section className="panel systems-panel"><div className="section-heading"><h2>Системы организма</h2><span className="note">Доступность данных</span></div><div className="system-list">{SYSTEMS.map(system => {
                const configs = METRICS.filter(config => config.system === system.id)
                const count = configs.filter(config => entries.some(entry => entry.metric === config.id && entry.latestKnownMeasurement)).length
                return <button className="system-row" key={system.id} onClick={() => setSelectedSystem(current => current === system.id ? null : system.id)} aria-pressed={selectedSystem === system.id}>
                  <span>{system.label}</span>
                  {configs.length ? <span className="system-segments" aria-hidden="true">
                    {Array.from({ length: 12 }, (_, index) => <span key={index} data-filled={index < count / configs.length * 12} />)}
                  </span> : <span className="system-unavailable">Не подключено</span>}
                  <span className="system-count">{configs.length ? `${count}/${configs.length}` : '—'}</span>
                </button>
              })}</div><p className="note">Выберите область на схеме, чтобы отфильтровать показатели.</p></section>
              <section className="panel attention-panel"><h2>О данных</h2>{unknownEntries.length ? <div className="data-notice"><strong>Время измерения неизвестно</strong><p>{unknownEntries.map(entry => METRICS.find(config => config.id === entry.metric)?.label).join(', ')}: последнее сообщение не содержит времени измерения. Оно не подтверждает свежесть.</p></div> : <div className="data-notice neutral"><strong>{loading ? 'Ожидание измерений' : knownCount ? 'Время измерений сохранено' : 'Начните с первого измерения'}</strong><p>{knownCount ? 'На карточках показаны последние значения с известным временем. В динамике можно посмотреть весь период.' : 'Запустите симулятор или внесите показания вручную.'}</p></div>}<p className="note">Клиническая оценка пока не подключена. Наличие данных не означает, что показатель в норме.</p></section>
            </div>
            <div className="section-heading metrics-heading"><div><h2>{selected?.label ?? 'Все показатели'}</h2>{selected ? <p className="note">{selected.description}</p> : null}</div>{selectedSystem ? <button className="text-button" onClick={() => setSelectedSystem(null)}>Показать все</button> : <span className="note">6 показателей</span>}</div>
            {latest ? <p className="quality-context note">Оценка сервера: {formatTime(latest.evaluatedAt)}. Сроки различаются по показателям; время доставки не подтверждает свежесть.{!connected ? ' Показан предыдущий ответ; свежесть не подтверждена.' : ''}</p> : null}
            <MetricOverview latest={latest} selectedSystem={selectedSystem} onSelect={openMetric} connected={connected} elapsedSeconds={freshnessElapsed} />
            <TaskPanel tasks={tasks} connected={connected} onChanged={refresh} />
          </>}
        </> : null}
        <footer className="page-footer"><CircleHelp size={14} aria-hidden="true" /><p>Демонстрационный прототип. Клинические правила не подключены; показания симулятора синтетические.</p></footer>
      </main>
    </div>
    <div className="statusbar"><span><span className={`context-dot ${connected ? '' : 'offline'}`} /> {connected ? 'Сервер подключён' : loading ? 'Подключение' : 'Сервер недоступен'}</span><span>Показатели {knownCount}/6</span><span>Задачи {completedTasks}/{tasks?.length ?? '—'}</span><span className="statusbar-end">AstroCheck · Space Health</span></div>
  </div>
}
