import type { DataQuality } from '../../shared/types'
import { currentFreshness, freshnessLabel, qualityDescription } from './freshness'

export function QualityBadge({ quality, connected, elapsedSeconds = 0 }: {
  quality?: DataQuality
  connected: boolean
  elapsedSeconds?: number
}) {
  if (!quality) return <span className="quality-badge quality-unavailable">Оценка не получена</span>
  const status = currentFreshness(quality, elapsedSeconds)
  if (!connected) return <span className="quality-badge quality-offline" title={`Последняя оценка сервера: ${freshnessLabel(quality, elapsedSeconds)}. ${qualityDescription(quality, elapsedSeconds)}`}>
    Свежесть не подтверждена
  </span>
  return <span className={`quality-badge quality-${status}`} title={qualityDescription(quality, elapsedSeconds)}>
    {freshnessLabel(quality, elapsedSeconds)}
  </span>
}
