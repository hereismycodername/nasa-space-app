import { memo, useState } from 'react'
import { AnatomyDiagram } from './AnatomyDiagram'
import { AnatomyViewer } from './AnatomyViewer'
import { anatomySystems } from './anatomySystems'
import type { AnatomyMode } from './anatomySystems'
import type { SystemId } from './types'
import type { RuleStatus } from '../../shared/types'
import { healthLabels } from '../dashboard/health'
import './anatomy.css'

export type { SystemId } from './types'

interface Props {
  selected: SystemId | null
  onSelect: (id: SystemId | null) => void
  availableSystems?: SystemId[]
  healthStatuses?: Partial<Record<SystemId, RuleStatus>>
}

const modes: { id: AnatomyMode; label: string }[] = [
  { id: 'organs', label: 'Органы' },
  { id: 'skeleton', label: 'Скелет' },
  { id: 'xray', label: 'Рентген' },
]
const emptySystems: SystemId[] = []

export const AnatomyPanel = memo(function AnatomyPanel({ selected, onSelect, availableSystems = emptySystems, healthStatuses }: Props) {
  const [mode, setMode] = useState<AnatomyMode>('organs')
  const [view, setView] = useState<'3d' | '2d'>('3d')
  const [hovered, setHovered] = useState<SystemId | null>(null)
  const diagram = <AnatomyDiagram mode={mode} selected={selected} hovered={hovered} onHover={setHovered} onSelect={onSelect} availableSystems={availableSystems} healthStatuses={healthStatuses} />

  return <div className="anatomy-panel">
    <div className="anatomy-heading">
      <h2>Карта тела</h2>
      <div className="anatomy-modes" role="group" aria-label="Вид карты тела">
        <button type="button" aria-pressed={view === '3d'} onClick={() => setView('3d')}>3D</button>
        <button type="button" aria-pressed={view === '2d'} onClick={() => setView('2d')}>2D</button>
      </div>
    </div>
    <div className="anatomy-layer-controls anatomy-modes" role="group" aria-label="Слой анатомической схемы">
      {modes.map(item => <button key={item.id} type="button" aria-pressed={mode === item.id}
        onClick={() => setMode(item.id)}>{item.label}</button>)}
    </div>
    <div className={`anatomy-canvas anatomy-mode-${mode}${selected ? ' anatomy-has-selection' : ''}`}>
      {view === '3d'
        ? <AnatomyViewer mode={mode} selected={selected} hovered={hovered} onHover={setHovered} onSelect={onSelect} fallback={diagram} healthStatuses={healthStatuses} />
        : diagram}
    </div>
    {view === '3d' ? <div className="anatomy-systems" role="group" aria-label="Системы организма">
      {anatomySystems.map(({ system, label, fullLabel }) => {
        const available = availableSystems.includes(system)
        return <button type="button" key={system} aria-pressed={selected === system}
          aria-label={`${fullLabel}: ${available ? 'есть данные' : 'нет данных'}`}
          onPointerEnter={() => setHovered(system)} onPointerLeave={() => setHovered(null)}
          onFocus={() => setHovered(system)} onBlur={() => setHovered(null)}
          onClick={() => onSelect(selected === system ? null : system)}>
          <i className={available ? 'has-data' : ''} aria-hidden="true" />
          <span>{label}<small>{available ? 'Есть данные' : 'Нет данных'}</small>{healthStatuses?.[system] ? <span className={`anatomy-health-status health-${healthStatuses[system]}`}>{healthLabels[healthStatuses[system]!]}</span> : null}</span>
        </button>
      })}
    </div> : null}
    <div className="anatomy-caption">
      <span><i className="anatomy-legend-dot" aria-hidden="true" /> Наличие данных, не оценка здоровья</span>
      {healthStatuses ? <p>Жёлтый — требуется внимание, красный — срочная помощь по демо-правилам. Это оценка показателей системы, не диагноз органа.</p> : null}
      {selected ? <button type="button" onClick={() => onSelect(null)}>Все системы</button> : <p>Выберите орган или подпись</p>}
      <a className="anatomy-credits" href="/anatomy-credits.txt" target="_blank" rel="noreferrer">Анатомия: BodyParts3D · условия использования</a>
    </div>
  </div>
})
