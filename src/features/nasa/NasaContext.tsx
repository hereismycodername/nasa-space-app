import { useState } from 'react'
import type { Metric } from '../../shared/types'
import { METRICS } from '../dashboard/metrics'
import { nasaCatalog, nasaEntriesForMetric, nasaSource } from './catalog'
import './nasa.css'

function ResearchEntry({ entry }: { entry: typeof nasaCatalog.entries[number] }) {
  const source = nasaSource(entry.sourceId)
  return <section className="nasa-entry">
    <h3>{entry.title}</h3>
    <p className="note">{entry.kind === 'risk' ? `Риск · ${source?.agency}` : `Исследование · ${source?.agency}`}</p>
    <p>{entry.summary}</p>
    <p><strong>Как используем: </strong>{entry.projectUse}</p>
    <p className="note"><strong>Ограничение: </strong>{entry.limitation}</p>
    {source ? <a href={source.url} target="_blank" rel="noreferrer">{source.agency}: {source.title} — раздел «{entry.sourceSection}» ↗<span className="sr-only"> (откроется в новой вкладке)</span></a> : null}
  </section>
}

export function NasaMetricContext({ metric }: { metric: Metric }) {
  const entries = nasaEntriesForMetric(metric)
  return <details className="nasa-metric-context">
    <summary>NASA / CSA: зачем отслеживать показатель</summary>
    <p className="note">Связь с показателем — интерпретация команды, не диагноз NASA.</p>
    {entries.length ? entries.map(entry => <ResearchEntry key={entry.id} entry={entry} />)
      : <p className="note">В этой версии каталога нет обоснованной прямой связи с данным показателем. Его запись не означает наличия правила или рекомендации NASA.</p>}
    <p className="note">Проверено {nasaCatalog.reviewedAt}. Каталог {nasaCatalog.version}.</p>
  </details>
}

export function NasaCatalogPanel() {
  const [metric, setMetric] = useState<Metric | ''>('')
  const entries = metric ? nasaEntriesForMetric(metric) : nasaCatalog.entries
  return <section className="panel nasa-catalog" aria-label="Использование ресурсов NASA / CSA">
    <h2>Использование ресурсов NASA / CSA</h2>
    <p>{nasaCatalog.notice}</p>
    <p className="note">Показания симулятора синтетические. Каталог объясняет выбор наблюдений и не изменяет числовые пороги или статус здоровья.</p>
    <label className="metric-select">Связанные с показателем материалы
      <select value={metric} onChange={event => setMetric(event.target.value as Metric | '')}>
        <option value="">Все риски и исследования</option>
        {METRICS.map(item => <option key={item.id} value={item.id}>{item.label}</option>)}
      </select>
    </label>
    <p className="note">Найдено: {entries.length}. Проверено {nasaCatalog.reviewedAt} · {nasaCatalog.version}</p>
    <div className="nasa-entries">{entries.map(entry => <div key={entry.id}>
      <ResearchEntry entry={entry} />
      <p className="note">Показатели: {METRICS.filter(item => entry.metrics.includes(item.id)).map(item => item.label).join(', ')}</p>
    </div>)}</div>
    {!entries.length ? <p>Прямой связи в каталоге пока нет. Мы не приписываем NASA обоснование этого показателя.</p> : null}
    <p className="note">Локальный каталог доступен без интернета после загрузки приложения. Оригинальные страницы открываются на сайте NASA.</p>
  </section>
}
