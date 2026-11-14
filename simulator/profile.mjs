// Synthetic test profiles only; these are not medical thresholds or alerts.
// fluctuation is in [0, 1], giving the same +12..15 bpm in history and live data.
export function heartRateValue(normalValue, profile, fluctuation) {
  const offset = profile === 'attention' ? 12 + 3 * fluctuation : 0
  return Math.round((normalValue + offset) * 100) / 100
}
