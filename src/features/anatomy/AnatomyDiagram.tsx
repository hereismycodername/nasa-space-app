import { useId } from 'react'
import type { KeyboardEvent } from 'react'
import { anatomyParts, ANATOMY_ANCHORS } from './anatomyData'
import { anatomySystems } from './anatomySystems'
import type { AnatomyMode } from './anatomySystems'
import { anatomyAppearance } from './anatomyAppearance'
import type { SystemId } from './types'

const callouts = anatomySystems.map(callout => ({ ...callout, ...ANATOMY_ANCHORS[callout.system] }))

export function AnatomyDiagram({ mode = 'organs', selected, hovered = null, onHover, onSelect, availableSystems }: {
  mode?: AnatomyMode
  selected: SystemId | null
  hovered?: SystemId | null
  onHover?: (system: SystemId | null) => void
  onSelect: (system: SystemId | null) => void
  availableSystems: SystemId[]
}) {
  const titleId = useId()
  const order = (part: typeof anatomyParts[number]) => part.layer === 'skin' ? 0 : part.system === selected ? 3
    : part.layer === (mode === 'organs' ? 'organ' : 'skeleton') ? 2 : 1
  const parts = anatomyParts.toSorted((a, b) => order(a) - order(b))
  const choose = (system: SystemId) => onSelect(selected === system ? null : system)
  const activate = (event: KeyboardEvent<SVGGElement>, system: SystemId) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      choose(system)
    }
  }
  return (
      <svg viewBox="-45 0 330 515" role="group" aria-labelledby={titleId} className="anatomy-figure" onPointerLeave={() => onHover?.(null)}>
        <title id={titleId}>Анатомическая карта: выберите систему, чтобы открыть связанные показатели</title>
        <g aria-hidden="true">
          {parts.map((part, index) => {
            const appearance = anatomyAppearance(mode, part.system, part.layer, selected, hovered)
            // The flat skin is the backdrop; keep its silhouette readable behind the organs.
            const opacity = part.layer === 'skin' ? 1 : appearance.opacity
            return <path key={index} d={part.d}
            data-system={part.system}
            className={`anatomy-part anatomy-layer-${part.layer}${part.system === selected ? ' anatomy-selected' : ''}${part.hit ? ' anatomy-clickable' : ''}`}
            style={{ opacity, filter: `brightness(${appearance.light})`, pointerEvents: part.hit && appearance.pickable ? 'visiblePainted' : 'none' }}
            fill={part.width === 0 ? part.color : 'none'}
            stroke={part.width === 0 ? (part.layer === 'skin' ? '#40546c' : part.color) : part.color}
            strokeWidth={part.width || 0.65} strokeLinejoin="round" strokeLinecap="round"
            onPointerEnter={() => onHover?.(part.system)}
            onClick={part.hit && appearance.pickable ? () => choose(part.system) : undefined} />
          })}
        </g>
        {callouts.map(callout => {
          const isSelected = selected === callout.system
          const available = availableSystems.includes(callout.system)
          const quiet = selected !== null && !isSelected && hovered !== callout.system
          const left = callout.side === 'left'
          const textX = callout.labelX
          const edgeX = left ? 36 : 204
          return <g key={callout.system} role="button" tabIndex={0} aria-pressed={isSelected}
            aria-label={`${callout.fullLabel}: ${available ? 'есть данные' : 'нет данных'}`}
            onClick={() => choose(callout.system)} onKeyDown={event => activate(event, callout.system)}
            onPointerEnter={() => onHover?.(callout.system)} onPointerLeave={() => onHover?.(null)}
            onFocus={() => onHover?.(callout.system)} onBlur={() => onHover?.(null)}
            className={`anatomy-callout${isSelected ? ' anatomy-callout-selected' : ''}${quiet ? ' anatomy-callout-quiet' : ''}${available ? ' anatomy-available' : ''}`}>
            <rect className="anatomy-label-target" x={left ? -41 : 201} y={callout.labelY - 17} width={80} height={34} />
            <path className="anatomy-leader" d={`M${callout.x},${callout.y} L${callout.x + (left ? -14 : 14)},${callout.labelY} L${edgeX},${callout.labelY}`} />
            <rect className="anatomy-marker-ring" x={callout.x - 5.5} y={callout.y - 5.5} width={11} height={11} />
            <rect className="anatomy-marker" x={callout.x - 2.5} y={callout.y - 2.5} width={5} height={5} />
            <text className="anatomy-label" x={textX} y={callout.labelY - 3} textAnchor={left ? 'start' : 'end'}>{callout.label}</text>
            <text className="anatomy-state" x={textX} y={callout.labelY + 10} textAnchor={left ? 'start' : 'end'}>{available ? 'Есть данные' : 'Нет данных'}</text>
          </g>
        })}
      </svg>
  )
}
