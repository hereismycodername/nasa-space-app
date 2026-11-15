import { expect, it } from 'vitest'
import type { DataQuality, LatestMeasurements } from '../../shared/types'
import { currentFreshness, formatMeasurementAge, freshnessLabel, nextFreshnessExpiry, qualityDescription } from './freshness'
import { formatUnit, formatValue } from './metrics'
import type { Metric, Unit } from '../../shared/types'

const pulse: DataQuality = { status: 'fresh', measurementId: 'pulse', ageSeconds: 14, freshForSeconds: 15, clockSkew: false }
const sleep: DataQuality = { ...pulse, measurementId: 'sleep', freshForSeconds: 86400 }

it('formats relative measurement ages and display units in Russian without changing wire units', () => {
  for (const [seconds, expected] of [[0, 'только что'], [16, '16 с назад'], [59, '59 с назад'], [60, '1 мин назад'], [3599, '59 мин назад'], [3600, '1 ч назад'], [18000, '5 ч назад'], [86400, '1 дн. назад'], [172800, '2 дн. назад']] as const) {
    expect(formatMeasurementAge(seconds)).toBe(`Последний замер ${expected}`)
  }
  expect(formatMeasurementAge(null)).toBe('Время последнего замера неизвестно')
  const cases: [number, Unit, Metric | undefined, string][] = [
    [2, 'score', 'mood', '2 из 5'], [2, 'score', 'headache', '2 из 10'], [3, 'score', 'visionBlur', '3 из 10'],
    [2, 'score', undefined, '2 балла'], [4.14, 'h', 'sleepHours', '4,14 ч'], [72, 'bpm', 'heartRate', '72 уд/мин'],
    [115, 'mmHg', 'systolicPressure', '115 мм рт. ст.'], [15, 'min', 'exerciseMinutes', '15 мин'],
    [90.5, 'mg/dL', 'glucose', '90,5 мг/дл'], [36.7, '°C', 'temperature', '36,7 °C'], [98, '%', 'spo2', '98 %'],
  ]
  for (const [value, unit, metric, expected] of cases) expect(`${formatValue(value)} ${formatUnit(unit, metric)}`).toBe(expected)
  expect(qualityDescription({ ...pulse, status: 'stale', freshForSeconds: 46800, ageSeconds: 50400 })).toContain('Демо-срок свежести 13 ч истёк')
})

it('makes a stopped pulse stream visibly stale before one minute without turning freshness into a health alert', () => {
  expect(freshnessLabel(pulse, 1)).toBe('Свежее измерение')
  for (const quality of [pulse, { ...pulse, status: 'stale' as const, ageSeconds: 16 }]) {
    const label = freshnessLabel(quality, quality.status === 'fresh' ? 2 : 0)
    expect(label).toBe('Ожидается новый замер. Последний замер 16 с назад')
    expect(label).not.toContain('только что')
    expect(qualityDescription(quality, quality.status === 'fresh' ? 2 : 0)).toContain('не оценка здоровья')
  }
  expect(freshnessLabel({ ...pulse, status: 'stale', ageSeconds: null })).toBe('Ожидается новый замер. Время последнего замера неизвестно')
})

it('ages server-confirmed freshness independently of the client calendar and at the TTL boundary', () => {
  expect(currentFreshness(pulse, 1)).toBe('fresh')
  expect(currentFreshness(pulse, 1.001)).toBe('stale')
  expect(currentFreshness(sleep, 1.001)).toBe('fresh')
  expect(currentFreshness(pulse, -10)).toBe('fresh')
})

it.each(['missing', 'stale', 'unknownTime', 'clockSkew'] as const)('never promotes a server %s decision locally', status => {
  expect(currentFreshness({ ...pulse, status }, 864000)).toBe(status)
})

it('does not infer freshness when the age is unavailable or clocks disagree', () => {
  expect(currentFreshness({ ...pulse, ageSeconds: null })).toBe('unknownTime')
  expect(currentFreshness({ ...pulse, clockSkew: true })).toBe('clockSkew')
  expect(qualityDescription({ ...pulse, status: 'clockSkew', clockSkew: true })).toContain('опережает часы сервера')
})

it('schedules the earliest expiry including a separate last receipt, then leaves expired data alone', () => {
  const latest: LatestMeasurements = { astronautId: 'demo-astronaut', evaluatedAt: '2026-11-14T08:00:00Z', metrics: [
    { metric: 'sleepHours', latestKnownMeasurement: null, latestMeasurement: null, latestReceivedMeasurement: null, quality: sleep, latestReceivedQuality: pulse },
  ] }
  expect(nextFreshnessExpiry(latest, 0)).toBe(1001)
  expect(nextFreshnessExpiry(latest, 1)).toBe(1)
  expect(nextFreshnessExpiry(latest, 2)).toBe((86400 - 14 - 2) * 1000 + 1)
  expect(nextFreshnessExpiry(latest, 86400)).toBeUndefined()
})

it('does not schedule an automatic promotion of missing, unknown or clock-skewed data', () => {
  const latest: LatestMeasurements = { astronautId: 'demo-astronaut', evaluatedAt: '2026-11-14T08:00:00Z', metrics: [
    { metric: 'heartRate', latestKnownMeasurement: null, latestMeasurement: null, latestReceivedMeasurement: null,
      quality: { ...pulse, status: 'clockSkew', clockSkew: true, ageSeconds: null },
      latestReceivedQuality: { ...pulse, status: 'unknownTime', ageSeconds: null } },
  ] }
  expect(nextFreshnessExpiry(latest, 0)).toBeUndefined()
})
