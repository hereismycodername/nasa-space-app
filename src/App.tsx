import { useEffect, useState } from 'react'
import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { getAssessment, getHistory, saveMeasurement } from './data/measurements'
import type { HealthAssessment, Measurement } from './shared/types'

export default function App() {
  const [history, setHistory] = useState<Measurement[]>([])
  const [assessment, setAssessment] = useState<HealthAssessment>()
  const [connected, setConnected] = useState(false)
  const [message, setMessage] = useState('')
  const [saving, setSaving] = useState(false)
  useEffect(() => {
    let active = true
    let timer: ReturnType<typeof setTimeout>
    async function poll() {
      try {
        const [rows, result] = await Promise.all([getHistory(), getAssessment()])
        if (active) { setHistory(rows.items); setAssessment(result); setConnected(true) }
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
  const pulse = history.filter(row => row.metric === 'heartRate')
  const displayedPulse = pulse.slice(0, 60)
  const chart = [...displayedPulse].reverse().map(row => ({
    time: `${new Date(row.measuredAt ?? row.receivedAt).toLocaleTimeString()}${row.timeBasis === 'receivedAt' ? ' *' : ''}`, value: row.value,
  }))
  return <main>
    <p className="eyebrow">SPACE HEALTH · RUST + REACT</p>
    <h1>Поток измерений</h1>
    <p role="status">{connected ? 'API подключён · SQLite' : 'API недоступен · показаны последние полученные данные'}</p>
    <section>
      <h2>Пульс: {pulse[0] ? `${pulse[0].value} bpm` : 'нет данных'}</h2>
      <p>{connected ? assessment?.explanation : 'Связь с сервером потеряна. Свежесть показателей не подтверждена.'}</p>
      {pulse[0] && <p>Источник: {pulse[0].source} · получено {new Date(pulse[0].receivedAt).toLocaleTimeString()}</p>}
      <div style={{ height: 240, marginTop: 24 }}>
        {chart.length > 0 ? <ResponsiveContainer width="100%" height="100%">
          <LineChart data={chart}><CartesianGrid stroke="#344755" /><XAxis dataKey="time" minTickGap={40} />
            <YAxis domain={['auto', 'auto']} unit=" bpm" /><Tooltip /><Line dataKey="value" stroke="#b5edda" dot={false} isAnimationActive={false} />
          </LineChart>
        </ResponsiveContainer> : <p>Запустите npm run simulator в отдельном терминале.</p>}
      </div>
      {displayedPulse.some(row => row.timeBasis === 'receivedAt') && <p className="note">* Показано время получения: время самого измерения неизвестно, его свежесть не подтверждена.</p>}
      <button onClick={addManualSample} disabled={saving}>{saving ? 'Сохранение…' : 'Добавить тестовые 70 bpm вручную'}</button>
      <p role="status">{message}</p>
    </section>
    <p className="note">Демонстрационный стенд. Все показания синтетические; медицинские правила не подключены.</p>
  </main>
}
