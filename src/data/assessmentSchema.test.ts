import { expect, it } from 'vitest'
import { healthFixture } from '../features/dashboard/health.fixture'
import { parseAssessment } from './assessmentSchema'

it('accepts BE-06 responses and the complete additive BE-08 contract', () => {
  const { assessment } = healthFixture()
  expect(parseAssessment(assessment)).toEqual(assessment)
  const extended = healthFixture(true).assessment
  expect(parseAssessment(extended)).toEqual(extended)
  delete assessment.health
  expect(parseAssessment(assessment)).toEqual(assessment)
})

it.each([
  ['unrecognized status', (payload: ReturnType<typeof healthFixture>['assessment']) => { Object.assign(payload.health!.rules[0], { status: 'healthy' }) }],
  ['missing expiry', (payload: ReturnType<typeof healthFixture>['assessment']) => { payload.health!.rules[0].validUntil = null }],
  ['malformed expiry', (payload: ReturnType<typeof healthFixture>['assessment']) => { payload.health!.rules[0].validUntil = 'tomorrow' }],
  ['unsafe link', (payload: ReturnType<typeof healthFixture>['assessment']) => { payload.health!.rules[0].sourceUrls = ['javascript:alert(1)'] }],
  ['duplicate rules', (payload: ReturnType<typeof healthFixture>['assessment']) => { payload.health!.rules[1] = payload.health!.rules[0] }],
  ['wrong input identity', (payload: ReturnType<typeof healthFixture>['assessment']) => { payload.health!.rules[0].measurementIds = [] }],
  ['inconsistent count', (payload: ReturnType<typeof healthFixture>['assessment']) => { payload.health!.evaluatedMetricCount = 0 }],
  ['nondemonstration flag', (payload: ReturnType<typeof healthFixture>['assessment']) => { Object.assign(payload.health!, { demonstrationOnly: false }) }],
] as const)('rejects %s instead of displaying a confident result', (_name, change) => {
  const { assessment } = healthFixture()
  change(assessment)
  expect(() => parseAssessment(assessment)).toThrow('несовместимую оценку')
})

it('allows unavailable or unconfigured rules without fabricated inputs or references', () => {
  const { assessment } = healthFixture()
  const rule = assessment.health!.rules[0]
  Object.assign(rule, { status: 'notEvaluated', measurementIds: [], validUntil: null, sourceUrls: [] })
  assessment.health!.evaluatedMetricCount = 5
  assessment.health!.status = 'insufficientData'
  expect(parseAssessment(assessment).health?.rules[0]).toEqual(rule)
})
