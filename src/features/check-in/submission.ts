import { ApiError, saveMeasurement } from '../../data/measurements'
import { METRICS } from '../dashboard/metrics'
import type { Measurement, Metric, NewMeasurement } from '../../shared/types'

export interface CheckInDraft {
  metric: Metric
  value: string
  localTime: string
  unknownTime: boolean
}

export type MeasurementInput = Pick<NewMeasurement, 'metric' | 'value' | 'unit' | 'measuredAt'>

export function localDateTime(date = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
}

function measurementTime(value: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$/.exec(value)
  const date = new Date(value)
  if (!match || !Number.isFinite(date.getTime()) || [
    date.getFullYear(), date.getMonth() + 1, date.getDate(), date.getHours(), date.getMinutes(), date.getSeconds(),
  ].some((part, index) => part !== Number(match[index + 1] ?? 0))) {
    throw new Error('Укажите существующие дату и время измерения.')
  }
  return date.toISOString()
}

export function parseCheckIn(draft: CheckInDraft): MeasurementInput {
  const config = METRICS.find(metric => metric.id === draft.metric)
  if (!config) throw new Error('Выберите показатель.')
  const raw = draft.value.trim()
  const value = Number(raw.replace(',', '.'))
  if (!raw || !Number.isFinite(value)) throw new Error('Введите числовое значение показателя.')
  if (draft.metric === 'sleepHours' && (value < 0 || value > 24)) {
    throw new Error('Продолжительность сна должна быть от 0 до 24 часов.')
  }
  if (draft.metric === 'mood' && (value < 1 || value > 5)) {
    throw new Error('Самочувствие оценивается от 1 до 5.')
  }
  if (!['sleepHours', 'mood'].includes(draft.metric) && value <= 0) {
    throw new Error('Значение должно быть больше нуля.')
  }
  return {
    metric: draft.metric,
    unit: config.unit,
    value,
    measuredAt: draft.unknownTime ? null : measurementTime(draft.localTime),
  }
}

function newSessionId(): string {
  // getRandomValues also works when the station is accessed over plain LAN HTTP.
  return Array.from(crypto.getRandomValues(new Uint32Array(4)), part => part.toString(16).padStart(8, '0')).join('-')
}

/** Retain an unconfirmed payload so transport retries cannot create a second row. */
export function createMeasurementSaver(
  save: (payload: NewMeasurement) => Promise<Measurement> = saveMeasurement,
  createSession: () => string = newSessionId,
) {
  let unconfirmed: { fingerprint: string; payload: NewMeasurement } | undefined
  let inFlight: Promise<Measurement> | undefined
  return (input: MeasurementInput): Promise<Measurement> => {
    const fingerprint = JSON.stringify(input)
    if (inFlight) return unconfirmed?.fingerprint === fingerprint
      ? inFlight : Promise.reject(new Error('Дождитесь завершения текущей отправки.'))
    if (unconfirmed?.fingerprint !== fingerprint) {
      unconfirmed = {
        fingerprint,
        payload: { ...input, deviceId: 'manual-01', sessionId: createSession(), sequence: 0 },
      }
    }
    // The exact same object/key is retried, including measuredAt, after a lost reply.
    const payload = unconfirmed.payload
    inFlight = Promise.resolve().then(() => save(payload)).then(result => {
      unconfirmed = undefined
      return result
    }).finally(() => { inFlight = undefined })
    return inFlight
  }
}

export function isUnconfirmedSave(error: unknown): boolean {
  return !(error instanceof ApiError) || error.status >= 500
}
