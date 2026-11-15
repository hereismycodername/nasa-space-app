import type { HealthAssessment, HealthRule, LatestMeasurements, Metric, PreviousHealthAlerts, RuleStatus } from '../../shared/types'
import { currentFreshness } from './freshness'
import { METRICS, SYSTEMS, supportedMetricConfigs } from './metrics'
import type { BodySystem } from './metrics'

export const healthLabels: Record<RuleStatus, string> = {
  notEvaluated: 'Не оценено', insufficientData: 'Недостаточно данных',
  noAlert: 'Нет срабатываний', attention: 'Требует внимания', critical: 'Срочная помощь',
}
export const escalationLabels = {
  none: 'Плановое наблюдение', self: 'Самопроверка', crew: 'Поддержка экипажа',
  ground: 'Консультация с медиком', urgent: 'Экипаж и срочная медицинская помощь',
}
export interface HealthContext {
  assessment?: HealthAssessment
  latest?: LatestMeasurements
  connected: boolean
  updating?: boolean
  elapsedSeconds?: number
  latestElapsedSeconds?: number
  previousHealthAlerts?: PreviousHealthAlerts
}
export interface RuleView {
  status: RuleStatus
  current: boolean
  reason: string
  rule?: HealthRule
  evaluatedAt?: string
}
const evaluatedStatuses = new Set<RuleStatus>(['noAlert', 'attention', 'critical'])

/** Only the server interprets values. Clients can withdraw, never promote, a result. */
export function metricHealth(metric: Metric, context: HealthContext): RuleView {
  const { assessment, latest, connected, elapsedSeconds = 0, latestElapsedSeconds = 0 } = context
  const currentRule = assessment?.health?.rules.find(item => item.metric === metric)
  const previous = context.previousHealthAlerts?.[metric]
  const rule = currentRule && evaluatedStatuses.has(currentRule.status) ? currentRule : previous?.rule ?? currentRule
  const evaluatedTime = previous?.rule === rule ? previous?.evaluatedAt : assessment?.evaluatedAt
  const neutral = (reason: string, status: RuleStatus = 'insufficientData'): RuleView => ({ status, current: false, reason, rule, evaluatedAt: evaluatedTime })
  if (!currentRule || !assessment) return neutral('Оценка правил ещё не получена.', 'notEvaluated')
  if (!connected) return neutral('Нет связи. Предыдущая оценка не подтверждает текущее состояние.')
  // Polling publishes latest + assessment atomically. Keep that coherent snapshot
  // during revalidation; expiry, evidence and connection checks still apply.
  if (!evaluatedStatuses.has(currentRule.status)) return neutral(currentRule.explanation, currentRule.status)
  if (!rule) return neutral('Оценка правил ещё не получена.', 'notEvaluated')
  const entry = latest?.metrics.find(item => item.metric === metric)
  const quality = assessment.metrics.find(item => item.metric === metric)?.quality
  const measurement = entry?.latestKnownMeasurement
  if (latest?.astronautId !== assessment.astronautId || !measurement || rule.measurementIds.length !== 1
    || rule.measurementIds[0] !== measurement.id || entry?.quality?.measurementId !== measurement.id
    || quality?.measurementId !== measurement.id) return neutral('Измерение изменилось или не совпадает с оценкой. Нужен новый ответ сервера.')
  if (!quality || !entry?.quality || !Number.isFinite(elapsedSeconds) || !Number.isFinite(latestElapsedSeconds)
    || currentFreshness(quality, elapsedSeconds) !== 'fresh'
    || currentFreshness(entry.quality, latestElapsedSeconds) !== 'fresh') return neutral('Нет свежего измерения с подтверждённым временем. Повторите измерение.')
  const evaluatedAt = Date.parse(assessment.evaluatedAt)
  const validUntil = Date.parse(rule.validUntil ?? '')
  if (!Number.isFinite(evaluatedAt) || !Number.isFinite(validUntil)
    || evaluatedAt + Math.max(0, elapsedSeconds) * 1000 > validUntil) return neutral('Срок оценки истёк. Повторите измерение и обновите данные.')
  return { status: rule.status, current: true, reason: rule.explanation, rule, evaluatedAt: assessment.evaluatedAt }
}

export function summarizeHealth(views: RuleView[], expectedCount: number): RuleStatus {
  if (views.some(view => view.current && view.status === 'critical')) return 'critical'
  if (views.some(view => view.current && view.status === 'attention')) return 'attention'
  if (expectedCount > 0 && views.length === expectedCount && views.every(view => view.current && view.status === 'noAlert')) return 'noAlert'
  return views.some(view => view.status !== 'notEvaluated') ? 'insufficientData' : 'notEvaluated'
}

export function systemHealth(context: HealthContext): Partial<Record<BodySystem, RuleStatus>> {
  const supported = assessmentMetricConfigs(context)
  return Object.fromEntries(SYSTEMS.map(system => {
    const metrics = supported.filter(metric => metric.system === system.id)
    return [system.id, summarizeHealth(metrics.map(metric => metricHealth(metric.id, context)), metrics.length)]
  }))
}

export function assessmentMetricConfigs(context: HealthContext) {
  if (context.latest) return supportedMetricConfigs(context.latest)
  const health = context.assessment?.health
  if (!health || health.rules.length !== health.totalMetricCount) return []
  const metrics = new Set(health.rules.map(rule => rule.metric))
  return METRICS.filter(metric => metrics.has(metric.id))
}

export function nextHealthExpiry(assessment: HealthAssessment, elapsedSeconds: number): number | undefined {
  const evaluatedAt = Date.parse(assessment.evaluatedAt)
  const remaining = (assessment.health?.rules ?? []).filter(rule => evaluatedStatuses.has(rule.status)).flatMap(rule => {
    const quality = assessment.metrics.find(item => item.metric === rule.metric)?.quality
    return [Date.parse(rule.validUntil ?? '') - evaluatedAt - elapsedSeconds * 1000,
      quality?.status === 'fresh' && quality.ageSeconds !== null ? (quality.freshForSeconds - quality.ageSeconds - elapsedSeconds) * 1000 : NaN]
  }).filter(ms => Number.isFinite(ms) && ms >= 0)
  return remaining.length ? Math.ceil(Math.min(...remaining)) + 1 : undefined
}
