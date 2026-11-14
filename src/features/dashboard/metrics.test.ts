import { expect, it } from 'vitest'
import { formatTime } from './metrics'

it('preserves server-accepted timestamps that JavaScript cannot parse instead of crashing the dashboard', () => {
  for (const timestamp of ['2016-12-31T23:59:60Z', '2020-01-01T00:00:00−01:00']) {
    expect(formatTime(timestamp)).toBe(timestamp)
  }
  expect(formatTime('2020-01-01T00:00:00Z')).not.toBe('2020-01-01T00:00:00Z')
})
