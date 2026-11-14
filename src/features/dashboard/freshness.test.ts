import { expect, it } from 'vitest'
import type { DataQuality, LatestMeasurements } from '../../shared/types'
import { currentFreshness, nextFreshnessExpiry, qualityDescription } from './freshness'

const pulse: DataQuality = { status: 'fresh', measurementId: 'pulse', ageSeconds: 14, freshForSeconds: 15, clockSkew: false }
const sleep: DataQuality = { ...pulse, measurementId: 'sleep', freshForSeconds: 86400 }

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
