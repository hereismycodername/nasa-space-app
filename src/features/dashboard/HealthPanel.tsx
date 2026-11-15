import { useState } from 'react'
import { NasaMetricContext } from '../nasa/NasaContext'
import type { FlightEducation, Metric, RuleStatus } from '../../shared/types'
import { assessmentMetricConfigs, escalationLabels, healthLabels, metricHealth, summarizeHealth } from './health'
import type { HealthContext, RuleView } from './health'
import { formatTime } from './metrics'
import './health.css'

export function HealthBadge({ status, label }: { status: RuleStatus; label?: string }) {
  return <span className={`health-badge health-${status}`}><i aria-hidden="true" />{label ?? healthLabels[status]}</span>
}

export function MetricHealth({ metric, context, onRecord, onHistory }: { metric: Metric; context: HealthContext; onRecord?: () => void; onHistory?: () => void }) {
  const view = metricHealth(metric, context)
  return <div className="metric-health"><span className="metric-health-label">Оценка показателя · демо</span><HealthBadge status={view.status} /><HealthDetails view={view} /><ResearchExplanation metric={metric} context={context} onRecord={onRecord} onHistory={onHistory} /><NasaMetricContext metric={metric} /></div>
}

function Sources({ urls }: { urls: string[] }) {
  return <ul className="health-sources" aria-label="Источники правила">{urls.map((url, index) => <li key={url}><a href={url} target="_blank" rel="noreferrer">{new URL(url).hostname.replace(/^www\./, '')} · источник {index + 1}<span className="sr-only"> (откроется в новой вкладке)</span></a></li>)}</ul>
}

export function HealthDetails({ view }: { view: RuleView }) {
  const rule = view.rule
  if (!rule) return <div className="health-details"><div className="health-overview"><p className="health-reason note">{view.reason}</p></div></div>
  const historical = !view.current && ['noAlert', 'attention', 'critical'].includes(rule.status)
  const currentAlert = view.current && ['attention', 'critical'].includes(rule.status)
  return <div className="health-details">
    <div className="health-overview">
      <p className="health-reason">{view.reason}</p>
      {currentAlert ? <div className="health-next-action"><strong>{escalationLabels[rule.escalation]}</strong><p>{rule.suggestedAction}</p></div> : null}
      {historical ? <div className={`historical-health historical-${rule.status}`}>
        <strong>Предыдущая оценка: {healthLabels[rule.status]}</strong>
        <p>{view.evaluatedAt ? formatTime(view.evaluatedAt) : ''} · не текущий статус</p>
        <p>{rule.explanation}</p>
      </div> : null}
    </div>
    <details>
      <summary>{historical ? 'Предыдущая рекомендация и контекст' : 'Что делать и почему'}</summary>
      <div className="health-guidance">
        {!currentAlert ? <div><h4>{historical ? 'Действие в предыдущей оценке' : 'Следующий шаг'}</h4><p>{rule.suggestedAction}</p></div> : null}
        <div><h4>К кому обратиться</h4><p>{escalationLabels[rule.escalation]}</p><p className="note">Приложение не отправляет сообщения экипажу или на Землю.</p></div>
        <div><h4>Влияние космического полёта</h4><p>{rule.flightContext}</p></div>
        <Sources urls={rule.sourceUrls} />
        <p className="note">Демонстрационное правило {rule.ruleId} · {rule.ruleVersion}.{view.evaluatedAt ? ` Оценено ${formatTime(view.evaluatedAt)}.` : ''}{rule.validUntil ? ` Срок до ${formatTime(rule.validUntil)}.` : ''}</p>
      </div>
    </details>
  </div>
}

export function FlightEducationPanel({ education }: { education: FlightEducation[] }) {
  return <section className="panel flight-education" aria-label="Пять опасностей космического полёта">
    <div className="section-heading"><h2>Как полёт влияет на здоровье</h2><span className="note">5 опасностей · NASA HRP</span></div>
    <p className="note">Контекст для самонаблюдения. Доступные показатели не охватывают все риски миссии.</p>
    <div className="hazard-list">{education.map(item => <details key={item.id}>
      <summary>{item.title}</summary><div><p>{item.explanation}</p><p><strong>В повседневной работе: </strong>{item.suggestedAction}</p><Sources urls={item.sourceUrls} /></div>
    </details>)}</div>
  </section>
}

export function HealthPanel({ context, compact = false }: { context: HealthContext; compact?: boolean }) {
  const health = context.assessment?.health
  const metrics = assessmentMetricConfigs(context)
  const views = metrics.map(metric => ({ metric, view: metricHealth(metric.id, context) }))
  const status = summarizeHealth(views.map(row => row.view), metrics.length)
  const evaluatedCount = views.filter(row => row.view.current).length
  const withoutRules = views.filter(row => !row.view.rule || row.view.rule.status === 'notEvaluated').length
  const alerts = views.filter(row => row.view.rule && ['attention', 'critical'].includes(row.view.rule.status))
  return <section className={`panel health-panel health-panel-${status}${compact ? ' health-panel-compact' : ''}`} aria-label="Оценка по демонстрационным правилам">
    <div className="section-heading"><div><h2>Оценка показателей</h2><p className="note">Демонстрационные правила · {health?.ruleSetVersion ?? 'ожидание сервера'}</p></div><HealthBadge status={status} /></div>
    <p className="health-summary">{status === 'noAlert' ? 'Заданные правила не сработали. Это не заключение о здоровье всех органов.' : status === 'critical' ? 'Есть срочный сигнал. Привлеките экипаж и следуйте медицинскому протоколу миссии.' : status === 'attention' ? 'Есть показатель, требующий внимания. Проверьте объяснение и следующий шаг.' : health ? withoutRules ? 'Для части показателей автоматические правила ещё не подключены. Их можно записывать и просматривать без автоматического заключения.' : 'Текущая оценка доступна не для всех показателей. Проверьте связь, время и свежесть измерений.' : 'Сервер пока не предоставил оценку показателей.'}</p>
    <div className="health-coverage"><span>Пригодны для оценки: {evaluatedCount} / {metrics.length}</span>{withoutRules > 0 && health ? <span>Без правил: {withoutRules}</span> : null}{context.assessment ? <span>Ответ сервера: {formatTime(context.assessment.evaluatedAt)}</span> : null}</div>
    {health ? <details className="emergency-guidance" open={status === 'critical'}><summary>Когда нужна срочная помощь</summary><p>{health.emergencyGuidance}</p><p>Приложение само никому сообщения не отправляет.</p></details> : null}
    <div className="health-alerts">{alerts.length ? alerts.map(({ metric, view }) => <div key={metric.id}><div className="health-alert-title"><strong>{metric.label}</strong><HealthBadge status={view.status} /></div>{compact ? <details className="finding-details"><summary>Причина и действия</summary><HealthDetails view={view} /></details> : <HealthDetails view={view} />}</div>) : <p className="note">Предупреждений по доступным оценкам нет.</p>}</div>
    {health ? <>
      {compact ? <details className="health-disclaimer"><summary>О демонстрационных правилах</summary><p>{health.disclaimer}</p></details> : <p className="health-disclaimer">{health.disclaimer}</p>}
    </> : <p className="note">Наличие или свежесть измерения сами по себе не означают нормальное состояние.</p>}
  </section>
}

export function ResearchExplanation({ metric, context, onRecord, onHistory }: {
  metric: Metric; context: HealthContext; onRecord?: () => void; onHistory?: () => void
}) {
  const [guidance, setGuidance] = useState(false)
  const insight = context.assessment?.researchInsights?.find(item => item.metric === metric)
  if (!insight) return null
  return <details className="nasa-metric-context">
    <summary>Что это значит в полёте · NASA / CSA + ваши данные</summary>
    <p className="note">Снимок сервера {context.assessment?.evaluatedAt}. Не текущая медицинская оценка.{!context.connected ? ' Нет связи: показан сохранённый ответ.' : ''}</p>
    <p>{insight.observation}</p>
    {insight.contexts.map(item => <section key={item.title}>
      <h4>{item.title}</h4><p>{item.explanation}</p>
      <p>Как используем: {item.projectUse}</p><p>Ограничение: {item.limitation}</p>
      <a href={item.sourceUrl} target="_blank" rel="noreferrer">{item.sourceAgency} · {item.sourceSection}</a>
    </section>)}
    <p className="note">Связи и действия разработаны командой. Пороги BE-08 имеют отдельные основания. Каталог: {insight.catalogVersion}.</p>
    <div className="research-actions">{insight.actions.map(action => <button type="button" key={action.kind}
      disabled={action.kind === 'record' ? !onRecord : action.kind === 'history' ? !onHistory : false}
      onClick={() => { if (action.kind === 'record') onRecord?.(); else if (action.kind === 'history') onHistory?.(); else setGuidance(value => !value) }}>
      {action.label}</button>)}</div>
    {guidance ? <HealthDetails view={metricHealth(metric, context)} /> : null}
  </details>
}
