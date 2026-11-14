import { formatUnit, formatValue } from './features/dashboard/metrics'
import { useState } from 'react'
import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import type { TooltipContentProps } from 'recharts'
import type { MeasurementSeries, MeasurementSeriesPoint } from './shared/types'
import { buildSeriesChart, getHoveredSeriesPoint } from './data/seriesChart'
import type { SeriesHover } from './data/seriesChart'

const axisDate = new Intl.DateTimeFormat('ru-RU', {
  timeZone: 'UTC', day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit',
})
const periodDate = new Intl.DateTimeFormat('ru-RU', {
  timeZone: 'UTC', year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
})
const compactDate = new Intl.DateTimeFormat('ru-RU', { timeZone: 'UTC', day: '2-digit', month: '2-digit' })
const compactTime = new Intl.DateTimeFormat('ru-RU', { timeZone: 'UTC', hour: '2-digit', minute: '2-digit' })
const number = new Intl.NumberFormat('ru-RU', { maximumFractionDigits: 2 })

function formatInterval(milliseconds: number) {
  if (milliseconds >= 3_600_000) return `${number.format(milliseconds / 3_600_000)} ч`
  if (milliseconds >= 60_000) return `${number.format(milliseconds / 60_000)} мин`
  if (milliseconds >= 1000) return `${number.format(milliseconds / 1000)} с`
  return `${milliseconds} мс`
}

function SeriesTooltip({ active, payload, series, hoveredPoint }: TooltipContentProps & { series: MeasurementSeries; hoveredPoint?: MeasurementSeriesPoint }) {
  const point = hoveredPoint ?? payload?.[0]?.payload as MeasurementSeriesPoint | undefined
  if (!active || !point) return null
  return <div style={{ background: '#182c39', padding: 12, border: '1px solid #344755',
    maxWidth: 'min(260px, 70vw)', whiteSpace: 'normal', overflowWrap: 'anywhere' }}>
    <p>{periodDate.format(new Date(point.startAt))} – {periodDate.format(new Date(point.endAt))} UTC</p>
    <p>{point.value === null ? 'Нет данных' : `${series.aggregation === 'mean' ? 'Среднее' : 'Последнее'}: ${number.format(point.value)} ${formatUnit(series.unit, series.metric)}`}</p>
    {point.min !== null && point.max !== null ? <p>Мин–макс: {number.format(point.min)}–{number.format(point.max)} {formatUnit(series.unit, series.metric)}</p> : null}
    <p>Измерений: {point.count}</p>
    {point.receivedCount > 0 ? <p>Без времени измерения: {point.receivedCount}; сгруппированы по времени получения.</p> : null}
  </div>
}

function SeriesDot({ cx, cy, payload, onEnter, onLeave }: {
  cx?: number
  cy?: number
  payload?: MeasurementSeriesPoint
  onEnter: (startAt: string) => void
  onLeave: () => void
}) {
  if (cx === undefined || cy === undefined || !payload?.count) return null
  return <circle className="recharts-line-dot" cx={cx} cy={cy} r={3}
    fill="#97b6df" stroke="#97b6df" strokeWidth={1}
    onPointerEnter={() => onEnter(payload.startAt)} onPointerLeave={onLeave} />
}

export function MeasurementSeriesChart({ series, compact = false, syncId }: { series?: MeasurementSeries; compact?: boolean; syncId?: string }) {
  const [hovered, setHovered] = useState<SeriesHover | null>(null)
  if (!series) return <p>Загрузка истории за 7 дней…</p>
  const chart = buildSeriesChart(series)
  const hasData = series.points.some(point => point.count > 0)
  const hoveredPoint = getHoveredSeriesPoint(series, hovered)
  const tickDate = compact ? (Date.parse(series.to) - Date.parse(series.from) <= 86_400_000 ? compactTime : compactDate) : axisDate
  return <>
    {!compact ? <p className="note">Период: {periodDate.format(new Date(series.from))} – {periodDate.format(new Date(series.to))} UTC.</p> : null}
    <p className="note">
      {series.aggregation === 'mean' ? 'Среднее за интервал' : 'Последнее значение за интервал'}
      {' · '}Интервал: {formatInterval(series.intervalMs)} · Единица: {formatUnit(series.unit, series.metric)}.
    </p>
    <div style={{ height: compact ? 150 : 240, marginTop: compact ? 10 : 24 }}>
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={chart.points} syncId={syncId} syncMethod="value" margin={compact ? { top: 5, right: 20, left: 0, bottom: 5 } : undefined}>
          <CartesianGrid stroke="#29323e" vertical={false} />
          <XAxis dataKey="time" type="number" scale="time" domain={chart.domain} ticks={chart.ticks}
            allowDataOverflow interval="preserveStartEnd" minTickGap={32} tickFormatter={value => tickDate.format(new Date(value))} />
          <YAxis tickFormatter={formatValue} width={compact ? 48 : 96} domain={['auto', 'auto']} unit={compact ? undefined : ` ${formatUnit(series.unit, series.metric)}`} />
          <Tooltip filterNull={false} active={hoveredPoint ? true : undefined}
            content={props => <SeriesTooltip {...props} series={series} hoveredPoint={hoveredPoint} />} />
          <Line dataKey="value" name={series.aggregation === 'mean' ? 'Среднее' : 'Последнее'} stroke="#97b6df"
            dot={<SeriesDot onEnter={startAt => setHovered({ metric: series.metric, startAt })} onLeave={() => setHovered(null)} />}
            activeDot={false} connectNulls={false} isAnimationActive={false} />
        </LineChart>
      </ResponsiveContainer>
    </div>
    {!hasData ? <p>За выбранный период нет данных.</p> : null}
    {!compact ? <p className="note">Разрывы графика обозначают интервалы без данных.</p> : null}
    {series.timeBasis === 'receivedAt' || series.timeBasis === 'mixed' ? <p className="note">
      Есть записи с неизвестным временем измерения: они сгруппированы по времени получения. Их свежесть не подтверждена.
    </p> : null}
  </>
}
