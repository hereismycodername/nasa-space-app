import { memo, useId, useState } from 'react'
import type { KeyboardEvent } from 'react'
import { anatomyParts } from './anatomyData'
import type { SystemId } from './types'
import './anatomy.css'

export type { SystemId } from './types'

type Mode = 'organs' | 'skeleton' | 'xray'
interface Props {
  selected: SystemId | null
  onSelect: (id: SystemId | null) => void
  availableSystems?: SystemId[]
}

const modes: { id: Mode; label: string }[] = [
  { id: 'organs', label: 'Органы' },
  { id: 'skeleton', label: 'Скелет' },
  { id: 'xray', label: 'Рентген' },
]
const callouts: { system: SystemId; label: string; fullLabel: string; x: number; y: number; labelY: number; side: 'left' | 'right' }[] = [
  { system: 'brain', label: 'Мозг', fullLabel: 'Сон и самочувствие', x: 136, y: 24, labelY: 24, side: 'right' },
  { system: 'eyes', label: 'Зрение', fullLabel: 'Зрение', x: 106, y: 43, labelY: 43, side: 'left' },
  { system: 'lungs', label: 'Дыхание', fullLabel: 'Дыхание', x: 96, y: 140, labelY: 128, side: 'left' },
  { system: 'heart', label: 'Сердце', fullLabel: 'Сердце и кровообращение', x: 136, y: 158, labelY: 150, side: 'right' },
  { system: 'gut', label: 'Обмен', fullLabel: 'Обмен веществ', x: 98, y: 208, labelY: 214, side: 'left' },
  { system: 'msk', label: 'Кости', fullLabel: 'Кости и мышцы', x: 153, y: 330, labelY: 318, side: 'right' },
  { system: 'immune', label: 'Всё тело', fullLabel: 'Температура тела', x: 81, y: 300, labelY: 304, side: 'left' },
]
const emptySystems: SystemId[] = []

/** Interactive navigation through the same vector anatomy as the native Crew UI. */
export const AnatomyPanel = memo(function AnatomyPanel({ selected, onSelect, availableSystems = emptySystems }: Props) {
  const [mode, setMode] = useState<Mode>('organs')
  const titleId = useId()
  const choose = (system: SystemId) => onSelect(selected === system ? null : system)
  const activate = (event: KeyboardEvent<SVGGElement>, system: SystemId) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      choose(system)
    }
  }
  return <div className="anatomy-panel">
    <div className="anatomy-heading">
      <h2>Карта тела</h2>
      <div className="anatomy-modes" role="group" aria-label="Слой анатомической схемы">
        {modes.map(item => <button key={item.id} type="button" aria-pressed={mode === item.id}
          onClick={() => setMode(item.id)}>{item.label}</button>)}
      </div>
    </div>
    <div className={`anatomy-canvas anatomy-mode-${mode}`}>
      <svg viewBox="-45 0 330 515" role="group" aria-labelledby={titleId} className="anatomy-figure">
        <title id={titleId}>Анатомическая карта: выберите систему, чтобы открыть связанные показатели</title>
        <g aria-hidden="true">
          {anatomyParts.map((part, index) => <path key={index} d={part.d}
            data-system={part.system}
            className={`anatomy-part anatomy-layer-${part.layer}${part.system === selected ? ' anatomy-selected' : ''}${part.hit ? ' anatomy-clickable' : ''}`}
            fill={part.width === 0 ? part.color : 'none'}
            stroke={part.width === 0 ? (part.layer === 'skin' ? '#40546c' : part.color) : part.color}
            strokeWidth={part.width || 0.65} strokeLinejoin="round" strokeLinecap="round"
            onClick={part.hit ? () => choose(part.system) : undefined} />)}
        </g>
        {callouts.map(callout => {
          const isSelected = selected === callout.system
          const available = availableSystems.includes(callout.system)
          const left = callout.side === 'left'
          const textX = left ? -37 : 277
          const edgeX = left ? 36 : 204
          return <g key={callout.system} role="button" tabIndex={0} aria-pressed={isSelected}
            aria-label={`${callout.fullLabel}: ${available ? 'есть данные' : 'нет данных'}`}
            onClick={() => choose(callout.system)} onKeyDown={event => activate(event, callout.system)}
            className={`anatomy-callout${isSelected ? ' anatomy-callout-selected' : ''}${available ? ' anatomy-available' : ''}`}>
            <rect className="anatomy-label-target" x={left ? -41 : 201} y={callout.labelY - 17} width={80} height={34} rx={4} />
            <path className="anatomy-leader" d={`M${callout.x},${callout.y} L${callout.x + (left ? -14 : 14)},${callout.labelY} L${edgeX},${callout.labelY}`} />
            <circle className="anatomy-marker-ring" cx={callout.x} cy={callout.y} r={5.5} />
            <circle className="anatomy-marker" cx={callout.x} cy={callout.y} r={3} />
            <text className="anatomy-label" x={textX} y={callout.labelY - 3} textAnchor={left ? 'start' : 'end'}>{callout.label}</text>
            <text className="anatomy-state" x={textX} y={callout.labelY + 10} textAnchor={left ? 'start' : 'end'}>{available ? 'Есть данные' : 'Нет данных'}</text>
          </g>
        })}
      </svg>
    </div>
    <div className="anatomy-caption">
      <span><i className="anatomy-legend-dot" aria-hidden="true" /> Наличие данных, не оценка здоровья</span>
      {selected ? <button type="button" onClick={() => onSelect(null)}>Все системы</button> : <p>Выберите орган или подпись</p>}
    </div>
  </div>
}, (previous, next) => previous.selected === next.selected && previous.onSelect === next.onSelect
  && (previous.availableSystems ?? emptySystems).join(',') === (next.availableSystems ?? emptySystems).join(','))
