import { expect, it } from 'vitest'
import { METRICS, formatTime } from './metrics'
import { METRIC_IDS } from '../../shared/types'

it('preserves server-accepted timestamps that JavaScript cannot parse instead of crashing the dashboard', () => {
  for (const timestamp of ['2016-12-31T23:59:60Z', '2020-01-01T00:00:00−01:00']) {
    expect(formatTime(timestamp)).toBe(timestamp)
  }
  expect(formatTime('2020-01-01T00:00:00Z')).not.toBe('2020-01-01T00:00:00Z')
})

it('covers the complete API metric catalog once and maps B/C data to its anatomical system', () => {
  expect(METRICS.map(metric => metric.id)).toEqual([...METRIC_IDS])
  expect(Object.fromEntries(METRICS.slice(6).map(metric => [metric.id, metric.system]))).toEqual({ spo2: 'lungs', glucose: 'gut', visionBlur: 'eyes', exerciseMinutes: 'msk', headache: 'brain', backPain: 'msk' })
})
