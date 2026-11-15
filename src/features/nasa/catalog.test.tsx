import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'
import { METRICS } from '../dashboard/metrics'
import { nasaCatalog, nasaEntriesForMetric, nasaSource } from './catalog'
import { NasaMetricContext, NasaCatalogPanel } from './NasaContext'

it('keeps curated records traceable to NASA and mappings within the supported registry', () => {
  const ids = nasaCatalog.entries.map(entry => entry.id)
  expect(new Set(ids).size).toBe(ids.length)
  for (const entry of nasaCatalog.entries) {
    const source = nasaSource(entry.sourceId)
    expect(source).toBeDefined()
    expect(new URL(source!.url).hostname).toBe(source!.agency === 'NASA' ? 'www.nasa.gov' : 'www.asc-csa.gc.ca')
    expect(entry.sourceSection.length).toBeGreaterThan(0)
    expect(entry.projectUse.length).toBeGreaterThan(0)
    expect(entry.limitation.length).toBeGreaterThan(0)
    for (const metric of entry.metrics) expect(METRICS.some(item => item.id === metric)).toBe(true)
  }
})

it('selects relevant research without inventing links for unsupported interpretations', () => {
  expect(nasaEntriesForMetric('heartRate').map(entry => entry.id)).toEqual(['cardiovascular', 'csa-space-science'])
  expect(nasaEntriesForMetric('sleepHours').map(entry => entry.id)).toEqual(['sleep', 'archer', 'standard-measures'])
  expect(nasaEntriesForMetric('glucose')).toEqual([])
  expect(nasaEntriesForMetric('headache')).toEqual([])
})

it('renders source, team interpretation and limits even without any live measurements', () => {
  const html = renderToStaticMarkup(<NasaMetricContext metric="visionBlur" />)
  expect(html).toContain('https://www.nasa.gov/hrp/risks/')
  expect(html).toContain('Самооценка не диагностирует SANS')
  expect(html).toContain('интерпретация команды')
  const unmapped = renderToStaticMarkup(<NasaMetricContext metric="glucose" />)
  expect(unmapped).toContain('нет обоснованной прямой связи')
  expect(unmapped).not.toContain('nasa-entry')
  const catalog = renderToStaticMarkup(<NasaCatalogPanel />)
  expect(catalog).toContain('Показания симулятора синтетические')
  expect(catalog).toContain('не изменяет числовые пороги')
})

it('keeps radiation separate from vital-sign inference', () => {
  const radiation = nasaCatalog.entries.find(entry => entry.id === 'csa-cans')!
  expect(radiation.metrics).toEqual([])
  expect(nasaSource(radiation.sourceId)?.agency).toBe('CSA')
  expect(nasaEntriesForMetric('exerciseMinutes').some(entry => entry.id === 'csa-space-science')).toBe(true)
})
