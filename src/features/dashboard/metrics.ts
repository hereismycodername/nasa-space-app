import type { LatestMeasurements, Metric, Unit } from '../../shared/types'

export type BodySystem = 'brain' | 'eyes' | 'heart' | 'lungs' | 'gut' | 'msk' | 'immune'

interface MetricConfig {
  id: Metric
  label: string
  unit: Unit
  system: BodySystem
  description: string
  input: { min: number; max?: number; step: number; exclusiveMin?: boolean }
  quickInputLabel?: string
}

export const METRICS: readonly MetricConfig[] = [
  { id: 'heartRate', label: 'Пульс', unit: 'bpm', system: 'heart', description: 'Частота сердечных сокращений', input: { min: 0, step: 0.1, exclusiveMin: true } },
  { id: 'systolicPressure', label: 'Систолическое давление', unit: 'mmHg', system: 'heart', description: 'Верхнее значение артериального давления', input: { min: 0, step: 0.1, exclusiveMin: true } },
  { id: 'diastolicPressure', label: 'Диастолическое давление', unit: 'mmHg', system: 'heart', description: 'Нижнее значение артериального давления', input: { min: 0, step: 0.1, exclusiveMin: true } },
  { id: 'temperature', label: 'Температура', unit: '°C', system: 'immune', description: 'Температура тела', input: { min: 0, step: 0.1, exclusiveMin: true } },
  { id: 'sleepHours', label: 'Сон', unit: 'h', system: 'brain', description: 'Продолжительность сна', input: { min: 0, max: 24, step: 0.1 } },
  { id: 'mood', label: 'Самочувствие', unit: 'score', system: 'brain', description: 'Субъективная оценка от 1 до 5', input: { min: 1, max: 5, step: 1 } },
  { id: 'spo2', label: 'Насыщение крови кислородом', unit: '%', system: 'lungs', description: 'SpO₂ по показанию пульсоксиметра, от 0 до 100%.', input: { min: 0, max: 100, step: 0.1 } },
  { id: 'glucose', label: 'Глюкоза крови', unit: 'mg/dL', system: 'gut', description: 'Значение глюкометра в мг/дл; время измерения сохраняется отдельно.', input: { min: 0, step: 0.1, exclusiveMin: true } },
  { id: 'visionBlur', label: 'Нечёткость зрения', unit: 'score', system: 'eyes', description: 'Личная оценка от 0 (нет) до 10 (максимальная выраженность). Это не проверка остроты зрения.', input: { min: 0, max: 10, step: 0.1 }, quickInputLabel: 'Отметить нечёткость зрения' },
  { id: 'exerciseMinutes', label: 'Активность за 24 часа', unit: 'min', system: 'msk', description: 'Суммарное время упражнений за последние 24 часа, от 0 до 1440 минут. Укажите время отчёта.', input: { min: 0, max: 1440, step: 0.1 } },
  { id: 'headache', label: 'Головная боль', unit: 'score', system: 'brain', description: 'Личная оценка от 0 (боли нет) до 10 (максимальная выраженность). По шкале нельзя установить причину.', input: { min: 0, max: 10, step: 0.1 }, quickInputLabel: 'Отметить головную боль' },
  { id: 'backPain', label: 'Боль в спине', unit: 'score', system: 'msk', description: 'Личная оценка от 0 (боли нет) до 10 (максимальная выраженность). По шкале нельзя установить причину.', input: { min: 0, max: 10, step: 0.1 }, quickInputLabel: 'Отметить боль в спине' },
]

export const SYSTEMS: readonly { id: BodySystem; label: string; description: string }[] = [
  { id: 'brain', label: 'Сон, самочувствие и голова', description: 'Продолжительность сна, самочувствие и личная оценка головной боли.' },
  { id: 'heart', label: 'Сердце и кровообращение', description: 'Пульс и оба показателя артериального давления.' },
  { id: 'immune', label: 'Температура тела', description: 'Измерения температуры. Они сами по себе не определяют состояние иммунной системы.' },
  { id: 'lungs', label: 'Дыхательная система', description: 'Насыщение крови кислородом по данным пульсоксиметра.' },
  { id: 'eyes', label: 'Зрение', description: 'Личная оценка нечёткости зрения; она не заменяет осмотр.' },
  { id: 'gut', label: 'Пищеварение и обмен веществ', description: 'Измерения глюкозы крови. Причину изменения показатель сам по себе не определяет.' },
  { id: 'msk', label: 'Кости, мышцы и спина', description: 'Активность за последние 24 часа и личная оценка боли в спине. Плотность костей этими показателями не измеряется.' },
]

export const metricConfig = (metric: Metric) => METRICS.find(item => item.id === metric)!

/** Presence of a metric row advertises API support even when its measurement is null. */
export function supportedMetricConfigs(latest?: LatestMeasurements) {
  const supported = new Set(latest?.metrics.map(entry => entry.metric) ?? [])
  return METRICS.filter(metric => supported.has(metric.id))
}

const valueFormatter = new Intl.NumberFormat('ru-RU', { maximumFractionDigits: 2 })
const timeFormatter = new Intl.DateTimeFormat('ru-RU', {
  day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit',
})

export function formatValue(value: number) {
  return valueFormatter.format(value)
}

const unitLabels: Record<Unit, string> = {
  bpm: 'уд/мин', mmHg: 'мм рт. ст.', '°C': '°C', h: 'ч', score: 'балла',
  '%': '%', 'mg/dL': 'мг/дл', min: 'мин',
}

/** Presentation only: API units remain unchanged in requests and responses. */
export function formatUnit(unit: Unit, metric?: Metric) {
  if (unit === 'score' && metric) return metric === 'mood' ? 'из 5' : 'из 10'
  return unitLabels[unit]
}

export function formatTime(timestamp: string) {
  const date = new Date(timestamp)
  // Stored RFC3339 may include a leap second or an offset accepted by Rust but not Date.
  return Number.isFinite(date.getTime()) ? timeFormatter.format(date) : timestamp
}
