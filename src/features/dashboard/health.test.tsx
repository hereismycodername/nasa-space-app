import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'
import { MetricOverview } from '../../App'
import { ObservationLog } from '../ground/GroundConsole'
import { AnatomyPanel } from '../anatomy/AnatomyPanel'
import { HealthPanel, MetricHealth } from './HealthPanel'
import { metricHealth, summarizeHealth, systemHealth } from './health'
import { healthFixture } from './health.fixture'
import { METRICS } from './metrics'

it('keeps server decisions current through the inclusive expiry boundary and neutralizes them immediately after it', () => {
  const context = healthFixture()
  expect(metricHealth('heartRate', { ...context, elapsedSeconds: 15 })).toMatchObject({ current: true, status: 'noAlert' })
  expect(metricHealth('heartRate', { ...context, elapsedSeconds: 15.001 })).toMatchObject({ current: false, status: 'insufficientData' })
  // validUntil may be shorter than the quality window; both have to hold.
  context.assessment.health!.rules[0].validUntil = '2026-11-15T08:00:03Z'
  expect(metricHealth('heartRate', { ...context, elapsedSeconds: 3.001 }).current).toBe(false)
})

it('does not interpret measurement values in the client, and requires full coverage for a no-alert summary', () => {
  const context = healthFixture()
  context.latest.metrics[0].latestKnownMeasurement!.value = 999999
  const views = context.latest.metrics.map(metric => metricHealth(metric.metric, context))
  expect(summarizeHealth(views, 6)).toBe('noAlert')
  expect(summarizeHealth(views.slice(1), 6)).toBe('insufficientData')
  context.assessment.health!.rules[0].status = 'critical'
  context.latest.metrics.splice(1)
  expect(summarizeHealth(METRICS.map(metric => metricHealth(metric.id, context)), 6)).toBe('critical')
})

it('withdraws results when disconnected, replaced, mismatched or no longer fresh in either snapshot', () => {
  const context = healthFixture()
  expect(metricHealth('heartRate', { ...context, connected: false }).current).toBe(false)
  expect(metricHealth('heartRate', { ...context, updating: true }).current).toBe(true)
  expect(metricHealth('heartRate', { ...context, latestElapsedSeconds: 16 }).current).toBe(false)
  context.latest.metrics[0].latestKnownMeasurement!.id = 'new'
  expect(metricHealth('heartRate', context).current).toBe(false)
  context.latest.metrics[0].latestKnownMeasurement!.id = 'known-heartRate'
  context.assessment.metrics[0].quality.measurementId = 'different'
  expect(metricHealth('heartRate', context).current).toBe(false)
  context.assessment.metrics[0].quality.measurementId = 'known-heartRate'
  for (const status of ['stale', 'unknownTime', 'clockSkew', 'missing'] as const) {
    context.assessment.metrics[0].quality.status = status
    expect(metricHealth('heartRate', context).current).toBe(false)
  }
})

it('retains a previous critical explanation with an explicit timestamp without showing a current red or green result offline', () => {
  const context = healthFixture()
  context.connected = false
  context.assessment.health!.rules[0].status = 'critical'
  context.assessment.health!.rules[0].explanation = 'Предыдущее срочное срабатывание.'
  const html = renderToStaticMarkup(<HealthPanel context={context} />)
  expect(html).toContain('Предыдущая оценка: Срочная помощь')
  expect(html).toContain('не текущий статус')
  expect(html).toContain('Предыдущее срочное срабатывание.')
  expect(html).toContain('Пригодны для оценки: 0 / 6')
  expect(html).not.toContain('health-badge health-critical')
  expect(html).not.toContain('health-badge health-noAlert')
})

it('shows rule explanation, next action, escalation, spaceflight context and sources in both Crew and Ground', () => {
  const context = healthFixture()
  context.assessment.health!.rules[0].status = 'attention'
  context.assessment.health!.rules[0].escalation = 'ground'
  for (const html of [
    renderToStaticMarkup(<MetricOverview latest={context.latest} connected healthContext={context} />),
    renderToStaticMarkup(<ObservationLog latest={context.latest} connected elapsedSeconds={0} healthContext={context} />),
  ]) {
    expect(html).toContain('health-badge health-attention')
    expect(html).toContain('quality-fresh')
    expect(html).toContain('Тестовое правило не сработало.')
    expect(html).toContain('Следуйте согласованному плану наблюдения.')
    expect(html).toContain('Консультация с медиком')
    expect(html).toContain('Контекст полёта и ограничений показателя.')
    expect(html).toContain('href="https://www.nasa.gov/hrp/hazards/"')
  }
})

it('leaves unsupported systems neutral and preserves anatomical navigation separately from health status', () => {
  const context = healthFixture()
  context.assessment.health!.rules[0].status = 'attention'
  const statuses = systemHealth(context)
  expect(statuses.heart).toBe('attention')
  expect(statuses.lungs).toBe('notEvaluated')
  expect(statuses.brain).toBe('noAlert')
  const html = renderToStaticMarkup(<AnatomyPanel selected="heart" onSelect={() => {}} availableSystems={['heart']} healthStatuses={statuses} />)
  expect(html).toContain('3D-модель человека')
  expect(html).toContain('Сердце и кровообращение: есть данные')
  expect(html).toContain('anatomy-health-status health-attention')
  expect(html).toContain('Это оценка показателей системы, не диагноз органа')
})

it('supports a BE-06 server without inventing a health result or breaking the readings', () => {
  const context = healthFixture()
  delete context.assessment.health
  const html = renderToStaticMarkup(<MetricHealth metric="heartRate" context={context} />)
  expect(html).toContain('Не оценено')
  expect(html).not.toContain('Нет срабатываний')
})

it('keeps the six B/C metrics unassessed even with fresh zero-valued measurements', () => {
  const context = healthFixture(true)
  context.latest.metrics[10].latestKnownMeasurement!.value = 0
  expect(metricHealth('headache', context)).toMatchObject({ current: false, status: 'notEvaluated' })
  expect(summarizeHealth(METRICS.map(metric => metricHealth(metric.id, context)), METRICS.length)).toBe('insufficientData')
  const html = renderToStaticMarkup(<MetricOverview latest={context.latest} connected selectedSystem="brain" healthContext={context} />)
  expect(html).toContain('0 <small>из 10</small>')
  expect(html).toContain('Правила ещё не подключены.')
})

it('retains the last alert when the next server response withdraws a result because data expired', () => {
  const context = healthFixture()
  const warning = { ...context.assessment.health!.rules[0], status: 'critical' as const }
  Object.assign(context.assessment.health!.rules[0], { status: 'insufficientData', measurementIds: [], validUntil: null, explanation: 'Нужны свежие данные.' })
  const view = metricHealth('heartRate', { ...context, previousHealthAlerts: { heartRate: { rule: warning, evaluatedAt: context.assessment.evaluatedAt } } })
  expect(view).toMatchObject({ status: 'insufficientData', current: false, rule: warning, reason: 'Нужны свежие данные.' })
})

it('keeps emergency guidance open in the compact overview while preserving expandable rule details', () => {
  const context = healthFixture()
  context.assessment.health!.rules[0].status = 'critical'
  const html = renderToStaticMarkup(<HealthPanel context={context} compact />)
  expect(html).toContain('health-panel-compact')
  expect(html).toContain('class="emergency-guidance" open=""')
  expect(html).toContain(context.assessment.health!.emergencyGuidance)
  expect(html).toContain('Причина и действия')
  expect(html).toContain(context.assessment.health!.rules[0].suggestedAction)
})
