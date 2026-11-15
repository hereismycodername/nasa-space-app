import data from '../../../resources/nasa/catalog.json'
import type { Metric } from '../../shared/types'

export const nasaCatalog = data
export function nasaEntriesForMetric(metric: Metric) {
  return nasaCatalog.entries.filter(entry => entry.metrics.includes(metric))
}
export function nasaSource(id: string) {
  return nasaCatalog.sources.find(source => source.id === id)
}
