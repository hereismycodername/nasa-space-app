import { createHash } from 'node:crypto'
import { describe, expect, it } from 'vitest'
import { generateHistory, HISTORY_METRICS } from './history.mjs'

const options = { scenario: 'baseline', seed: 42, reference: '2026-11-14T12:00:00Z' }
const units = { heartRate: 'bpm', systolicPressure: 'mmHg', diastolicPressure: 'mmHg', temperature: '°C', sleepHours: 'h', mood: 'score', spo2: '%', glucose: 'mg/dL', visionBlur: 'score', exerciseMinutes: 'min', headache: 'score', backPain: 'score' }
const key = row => `${row.deviceId}/${row.sessionId}/${row.sequence}`

describe('synthetic history', () => {
  it('reproduces payloads, varies values with seed and obeys the ingestion contract', () => {
    const rows = generateHistory(options)
    expect(JSON.stringify(generateHistory(options))).toBe(JSON.stringify(rows))
    expect(generateHistory({ ...options, seed: 43 }).map(row => row.value)).not.toEqual(rows.map(row => row.value))
    const attention = generateHistory({ ...options, scenario: 'attention' })
    const attentionPulse = attention.filter(row => row.metric === 'heartRate')
    const normalPulse = rows.filter(row => row.metric === 'heartRate')
    for (const [index, row] of attentionPulse.entries()) {
      const delta = row.value - normalPulse[index].value
      if (Date.parse(row.measuredAt) < Date.parse(options.reference) - 3 * 86400_000) expect(delta).toBe(0)
      else {
        expect(delta).toBeGreaterThanOrEqual(12)
        expect(delta).toBeLessThanOrEqual(15)
      }
    }
    expect(new Set(rows.map(row => row.sequence)).size).toBe(rows.length)
    expect(rows.map(row => row.sequence)).toEqual(rows.map((_, index) => index))
    expect(Object.fromEntries(Object.keys(units).map(metric => [metric, rows.filter(row => row.metric === metric).length])))
      .toEqual({ heartRate: 2016, systolicPressure: 14, diastolicPressure: 14, temperature: 21, sleepHours: 7, mood: 7, spo2: 168, glucose: 7, visionBlur: 7, exerciseMinutes: 7, headache: 14, backPain: 7 })
    expect(new Set(rows.map(row => row.metric))).toEqual(new Set(HISTORY_METRICS))
    for (const readings of [rows, attention]) {
      for (const systolic of readings.filter(row => row.metric === 'systolicPressure')) {
        const diastolic = readings.find(row => row.metric === 'diastolicPressure' && row.measuredAt === systolic.measuredAt)
        expect(diastolic).toBeDefined()
        expect(systolic.value - diastolic.value).toBeGreaterThanOrEqual(25)
        expect(new Date(systolic.measuredAt).getUTCMinutes()).toBe(30)
        expect([7, 19]).toContain(new Date(systolic.measuredAt).getUTCHours())
      }
    }
    for (const [metric, minimum, maximum] of [['systolicPressure', 10, 14], ['diastolicPressure', 6, 9]]) {
      for (const baseline of rows.filter(row => row.metric === metric)) {
        const raised = attention.find(row => row.metric === metric && row.measuredAt === baseline.measuredAt)
        const delta = raised.value - baseline.value
        if (Date.parse(baseline.measuredAt) < Date.parse(options.reference) - 3 * 86400_000) expect(delta).toBe(0)
        else {
          expect(delta).toBeGreaterThanOrEqual(minimum - 1e-9)
          expect(delta).toBeLessThanOrEqual(maximum + 1e-9)
        }
      }
    }
    const newSchedules = {
      spo2: { minutes: [15], hours: Array.from({ length: 24 }, (_, i) => i), normal: [96, 99], attention: [93, 95], direction: -1 },
      glucose: { minutes: [15], hours: [7], normal: [80, 95], attention: [105, 118], direction: 1 },
      visionBlur: { minutes: [0], hours: [9], normal: [0, 1], attention: [2, 4], direction: 1 },
      exerciseMinutes: { minutes: [30], hours: [21], normal: [90, 150], attention: [25, 50], direction: -1 },
      headache: { minutes: [0], hours: [10, 18], normal: [0, 1], attention: [4, 6], direction: 1 },
      backPain: { minutes: [0], hours: [18], normal: [0, 2], attention: [3, 5], direction: 1 },
    }
    for (const [metric, schedule] of Object.entries(newSchedules)) {
      for (const baseline of rows.filter(row => row.metric === metric)) {
        const time = new Date(baseline.measuredAt)
        expect(schedule.minutes).toContain(time.getUTCMinutes())
        expect(schedule.hours).toContain(time.getUTCHours())
        expect(baseline.value).toBeGreaterThanOrEqual(schedule.normal[0])
        expect(baseline.value).toBeLessThanOrEqual(schedule.normal[1])
        const changed = attention.find(row => row.metric === metric && row.measuredAt === baseline.measuredAt)
        if (time.getTime() < Date.parse(options.reference) - 3 * 86400_000) expect(changed.value).toBe(baseline.value)
        else {
          expect((changed.value - baseline.value) * schedule.direction).toBeGreaterThan(0)
          expect(changed.value).toBeGreaterThanOrEqual(schedule.attention[0])
          expect(changed.value).toBeLessThanOrEqual(schedule.attention[1])
        }
      }
    }
    // SHA-256 of the original six channels before this extension (seed 42).
    // Exclude sequence: adding interleaved channels necessarily renumbers messages.
    const originalHashes = {
      baseline: '32015578fa035e94a2478a78ac868c28b3da06506c92a66ddd825f02bb8208d5',
      attention: '7ca0b8fe2c4fcffe0145d301631193b108ce50743b74c82121fcbd31dbf2502b',
      stale: 'ba48d0690e5c376f26a42573957e9ca0643e2f73908c2dd5bec558d8bbe9f409',
      delayed: 'a8ac96962b3d9b387501715f94f33b8c0df12f3483d2073aa76efc330aee11b4',
    }
    for (const [scenario, hash] of Object.entries(originalHashes)) {
      const original = generateHistory({ ...options, scenario })
        .filter(row => !Object.hasOwn(newSchedules, row.metric))
        .map(({ metric, value, unit, measuredAt }) => ({ metric, value, unit, measuredAt }))
      expect(createHash('sha256').update(JSON.stringify(original)).digest('hex')).toBe(hash)
    }
    for (const row of rows) {
      expect(row.deviceId).toBe('simulator-01')
      expect(row.sessionId).toBe('demo-baseline-42')
      expect(row.unit).toBe(units[row.metric])
      expect(row.measuredAt).toMatch(/Z$/)
      expect(Date.parse(row.measuredAt)).toBeLessThan(Date.parse(options.reference))
      expect(Number.isFinite(row.value)).toBe(true)
      expect(row.value).toBeGreaterThanOrEqual(0)
      if (row.metric === 'sleepHours') expect(row.value).toBeLessThanOrEqual(24)
      if (row.metric === 'mood') {
        expect(Number.isInteger(row.value)).toBe(true)
        expect(row.value).toBeLessThanOrEqual(5)
      }
    }
  })

  it('cuts off stale streams and delivers a delayed shuffled buffer with exact retries', () => {
    const end = Date.parse(options.reference)
    const stale = generateHistory({ ...options, scenario: 'stale' })
    expect(Math.max(...stale.map(row => Date.parse(row.measuredAt)))).toBeLessThanOrEqual(end - 6 * 3600_000)
    expect(Math.max(...stale.filter(row => row.metric === 'sleepHours').map(row => Date.parse(row.measuredAt))))
      .toBeLessThanOrEqual(end - 2 * 86400_000)
    const delayed = generateHistory({ ...options, scenario: 'delayed' })
    expect(delayed.filter(row => row.metric !== 'heartRate' && Date.parse(row.measuredAt) >= end - 1800_000)).toEqual([])
    const buffer = delayed.slice(-9, -3)
    expect(buffer).toHaveLength(6)
    expect(buffer.every(row => row.metric === 'heartRate' && Date.parse(row.measuredAt) >= end - 1800_000
      && Date.parse(row.measuredAt) < end)).toBe(true)
    expect(buffer.some((row, i) => i > 0 && row.measuredAt < buffer[i - 1].measuredAt)).toBe(true)
    expect(delayed.slice(-3)).toEqual(delayed.slice(0, 3))
    expect(new Set(delayed.map(key)).size).toBe(delayed.length - 3)
  })
})
