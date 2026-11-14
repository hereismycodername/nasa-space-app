import { useEffect, useState } from 'react'
import { getAssessment, getLatestMeasurements, getMeasurementSeries, saveMeasurement } from './data/measurements'
import { MeasurementSeriesChart } from './MeasurementSeriesChart'
import type { HealthAssessment, LatestMeasurements, MeasurementSeries, Metric } from './shared/types'

const metricNames: { metric: Metric; label: string }[] = [
  { metric: 'heartRate', label: 'Пульс' },
  { metric: 'systolicPressure', label: 'Систолическое давление' },
  { metric: 'diastolicPressure', label: 'Диастолическое давление' },
  { metric: 'temperature', label: 'Температура' },
  { metric: 'sleepHours', label: 'Сон' },
  { metric: 'mood', label: 'Самочувствие' },
]

export function MetricOverview({ latest }: { latest?: LatestMeasurements }) {
  return <div className="metric-grid">
    {metricNames.map(({ metric, label }) => {
      const entry = latest?.metrics.find(row => row.metric === metric)
      const known = entry?.latestKnownMeasurement
      const received = entry?.latestReceivedMeasurement
      const unknown = received?.measuredAt === null ? received : undefined
      return <article className="metric-card" key={metric}>
        <h3>{label}</h3>
        {known ? <>
          <p className="metric-value">{known.value} {known.unit}</p>
          <p className="note">Измерено {new Date(known.measuredAt).toLocaleString()}</p>
          <p className="note">Источник: {known.source}</p>
        </> : <p>{latest ? (unknown ? 'Нет измерения с известным временем' : 'Нет данных') : 'Загрузка…'}</p>}
        {unknown ? <p className="unknown-time">
          Получено {new Date(unknown.receivedAt).toLocaleString()}: {unknown.value} {unknown.unit}.
          {' '}Время измерения неизвестно; свежесть не подтверждена.
        </p> : null}
      </article>
    })}
  </div>
}

export default function App() {
  const [series, setSeries] = useState<MeasurementSeries>()
  const [latest, setLatest] = useState<LatestMeasurements>()
  const [assessment, setAssessment] = useState<HealthAssessment>()
  const [connected, setConnected] = useState(false)
  const [message, setMessage] = useState('')
  const [saving, setSaving] = useState(false)
  useEffect(() => {
    let active = true
    let timer: ReturnType<typeof setTimeout>
    async function poll() {
      try {
        const to = new Date()
        const from = new Date(to.getTime() - 7 * 24 * 60 * 60 * 1000)
        const [history, overview, result] = await Promise.all([
          getMeasurementSeries({ metric: 'heartRate', from: from.toISOString(), to: to.toISOString(), maxPoints: 168 }),
          getLatestMeasurements(), getAssessment(),
        ])
        if (active) { setSeries(history); setLatest(overview); setAssessment(result); setConnected(true) }
      } catch { if (active) setConnected(false) }
      finally { if (active) timer = setTimeout(poll, 2000) }
    }
    void poll()
    return () => { active = false; clearTimeout(timer) }
  }, [])
  async function addManualSample() {
    setSaving(true)
    try {
      // UUID fallback allows local HTTP access from another LAN device.
      const sessionId = Array.from(crypto.getRandomValues(new Uint32Array(4)), x => x.toString(16)).join('-')
      await saveMeasurement({ deviceId: 'manual-01', sessionId, sequence: 0,
        metric: 'heartRate', value: 70, unit: 'bpm', measuredAt: new Date().toISOString() })
      setMessage('Тестовое ручное измерение сохранено в SQLite.')
    } catch { setMessage('Сохранить не удалось. Проверьте Rust-сервер.') }
    finally { setSaving(false) }
  }
  return <main>
    <p className="eyebrow">SPACE HEALTH · RUST + REACT</p>
    <h1>Поток измерений</h1>
    <p role="status">{connected ? 'API подключён · SQLite' : 'API недоступен · показаны последние полученные данные'}</p>
    <section>
      <h2>Обзор показателей</h2>
      <p className="note">Последние измерения с известным временем. Полученные записи без времени показаны отдельно.</p>
      <MetricOverview latest={latest} />
    </section>
    <section>
      <h2>Последнее принятое сообщение</h2>
      <p className="note">Эта оценка относится к одному сообщению, а не ко всем показателям обзора.</p>
      <p>{connected ? assessment?.explanation : 'Связь с сервером потеряна. Свежесть показателей не подтверждена.'}</p>
    </section>
    <section>
      <h2>История пульса за 7 дней</h2>
      <MeasurementSeriesChart series={series} />
      <button onClick={addManualSample} disabled={saving}>{saving ? 'Сохранение…' : 'Добавить тестовые 70 bpm вручную'}</button>
      <p role="status">{message}</p>
    </section>
    <p className="note">Демонстрационный стенд. Все показания синтетические; медицинские правила не подключены.</p>
  </main>
}
