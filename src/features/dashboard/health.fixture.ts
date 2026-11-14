import type { HealthAssessment, LatestMeasurements } from '../../shared/types'
import { METRICS } from './metrics'

/** Synthetic API fixture, not production values or clinical rules. */
export function healthFixture(extended = false) {
  const evaluatedAt = '2026-11-15T08:00:00Z'
  const metrics = extended ? METRICS : METRICS.slice(0, 6)
  const latest: LatestMeasurements = { astronautId: 'demo-astronaut', evaluatedAt, metrics: metrics.map(metric => {
    const measurement = { id: `known-${metric.id}`, astronautId: 'demo-astronaut', deviceId: 'manual-01', sessionId: 'test', sequence: 0,
      source: 'manual' as const, metric: metric.id, unit: metric.unit, value: 1, measuredAt: evaluatedAt, receivedAt: evaluatedAt, timeBasis: 'measuredAt' as const }
    const quality = { status: 'fresh' as const, measurementId: measurement.id, freshForSeconds: 15, ageSeconds: 0, clockSkew: false }
    return { metric: metric.id, latestMeasurement: measurement, latestKnownMeasurement: measurement, latestReceivedMeasurement: measurement, quality, latestReceivedQuality: quality }
  }) }
  const assessment: HealthAssessment = { astronautId: latest.astronautId, evaluatedAt, status: 'not-evaluated', explanation: 'BE-06',
    metrics: latest.metrics.map(({ metric, quality, latestReceivedQuality }) => ({ metric, quality: { ...quality }, latestReceivedQuality: { ...latestReceivedQuality } })),
    health: { ruleSetVersion: 'be08-demo-1', demonstrationOnly: true, status: 'noAlert', explanation: 'Тестовая оценка',
      disclaimer: 'Демонстрация, не диагноз.', emergencyGuidance: 'При выраженных симптомах немедленно привлеките экипаж.',
      evaluatedMetricCount: 6, totalMetricCount: metrics.length,
      rules: metrics.map((metric, index) => ({ ruleId: `be08-${metric.id}`, ruleVersion: 'be08-demo-1', metric: metric.id, title: metric.label,
        status: index < 6 ? 'noAlert' : 'notEvaluated', explanation: index < 6 ? 'Тестовое правило не сработало.' : 'Правила ещё не подключены.', measurementIds: index < 6 ? [`known-${metric.id}`] : [], validUntil: index < 6 ? '2026-11-15T08:00:15Z' : null,
        suggestedAction: 'Следуйте согласованному плану наблюдения.', escalation: 'none', flightContext: 'Контекст полёта и ограничений показателя.',
        sourceUrls: ['https://www.nasa.gov/hrp/hazards/'], demonstrationOnly: true })),
      education: ['gravity', 'isolation', 'radiation', 'distance', 'environment'].map(id => ({ id, title: id, explanation: 'Опасность полёта.', suggestedAction: 'Следуйте плану миссии.', sourceUrls: ['https://www.nasa.gov/hrp/hazards/'] })),
    },
  }
  return { latest, assessment, connected: true }
}
