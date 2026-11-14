import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it, vi } from 'vitest'
import { MetricOverview } from '../../App'
import { CheckInPanel } from '../check-in/CheckInPanel'
import { SystemCheckInActions } from '../check-in/SystemCheckInActions'
import GroundConsole from '../ground/GroundConsole'
import { useTelemetry } from '../ground/useTelemetry'
import { HealthPanel } from './HealthPanel'
import TrendPanel from './TrendPanel'
import { healthFixture } from './health.fixture'
import { supportedMetricConfigs } from './metrics'

vi.mock('../ground/useTelemetry', () => ({ useTelemetry: vi.fn(() => ({ series: {}, loading: false, error: '' })) }))

it('uses six advertised capabilities on an older API and twelve after the server expands, including missing measurements', () => {
  for (const extended of [false, true]) {
    const context = healthFixture(extended)
    context.latest.metrics[0].latestKnownMeasurement = null
    const supported = supportedMetricConfigs(context.latest).map(metric => metric.id)
    expect(supported).toHaveLength(extended ? 12 : 6)
    const cards = renderToStaticMarkup(<MetricOverview latest={context.latest} connected />)
    expect(cards.match(/<article/g)).toHaveLength(supported.length)
    const form = renderToStaticMarkup(<CheckInPanel supportedMetrics={supported} onSaved={() => {}} onClose={() => {}} />)
    expect(form.match(/<option /g)).toHaveLength(supported.length)
  }
})

it('allows the six-rule server summary to be complete and describes missing twelve-metric rules without asking for another measurement', () => {
  const old = renderToStaticMarkup(<HealthPanel context={healthFixture()} />)
  expect(old).toContain('Пригодны для оценки: 6 / 6')
  expect(old).toContain('health-panel-noAlert')
  const expanded = renderToStaticMarkup(<HealthPanel context={healthFixture(true)} />)
  expect(expanded).toContain('Пригодны для оценки: 6 / 12')
  expect(expanded).toContain('Без правил: 6')
  expect(expanded).toContain('автоматические правила ещё не подключены')
  expect(expanded).not.toContain('получите свежие измерения')
})

it('disables unsupported symptom writes and leaves the body area selectable with an explanation', () => {
  const supported = supportedMetricConfigs(healthFixture().latest).map(metric => metric.id)
  const actions = renderToStaticMarkup(<SystemCheckInActions system="msk" supportedMetrics={supported} onAdd={() => {}} />)
  expect(actions.match(/disabled=""/g)).toHaveLength(2)
  expect(actions).toContain('Сервер пока не поддерживает ввод')
  const form = renderToStaticMarkup(<CheckInPanel initialMetric="headache" supportedMetrics={supported} onSaved={() => {}} onClose={() => {}} />)
  expect(form).toContain('type="submit" class="primary-button" disabled=""')
  expect(form).toContain('Сервер пока не поддерживает ввод этого показателя')
  const trend = renderToStaticMarkup(<TrendPanel metric="headache" supportedMetrics={supported} onMetricChange={() => {}} onAdd={() => {}} refreshVersion={0} />)
  expect(trend).toContain('Сервер пока не поддерживает ввод и историю')
  expect(trend).not.toContain('Загрузка истории')
})

it('filters persisted unsupported Ground channels before passing them to the HTTP polling hook', () => {
  const { latest } = healthFixture()
  renderToStaticMarkup(<GroundConsole latest={latest} connected elapsedSeconds={0} refreshVersion={0} selectedSystem="msk" onSystemSelect={() => {}} onOpenMetric={() => {}} channels={['heartRate', 'headache', 'spo2']} onChannelsChange={() => {}} days={1} onDaysChange={() => {}} />)
  expect(vi.mocked(useTelemetry).mock.lastCall?.[0]).toEqual(['heartRate'])
})
