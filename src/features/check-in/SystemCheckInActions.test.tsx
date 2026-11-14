import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'
import { SystemCheckInActions } from './SystemCheckInActions'
import { CheckInPanel } from './CheckInPanel'
import { METRIC_IDS } from '../../shared/types'

it('offers the correct symptom explicitly after choosing a body area', () => {
  const head = renderToStaticMarkup(<SystemCheckInActions system="brain" onAdd={() => {}} supportedMetrics={METRIC_IDS} />)
  const back = renderToStaticMarkup(<SystemCheckInActions system="msk" onAdd={() => {}} supportedMetrics={METRIC_IDS} />)
  expect(head).toContain('Отметить головную боль')
  expect(head).not.toContain('Отметить боль в спине')
  expect(back).toContain('Отметить боль в спине')
  expect(back).toContain('Активность за 24 часа')
  expect(renderToStaticMarkup(<SystemCheckInActions system={null} onAdd={() => {}} />)).toBe('')
})

it('opens an unfilled symptom scale at zero-inclusive bounds and keeps all twelve choices available', () => {
  const html = renderToStaticMarkup(<CheckInPanel initialMetric="headache" supportedMetrics={METRIC_IDS} onSaved={() => {}} onClose={() => {}} />)
  expect(html).toContain('value="headache" selected=""')
  expect(html).toContain('min="0" max="10" step="any"')
  expect(html).toContain('0 (боли нет)')
  expect(html.match(/<option /g)).toHaveLength(12)
  expect(html).not.toContain('Измерение сохранено')
})
