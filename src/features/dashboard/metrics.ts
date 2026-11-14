import type { Metric, Unit } from '../../shared/types'

export type BodySystem = 'brain' | 'eyes' | 'heart' | 'lungs' | 'gut' | 'msk' | 'immune'

interface MetricConfig {
  id: Metric
  label: string
  unit: Unit
  system: BodySystem
  description: string
  input: { min: number; max?: number; step: number }
}

export const METRICS: readonly MetricConfig[] = [
  { id: 'heartRate', label: 'Пульс', unit: 'bpm', system: 'heart', description: 'Частота сердечных сокращений', input: { min: 0, step: 0.1 } },
  { id: 'systolicPressure', label: 'Систолическое давление', unit: 'mmHg', system: 'heart', description: 'Верхнее значение артериального давления', input: { min: 0, step: 0.1 } },
  { id: 'diastolicPressure', label: 'Диастолическое давление', unit: 'mmHg', system: 'heart', description: 'Нижнее значение артериального давления', input: { min: 0, step: 0.1 } },
  { id: 'temperature', label: 'Температура', unit: '°C', system: 'immune', description: 'Температура тела', input: { min: 0, step: 0.1 } },
  { id: 'sleepHours', label: 'Сон', unit: 'h', system: 'brain', description: 'Продолжительность сна', input: { min: 0, max: 24, step: 0.1 } },
  { id: 'mood', label: 'Самочувствие', unit: 'score', system: 'brain', description: 'Субъективная оценка от 1 до 5', input: { min: 1, max: 5, step: 1 } },
]

export const SYSTEMS: readonly { id: BodySystem; label: string; description: string }[] = [
  { id: 'brain', label: 'Сон и самочувствие', description: 'Продолжительность сна и личная оценка самочувствия.' },
  { id: 'heart', label: 'Сердце и кровообращение', description: 'Пульс и оба показателя артериального давления.' },
  { id: 'immune', label: 'Температура тела', description: 'Измерения температуры. Они сами по себе не определяют состояние иммунной системы.' },
  { id: 'lungs', label: 'Дыхательная система', description: 'Показатели дыхания пока не подключены.' },
  { id: 'eyes', label: 'Зрение', description: 'Показатели зрения пока не подключены.' },
  { id: 'gut', label: 'Пищеварение и обмен веществ', description: 'Показатели питания и обмена веществ пока не подключены.' },
  { id: 'msk', label: 'Кости и мышцы', description: 'Показатели костной и мышечной систем пока не подключены.' },
]

export const metricConfig = (metric: Metric) => METRICS.find(item => item.id === metric)!

const valueFormatter = new Intl.NumberFormat('ru-RU', { maximumFractionDigits: 2 })
const timeFormatter = new Intl.DateTimeFormat('ru-RU', {
  day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit',
})

export function formatValue(value: number) {
  return valueFormatter.format(value)
}

export function formatTime(timestamp: string) {
  const date = new Date(timestamp)
  // Stored RFC3339 may include a leap second or an offset accepted by Rust but not Date.
  return Number.isFinite(date.getTime()) ? timeFormatter.format(date) : timestamp
}
