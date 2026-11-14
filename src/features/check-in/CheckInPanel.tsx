import { useRef, useState } from 'react'
import type { FormEvent } from 'react'
import { METRICS } from '../dashboard/metrics'
import type { Metric } from '../../shared/types'
import { createMeasurementSaver, isUnconfirmedSave, localDateTime, parseCheckIn } from './submission'
import './forms.css'

interface CheckInPanelProps {
  initialMetric?: Metric
  onSaved: () => void
  onClose: () => void
}

export function CheckInPanel({ initialMetric = 'heartRate', onSaved, onClose }: CheckInPanelProps) {
  const [metric, setMetric] = useState<Metric>(initialMetric)
  const [value, setValue] = useState('')
  const [localTime, setLocalTime] = useState(localDateTime)
  const [unknownTime, setUnknownTime] = useState(false)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  const [unconfirmed, setUnconfirmed] = useState(false)
  const [success, setSuccess] = useState('')
  const saver = useRef<ReturnType<typeof createMeasurementSaver> | null>(null)
  if (saver.current === null) saver.current = createMeasurementSaver()
  const submitMeasurement = saver.current
  const config = METRICS.find(item => item.id === metric) ?? METRICS[0]

  function edited() {
    setError('')
    setSuccess('')
    setUnconfirmed(false)
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (pending) return
    setSuccess('')
    setError('')
    let input
    try {
      input = parseCheckIn({ metric, value, localTime, unknownTime })
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Проверьте введённые данные.')
      return
    }
    setPending(true)
    try {
      const saved = await submitMeasurement(input)
      setSuccess(`${config.label}: ${saved.value} ${saved.unit}. Измерение сохранено.`)
      setValue('')
      setLocalTime(localDateTime())
      setUnconfirmed(false)
      onSaved()
    } catch (reason) {
      const uncertain = isUnconfirmedSave(reason)
      setUnconfirmed(uncertain)
      setError(uncertain
        ? 'Нет подтверждения сохранения. Повторите отправку, чтобы проверить результат.'
        : `Измерение не сохранено. ${reason instanceof Error ? reason.message : 'Проверьте данные.'}`)
    } finally {
      setPending(false)
    }
  }

  return <section className="panel checkin-panel" aria-labelledby="checkin-title">
    <div className="checkin-heading">
      <div><h2 id="checkin-title">Добавить измерение</h2><p className="form-hint">Значение будет сохранено в журнале экипажа.</p></div>
      <button type="button" className="secondary-button" disabled={pending} onClick={onClose}>Закрыть</button>
    </div>
    <form onSubmit={submit} aria-busy={pending}>
      <fieldset disabled={pending} className="checkin-fields">
        <div className="form-field">
          <label htmlFor="checkin-metric">Показатель</label>
          <select id="checkin-metric" value={metric} onChange={event => { setMetric(event.target.value as Metric); setValue(''); edited() }}>
            {METRICS.map(item => <option key={item.id} value={item.id}>{item.label}</option>)}
          </select>
        </div>
        <div className="form-field">
          <label htmlFor="checkin-value">Значение · {config.unit}</label>
          <input id="checkin-value" name="value" type="number" inputMode="decimal" value={value}
            min={config.input.min} max={config.input.max} step="any" required
            placeholder={metric === 'mood' ? 'От 1 до 5' : metric === 'sleepHours' ? 'От 0 до 24' : 'Введите значение'}
            onChange={event => { setValue(event.target.value); edited() }} />
          <span className="form-hint">{config.description}</span>
        </div>
        <div className="form-field form-field-wide">
          <label htmlFor="checkin-time">Когда измерено · местное время</label>
          <input id="checkin-time" name="measuredAt" type="datetime-local" step="1" value={localTime}
            disabled={unknownTime || pending} required={!unknownTime}
            onChange={event => { setLocalTime(event.target.value); edited() }} />
          <label className="form-checkbox" htmlFor="checkin-unknown-time">
            <input id="checkin-unknown-time" type="checkbox" checked={unknownTime}
              onChange={event => { setUnknownTime(event.target.checked); edited() }} />
            Время измерения неизвестно
          </label>
          {unknownTime ? <span className="form-hint">Запись сохранится без времени измерения. Свежесть этих данных не подтверждена.</span> : null}
        </div>
      </fieldset>
      {error ? <p className="form-error" role="alert">{error}</p> : null}
      {success ? <p className="form-success" role="status">{success}</p> : null}
      <div className="form-actions">
        <button type="submit" className="primary-button" disabled={pending}>
          {pending ? 'Сохраняем…' : unconfirmed ? 'Повторить отправку' : 'Сохранить измерение'}
        </button>
        <span className="form-hint">Указывайте время самого измерения, даже если записываете его позже.</span>
      </div>
    </form>
  </section>
}
