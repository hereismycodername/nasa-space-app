import type { DataQuality, FreshnessStatus, LatestMeasurements } from '../../shared/types'

export const freshnessLabels: Record<FreshnessStatus, string> = {
  missing: 'Нет измерения',
  fresh: 'Свежее измерение',
  stale: 'Ожидается новый замер',
  unknownTime: 'Время неизвестно',
  clockSkew: 'Расхождение часов',
}

/** Only expire a server-confirmed fresh result; never promote uncertain or stale data locally. */
export function currentFreshness(quality: DataQuality, elapsedSeconds = 0): FreshnessStatus {
  if (quality.clockSkew) return 'clockSkew'
  if (quality.status !== 'fresh') return quality.status
  if (quality.ageSeconds === null || !Number.isFinite(quality.ageSeconds)) return 'unknownTime'
  return quality.ageSeconds + Math.max(0, elapsedSeconds) > quality.freshForSeconds ? 'stale' : 'fresh'
}

export function formatFreshnessWindow(seconds: number) {
  if (seconds % 86400 === 0) return `${seconds / 86400} сут.`
  if (seconds % 3600 === 0) return `${seconds / 3600} ч`
  if (seconds % 60 === 0) return `${seconds / 60} мин`
  return `${seconds} с`
}

export function formatMeasurementAge(seconds: number | null) {
  if (seconds === null || !Number.isFinite(seconds)) return 'Время последнего замера неизвестно'
  const age = Math.max(0, seconds)
  const relative = age < 1 ? 'только что' : age < 60 ? `${Math.floor(age)} с назад` : age < 3600 ? `${Math.floor(age / 60)} мин назад`
    : age < 86400 ? `${Math.floor(age / 3600)} ч назад` : `${Math.floor(age / 86400)} дн. назад`
  return `Последний замер ${relative}`
}

export function freshnessLabel(quality: DataQuality, elapsedSeconds = 0) {
  const status = currentFreshness(quality, elapsedSeconds)
  return status === 'stale'
    ? `${freshnessLabels.stale}. ${formatMeasurementAge(quality.ageSeconds === null ? null : quality.ageSeconds + Math.max(0, elapsedSeconds))}`
    : freshnessLabels[status]
}

export function qualityDescription(quality: DataQuality, elapsedSeconds = 0) {
  const status = currentFreshness(quality, elapsedSeconds)
  const window = formatFreshnessWindow(quality.freshForSeconds)
  switch (status) {
    case 'missing': return 'Измерений этого показателя пока нет.'
    case 'fresh': return `Время измерения укладывается в демо-срок ${window}. Это не оценка здоровья.`
    case 'stale': return `${freshnessLabel(quality, elapsedSeconds)}. Демо-срок свежести ${window} истёк. Это характеристика данных, не оценка здоровья. Недавняя доставка не обновляет время измерения.`
    case 'unknownTime': return 'Время измерения неизвестно; время доставки не подтверждает свежесть.'
    case 'clockSkew': return 'Время измерения опережает часы сервера. Свежесть не подтверждена.'
  }
}

/** Schedule one update at the next expiry, rather than re-rendering the dashboard every second. */
export function nextFreshnessExpiry(latest: LatestMeasurements, elapsedSeconds: number): number | undefined {
  const remaining = latest.metrics.flatMap(entry => [entry.quality, entry.latestReceivedQuality])
    .filter(quality => quality?.status === 'fresh' && !quality.clockSkew && quality.ageSeconds !== null)
    .map(quality => quality.freshForSeconds - quality.ageSeconds! - Math.max(0, elapsedSeconds))
    .filter(seconds => seconds >= 0)
  return remaining.length ? Math.ceil(Math.min(...remaining) * 1000) + 1 : undefined
}
