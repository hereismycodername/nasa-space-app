// Synthetic test profiles only; these are not medical thresholds or alerts.
// fluctuation is in [0, 1], giving the same +12..15 bpm in history and live data.
export function heartRateValue(normalValue, profile, fluctuation) {
  const offset = profile === 'attention' ? 12 + 3 * fluctuation : 0
  return Math.round((normalValue + offset) * 100) / 100
}

// The same synthetic attention window as pulse: pressure remains historical only.
export function bloodPressureValues(systolic, diastolic, profile, fluctuation) {
  const attention = profile === 'attention'
  const round = value => Math.round(value * 10) / 10
  return {
    systolic: round(systolic + (attention ? 10 + 4 * fluctuation : 0)),
    diastolic: round(diastolic + (attention ? 6 + 3 * fluctuation : 0)),
  }
}
