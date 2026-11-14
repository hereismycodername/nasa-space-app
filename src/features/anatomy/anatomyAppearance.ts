import type { AnatomyMode } from './anatomySystems'
import type { SystemId } from './types'

export type AnatomyLayer = 'skin' | 'skeleton' | 'organ'

/** Shared selection rules for the volume and its accessible 2D fallback. */
export function anatomyAppearance(mode: AnatomyMode, system: SystemId, layer: AnatomyLayer,
  selected: SystemId | null, hovered: SystemId | null = null) {
  const active = system === selected
  const hover = system === hovered
  const focus = !selected || active ? 1 : hover ? 0.65 : 0
  if (layer === 'skin') {
    return { active: false, light: selected ? 0.7 : 1, opacity: selected ? 0.065 : 0.12, pickable: false }
  }
  const base = layer === 'skeleton' ? (mode === 'organs' ? 0.16 : mode === 'xray' ? 0.65 : 1)
    : (mode === 'skeleton' ? 0.1 : mode === 'xray' ? 0.82 : 1)
  // Dimmed surroundings become translucent so that the chosen volume is still visible
  // through its neighbours, while its own front and back surfaces keep real depth.
  const opacity = active ? 1 : hover ? Math.max(base, 0.6) : selected ? Math.min(base, 0.22) : base
  return { active, light: 0.28 + 0.72 * focus, opacity, pickable: active || hover || base >= 0.3 }
}
