import { bloodPressureValues, heartRateValue } from './profile.mjs'

const MINUTE = 60_000
const DAY = 24 * 60 * MINUTE
export const SCENARIOS = ['baseline', 'attention', 'stale', 'delayed']
export const HISTORY_METRICS = ['heartRate', 'systolicPressure', 'diastolicPressure', 'temperature', 'sleepHours', 'mood']

// Mulberry32: the state is local to each generation, including the buffer shuffle.
function randomFor(seed) {
  let state = seed >>> 0
  return () => {
    state = (state + 0x6d2b79f5) >>> 0
    let value = Math.imul(state ^ (state >>> 15), 1 | state)
    value ^= value + Math.imul(value ^ (value >>> 7), 61 | value)
    return ((value ^ (value >>> 14)) >>> 0) / 4294967296
  }
}

/** Pure synthetic history in submission order. All daily schedules use UTC. */
export function generateHistory({ scenario, seed, reference }) {
  if (!SCENARIOS.includes(scenario)) throw new Error(`Unknown scenario: ${scenario}`)
  if (!Number.isInteger(seed) || seed < 0 || seed > 0xffffffff) {
    throw new Error('seed must be an integer from 0 to 4294967295')
  }
  if (typeof reference !== 'string' || !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)$/.test(reference)) {
    throw new Error('reference must be an RFC3339 timestamp with timezone')
  }
  const end = Date.parse(reference)
  if (!Number.isFinite(end) || !Number.isFinite(new Date(end - 7 * DAY).getTime())) {
    throw new Error('Invalid reference date')
  }
  const random = randomFor(seed)
  const rows = []
  const round = value => Math.round(value * 100) / 100
  const start = end - 7 * DAY
  const add = (time, metric, value, unit) => {
    if (scenario === 'stale' && (time >= end - 6 * 60 * MINUTE
      || (metric === 'sleepHours' && time > end - 2 * DAY))) return
    if (scenario === 'delayed' && metric !== 'heartRate' && time >= end - 30 * MINUTE) return
    rows.push({ metric, value, unit, measuredAt: new Date(time).toISOString() })
  }
  for (let time = start; time < end; time += 5 * MINUTE) {
    const date = new Date(time)
    const hour = date.getUTCHours() + date.getUTCMinutes() / 60
    const attention = scenario === 'attention' && time >= end - 3 * DAY
    const activity = Math.max(0, 1 - Math.abs(hour - 11) / 0.25)
      + Math.max(0, 1 - Math.abs(hour - 16) / 0.25)
    const resting = 67.5 + 5 * Math.cos((hour - 15) * Math.PI / 12)
    const noise = random() * 6 - 3
    add(time, 'heartRate', heartRateValue(resting + noise + activity * 37,
      attention ? 'attention' : 'normal', (noise + 3) / 6), 'bpm')
  }
  // Calendar schedules keep sleep in the morning and mood in the evening even
  // when reference is not midnight. Filtering [start, end) gives seven of each.
  for (let day = Math.floor(start / DAY) * DAY; day < end; day += DAY) {
    for (const hour of [7, 8, 14, 20, 21]) {
      const time = day + hour * 60 * MINUTE
      if (time < start || time >= end) continue
      const attention = scenario === 'attention' && time >= end - 3 * DAY
      if (hour === 7) {
        const sample = random()
        add(time, 'sleepHours', round(attention ? 4 + sample : 6.5 + sample * 1.5), 'h')
      } else if (hour === 21) {
        const sample = random()
        add(time, 'mood', attention ? 2 : 3 + Math.floor(sample * 3), 'score')
      } else {
        add(time, 'temperature', round(36.4 + random() * 0.5), '°C')
      }
    }
  }
  // Draw pressure after the existing schedules to preserve their seeded values.
  for (let day = Math.floor(start / DAY) * DAY; day < end; day += DAY) {
    const restingSystolic = 112 + random() * 8
    const restingDiastolic = 69 + random() * 7
    for (const minute of [7 * 60 + 30, 19 * 60 + 30]) {
      const time = day + minute * MINUTE
      if (time < start || time >= end) continue
      const evening = minute > 12 * 60
      const fluctuation = random()
      const profile = scenario === 'attention' && time >= end - 3 * DAY ? 'attention' : 'normal'
      const pressure = bloodPressureValues(
        restingSystolic + (fluctuation - 0.5) * 2 + (evening ? 3 : 0),
        restingDiastolic + (random() - 0.5) * 2 + (evening ? 2 : 0),
        profile, fluctuation,
      )
      add(time, 'systolicPressure', pressure.systolic, 'mmHg')
      add(time, 'diastolicPressure', pressure.diastolic, 'mmHg')
    }
  }
  rows.sort((a, b) => a.measuredAt.localeCompare(b.measuredAt))
  let ordered = rows
  if (scenario === 'delayed') {
    const buffered = rows.filter(row => row.metric === 'heartRate' && Date.parse(row.measuredAt) >= end - 30 * MINUTE)
    ordered = rows.filter(row => !buffered.includes(row))
    for (let i = buffered.length - 1; i > 0; i--) {
      const j = Math.floor(random() * (i + 1))
      ;[buffered[i], buffered[j]] = [buffered[j], buffered[i]]
    }
    // Guarantee observable out-of-order delivery, even for an unlucky seed.
    const latest = buffered.reduce((a, b) => a.measuredAt > b.measuredAt ? a : b)
    if (buffered.at(-1) === latest) [buffered[0], buffered[buffered.length - 1]] = [buffered.at(-1), buffered[0]]
    ordered = [...ordered, ...buffered]
  }
  const payloads = ordered.map((row, sequence) => ({
    deviceId: 'simulator-01', sessionId: `demo-${scenario}-${seed}`, sequence, ...row,
  }))
  // Retransmissions retain the original sequence and every original field.
  if (scenario === 'delayed') payloads.push(...payloads.slice(0, 3).map(row => ({ ...row })))
  return payloads
}
