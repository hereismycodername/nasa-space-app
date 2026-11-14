import type { AnatomyMode } from './anatomySystems'
import type { SystemId } from './types'
import type { RuleStatus } from '../../shared/types'

export type AnatomyLayer = 'skin' | 'skeleton' | 'organ'
export type AnatomyHealth = Partial<Record<SystemId, RuleStatus>>

// The source model groups all bones under msk. Share the regional projection
// between the GLB meshes and the 2D paths without changing selection targets.
function regionSystems(system: SystemId, layer: AnatomyLayer, name: string): SystemId[] {
  name = name.replaceAll('_', ' ')
  if (layer !== 'skeleton') return [system]
  if (name === 'Skull') return ['msk', 'brain', 'eyes']
  if (name === 'Rib cage' || name === 'Sternum') return ['msk', 'heart', 'lungs', 'immune']
  if (name === 'Vertebral column' || name === 'Pelvis') return ['msk', 'gut']
  return ['msk']
}

export function anatomyHealthColor(system: SystemId, layer: AnatomyLayer, name: string, health: AnatomyHealth = {}) {
  if (layer === 'skin') return undefined
  const statuses = regionSystems(system, layer, name).map(id => health[id])
  if (statuses.includes('critical')) return '#ff978c'
  if (statuses.includes('attention')) return '#f2b632'
  return undefined
}

/** Shared selection rules for the volume and its accessible 2D fallback. */
export function anatomyAppearance(mode: AnatomyMode, system: SystemId, layer: AnatomyLayer,
  selected: SystemId | null, hovered: SystemId | null = null, health: AnatomyHealth = {}, name = '') {
  const color = anatomyHealthColor(system, layer, name, health)
  const active = system === selected
  const hover = system === hovered
  const focus = !selected || active ? 1 : hover ? 0.65 : 0
  if (layer === 'skin') {
    return { active: false, light: selected ? 0.7 : 1, opacity: selected ? 0.065 : 0.12, pickable: false, color }
  }
  const base = layer === 'skeleton' ? (mode === 'organs' ? 0.16 : mode === 'xray' ? 0.65 : 1)
    : (mode === 'skeleton' ? 0.1 : mode === 'xray' ? 0.82 : 1)
  // Dimmed surroundings become translucent so that the chosen volume is still visible
  // through its neighbours, while its own front and back surfaces keep real depth.
  const opacity = active ? 1 : hover ? Math.max(base, 0.6) : selected ? Math.min(base, 0.22) : base
  return { active, light: color ? Math.max(0.75, 0.28 + 0.72 * focus) : 0.28 + 0.72 * focus,
    opacity: color && !selected ? Math.max(base, 0.55) : opacity, pickable: active || hover || base >= 0.3, color }
}
