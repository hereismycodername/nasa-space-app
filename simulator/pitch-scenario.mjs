import { parseArgs } from 'node:util'
import { apiOrigin } from './config.mjs'

export const PITCH_STAGES = [
  { id: 'baseline', label: 'Стабильные показания', hint: 'Покажите живые значения и отдельные результаты правил. Шесть новых метрик пока без правил.' },
  { id: 'attention', label: 'Требуется внимание', hint: 'Покажите причины, следующие действия и влияние условий полёта.' },
  { id: 'critical', label: 'Срочный сигнал', hint: 'Покажите обращение к экипажу и инструкцию сервера. Это синтетический сценарий.' },
  { id: 'recovery', label: 'Повторные стабильные показания', hint: 'Новый набор данных заменяет прошлую оценку. Это монтаж сценария, а не результат лечения.' },
  { id: 'stale', label: 'Поток датчиков остановлен', hint: 'API остаётся доступен; через 15 секунд без новых отсчётов пульс устареет.' },
]

export const PITCH_METRICS = [
  { metric: 'heartRate', unit: 'bpm', sensor: true },
  { metric: 'systolicPressure', unit: 'mmHg', sensor: true },
  { metric: 'diastolicPressure', unit: 'mmHg', sensor: true },
  { metric: 'temperature', unit: '°C', sensor: true },
  { metric: 'sleepHours', unit: 'h', sensor: false },
  { metric: 'mood', unit: 'score', sensor: false },
  { metric: 'spo2', unit: '%', sensor: true },
  { metric: 'glucose', unit: 'mg/dL', sensor: true },
  { metric: 'visionBlur', unit: 'score', sensor: false },
  { metric: 'exerciseMinutes', unit: 'min', sensor: false },
  { metric: 'headache', unit: 'score', sensor: false },
  { metric: 'backPain', unit: 'score', sensor: false },
]

// These are presentation fixtures, not thresholds, diagnoses, or sensor drivers.
const stable = { heartRate: 72, systolicPressure: 118, diastolicPressure: 76, temperature: 36.7, sleepHours: 8, mood: 4, spo2: 98, glucose: 90, visionBlur: 0, exerciseMinutes: 90, headache: 0, backPain: 0 }
const profiles = {
  baseline: stable,
  attention: { heartRate: 110, systolicPressure: 138, diastolicPressure: 88, temperature: 38.2, sleepHours: 4.5, mood: 2, spo2: 94, glucose: 112, visionBlur: 2.5, exerciseMinutes: 30, headache: 4, backPain: 3 },
  critical: { heartRate: 124, systolicPressure: 190, diastolicPressure: 125, temperature: 38.5, sleepHours: 4.5, mood: 2, spo2: 92, glucose: 120, visionBlur: 3, exerciseMinutes: 30, headache: 6, backPain: 4 },
  recovery: stable,
}
const amplitude = { heartRate: 1.5, systolicPressure: 0.8, diastolicPressure: 0.6, temperature: 0.03, spo2: 0.3, glucose: 1.2 }

/** Complete fixture frame. Callers publish self-reports only at scene entry. */
export function stageReadings(stage, elapsedSeconds, fromStage = stage, transitionSeconds = 8) {
  if (!PITCH_STAGES.some(item => item.id === stage)) throw new Error(`Unknown pitch stage: ${stage}`)
  if (!Number.isFinite(elapsedSeconds) || elapsedSeconds < 0 || !Number.isFinite(transitionSeconds) || transitionSeconds <= 0) throw new Error('Pitch scene times must be finite and nonnegative')
  if (stage === 'stale') return []
  const target = profiles[stage]
  const source = typeof fromStage === 'string' ? profiles[fromStage] ?? target : fromStage
  const linear = Math.min(1, elapsedSeconds / transitionSeconds)
  const blend = linear * linear * (3 - 2 * linear)
  return PITCH_METRICS.map(({ metric, unit, sensor }, index) => {
    const starting = Number.isFinite(source?.[metric]) ? source[metric] : target[metric]
    const value = sensor
      ? starting + (target[metric] - starting) * blend + amplitude[metric] * Math.sin(elapsedSeconds / 2.7 + index * 0.8)
      : target[metric]
    return { metric, unit, value: Math.round(value * 100) / 100, sensor }
  })
}

function integer(value, fallback, label, minimum, maximum) {
  const raw = value ?? String(fallback)
  const number = Number(raw)
  if (!/^\d+$/.test(raw) || !Number.isSafeInteger(number) || number < minimum || number > maximum) throw new Error(`${label}: ожидается целое число ${minimum}…${maximum}`)
  return number
}

export function pitchConfig(args = [], env = process.env) {
  const { values, tokens } = parseArgs({ args, allowPositionals: false, strict: true, tokens: true, options: {
    'api-url': { type: 'string' }, 'stage-seconds': { type: 'string' },
    'interval-ms': { type: 'string' }, cycles: { type: 'string' },
    manual: { type: 'boolean' }, help: { type: 'boolean' },
  } })
  const seen = new Set()
  for (const token of tokens) if (token.kind === 'option') {
    if (seen.has(token.name)) throw new Error(`Повтор параметра --${token.name}`)
    seen.add(token.name)
  }
  const config = {
    help: values.help ?? false,
    base: apiOrigin(values['api-url'] ?? env.API_URL),
    stageSeconds: integer(values['stage-seconds'], 30, '--stage-seconds', 1, 3600),
    interval: integer(values['interval-ms'], 2000, '--interval-ms', 100, 60_000),
    cycles: integer(values.cycles, 1, '--cycles', 0, 1_000_000),
    manual: values.manual ?? false,
    retry: integer(env.SIM_RETRY_MS, 1000, 'SIM_RETRY_MS', 100, 60_000),
    timeout: integer(env.SIM_REQUEST_TIMEOUT_MS, 5000, 'SIM_REQUEST_TIMEOUT_MS', 100, 60_000),
  }
  if (config.stageSeconds * 1000 < 2 * config.interval) throw new Error('--stage-seconds должен вмещать не менее двух --interval-ms: иначе сцена не успеет передать целевые значения')
  return config
}

export function stageDuration(stage, seconds) {
  return stage === 'stale' ? Math.max(30, seconds) : seconds
}
