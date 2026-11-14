import type { DataQuality } from '../../shared/types'
import { currentFreshness, freshnessLabels, qualityDescription } from './freshness'

export function QualityBadge({ quality, connected, elapsedSeconds = 0 }: {
  quality?: DataQuality
  connected: boolean
  elapsedSeconds?: number
}) {
  if (!quality) return <span className="quality-badge quality-unavailable">Оценка не получена</span>
  const status = currentFreshness(quality, elapsedSeconds)
  if (!connected) return <span className="quality-badge quality-offline" title={`Последняя оценка сервера: ${freshnessLabels[quality.status]}. ${qualityDescription(quality)}`}>
    Свежесть не подтверждена
  </span>
  return <span className={`quality-badge quality-${status}`} title={qualityDescription(quality, elapsedSeconds)}>
    {freshnessLabels[status]}
  </span>
}
