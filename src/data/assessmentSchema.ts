import { z } from 'zod'
import type { HealthAssessment } from '../shared/types'
import { METRIC_IDS } from '../shared/types'

const metric = z.enum(METRIC_IDS)
const status = z.enum(['notEvaluated', 'insufficientData', 'noAlert', 'attention', 'critical'])
const text = z.string().min(1)
const timestamp = text.refine(value => /T.*(?:Z|[+-]\d{2}:\d{2})$/i.test(value) && Number.isFinite(Date.parse(value)))
const source = text.refine(value => {
  try { const url = new URL(value); return url.protocol === 'https:' && !url.username && !url.password } catch { return false }
})
const quality = z.object({
  status: z.enum(['missing', 'fresh', 'stale', 'unknownTime', 'clockSkew']),
  measurementId: text.nullable(), freshForSeconds: z.number().nonnegative(),
  ageSeconds: z.number().nonnegative().nullable(), clockSkew: z.boolean(),
})
const rule = z.object({
  ruleId: text, ruleVersion: text, metric, title: text, status, explanation: text,
  measurementIds: z.array(text), validUntil: timestamp.nullable(), suggestedAction: text,
  escalation: z.enum(['none', 'self', 'crew', 'ground', 'urgent']), flightContext: text,
  sourceUrls: z.array(source), demonstrationOnly: z.literal(true),
}).refine(value => ['noAlert', 'attention', 'critical'].includes(value.status)
  ? value.measurementIds.length === 1 && value.validUntil !== null && value.sourceUrls.length > 0
  : value.measurementIds.length === 0 && value.validUntil === null)
const education = z.object({ id: text, title: text, explanation: text, suggestedAction: text, sourceUrls: z.array(source).min(1) })
const health = z.object({
  ruleSetVersion: text, demonstrationOnly: z.literal(true), status, explanation: text,
  disclaimer: text, emergencyGuidance: text, evaluatedMetricCount: z.number().int().nonnegative(),
  totalMetricCount: z.number().int().positive().max(METRIC_IDS.length), rules: z.array(rule).min(1), education: z.array(education).length(5),
}).refine(value => value.rules.length === value.totalMetricCount
  && new Set(value.rules.map(item => item.metric)).size === value.totalMetricCount
  && new Set(value.education.map(item => item.id)).size === 5
  && value.rules.every(item => item.ruleVersion === value.ruleSetVersion)
  && value.evaluatedMetricCount === value.rules.filter(item => ['noAlert', 'attention', 'critical'].includes(item.status)).length)

const assessment = z.object({
  status: z.enum(['no-data', 'stale', 'not-evaluated', 'unknown-time', 'clock-skew']),
  explanation: z.string(), astronautId: text, evaluatedAt: timestamp,
  metrics: z.array(z.object({ metric, quality, latestReceivedQuality: quality })), health: health.optional(),
})

/** An absent additive field is a BE-06 server; a malformed field must never become a green result. */
export function parseAssessment(payload: unknown): HealthAssessment {
  const parsed = assessment.safeParse(payload)
  if (!parsed.success) throw new Error('Сервер вернул несовместимую оценку. Текущий статус не подтверждён.')
  return parsed.data
}
