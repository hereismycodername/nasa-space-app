import { expect, it } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { healthFixture } from '../dashboard/health.fixture'
import { parseAssessment } from '../../data/assessmentSchema'
import { ResearchExplanation } from '../dashboard/HealthPanel'
import type { ResearchInsight } from '../../shared/types'

const insight: ResearchInsight = {
  metric: 'sleepHours', catalogVersion: 'test-version', observation: 'Записано: 5 h.',
  measurementId: 'sleep-record', quality: 'stale',
  contexts: [{ title: 'Sleep risk', explanation: 'NASA context', projectUse: 'Дневник сна', limitation: 'Не диагноз', sourceAgency: 'NASA', sourceUrl: 'https://www.nasa.gov/hrp/risks/', sourceSection: 'Risk of Inadequate Sleep and Irregular Schedules' }],
  actions: [{ kind: 'record', label: 'Записать сон' }, { kind: 'history', label: 'История сна' }, { kind: 'guidance', label: 'Рекомендации' }],
}
it('preserves server observations and actions, labels offline snapshots, filters by metric', () => {
  const { assessment } = healthFixture()
  assessment.researchInsights = [insight]
  const parsed = parseAssessment(assessment)
  expect(parsed.researchInsights).toEqual([insight])
  const context = { assessment: parsed, connected: false }
  const html = renderToStaticMarkup(<ResearchExplanation metric="sleepHours" context={context} onRecord={() => {}} onHistory={() => {}} />)
  expect(html).toContain('Записано: 5 h.')
  expect(html).toContain('Нет связи: показан сохранённый ответ')
  expect(html).toContain('История сна')
  expect(html).toContain('https://www.nasa.gov/hrp/risks/')
  expect(renderToStaticMarkup(<ResearchExplanation metric="glucose" context={context} />)).toBe('')
  assessment.researchInsights = [{ ...insight, contexts: [{ ...insight.contexts[0], sourceUrl: 'javascript:alert(1)' }] }]
  expect(() => parseAssessment(assessment)).toThrow()
})
