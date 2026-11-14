import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'
import { AnatomyPanel } from './AnatomyPanel'
import { AnatomyDiagram } from './AnatomyDiagram'
import { anatomyAppearance } from './anatomyAppearance'
import { systemHealth } from '../dashboard/health'
import { healthFixture } from '../dashboard/health.fixture'

const noSelection = () => {}

it('colors organs and skeletal regions by the worst current system status in every mode', () => {
  for (const [sleep, mood, expected] of [
    ['attention', 'noAlert', '#f2b632'], ['attention', 'critical', '#ff978c'],
    ['noAlert', 'noAlert', undefined], ['notEvaluated', 'notEvaluated', undefined],
  ] as const) {
    const context = healthFixture()
    context.assessment.health!.rules.find(rule => rule.metric === 'sleepHours')!.status = sleep
    context.assessment.health!.rules.find(rule => rule.metric === 'mood')!.status = mood
    const health = systemHealth(context)
    for (const mode of ['organs', 'skeleton', 'xray'] as const) {
      expect(anatomyAppearance(mode, 'brain', 'organ', null, null, health, 'Brain').color).toBe(expected)
      expect(anatomyAppearance(mode, 'msk', 'skeleton', null, null, health, 'Skull').color).toBe(expected)
      expect(anatomyAppearance(mode, 'msk', 'skeleton', 'msk', null, health, 'Skull')).toMatchObject({ active: true, opacity: 1 })
      const html = renderToStaticMarkup(<AnatomyDiagram mode={mode} selected={null} onSelect={noSelection} availableSystems={[]} healthStatuses={health} />)
      if (expected) expect(html).toContain(`fill="${expected}"`)
    }
    for (const invalid of [{ ...context, connected: false }, { ...context, elapsedSeconds: 86401 }, { ...context, latestElapsedSeconds: 86401 }]) {
      expect(anatomyAppearance('skeleton', 'msk', 'skeleton', null, null, systemHealth(invalid), 'Skull').color).toBeUndefined()
    }
  }
})

it('keeps all seven systems navigable before WebGL has loaded without claiming measurements exist', () => {
  const html = renderToStaticMarkup(<AnatomyPanel selected={null} onSelect={noSelection} />)
  expect(html.match(/aria-label="[^"]+: нет данных"/g)).toHaveLength(7)
  expect(html).not.toContain(': есть данные')
  expect(html).toContain('Наличие данных, не оценка здоровья')
  expect(html).toContain('aria-label="Вид карты тела"')
  expect(html).toContain('>2D</button>')
  expect(html).toContain('aria-busy="true"')
  expect(html).not.toContain('<canvas')
})

it('keeps selection independent from data availability and preserves the three anatomical layers', () => {
  const html = renderToStaticMarkup(<AnatomyPanel selected="heart" onSelect={noSelection} availableSystems={['brain']} />)
  expect(html).toContain('aria-pressed="true" aria-label="Сердце и кровообращение: нет данных"')
  expect(html).toContain('aria-pressed="false" aria-label="Сон и самочувствие: есть данные"')
  expect(html.match(/aria-label="[^"]+: есть данные"/g)).toHaveLength(1)
  expect(html).toContain('>Органы</button>')
  expect(html).toContain('>Скелет</button>')
  expect(html).toContain('>Рентген</button>')
  expect(html).toContain('>Все системы</button>')
  expect(html).toContain('href="/anatomy-credits.txt"')
})

it('retains keyboard navigation and current selection in the 2D fallback', () => {
  const html = renderToStaticMarkup(<AnatomyDiagram selected="heart" onSelect={noSelection} availableSystems={['brain', 'heart']} />)
  expect(html.match(/role="button" tabindex="0"/g)).toHaveLength(7)
  expect(html).toContain('aria-pressed="true" aria-label="Сердце и кровообращение: есть данные"')
  expect(html).toContain('aria-pressed="false" aria-label="Дыхание: нет данных"')
  expect(html.match(/aria-label="[^"]+: есть данные"/g)).toHaveLength(2)
  expect(html).toContain('anatomy-layer-skin')
  expect(html).toContain('anatomy-layer-skeleton')
  expect(html).toContain('anatomy-layer-organ')
})

it('reveals a selected organ in skeleton mode and selected bones in organs mode', () => {
  expect(anatomyAppearance('skeleton', 'heart', 'organ', 'heart')).toMatchObject({ opacity: 1, pickable: true, light: 1 })
  expect(anatomyAppearance('organs', 'msk', 'skeleton', 'msk')).toMatchObject({ opacity: 1, pickable: true, light: 1 })
  expect(anatomyAppearance('skeleton', 'heart', 'organ', null).pickable).toBe(false)
  expect(anatomyAppearance('organs', 'msk', 'skeleton', null).pickable).toBe(false)
})

it('keeps surrounding systems dim but selectable and restores their original appearance when cleared', () => {
  const dimmed = anatomyAppearance('organs', 'lungs', 'organ', 'heart')
  const hovered = anatomyAppearance('organs', 'lungs', 'organ', 'heart', 'lungs')
  const cleared = anatomyAppearance('organs', 'lungs', 'organ', null)
  expect(dimmed.light).toBeCloseTo(0.28)
  expect(dimmed.opacity).toBeLessThan(0.3)
  expect(dimmed.pickable).toBe(true)
  expect(hovered.light).toBeGreaterThan(dimmed.light)
  expect(hovered.opacity).toBeGreaterThan(dimmed.opacity)
  expect(cleared).toMatchObject({ light: 1, opacity: 1, pickable: true })
  expect(anatomyAppearance('organs', 'immune', 'skin', 'immune').pickable).toBe(false)
})

it('makes a selected organ visible and clickable in the 2D skeleton fallback too', () => {
  const html = renderToStaticMarkup(<AnatomyDiagram mode="skeleton" selected="heart" onSelect={noSelection} availableSystems={[]} />)
  const heartPaths = html.match(/<path[^>]*data-system="heart"[^>]*>/g) ?? []
  expect(heartPaths.length).toBeGreaterThan(0)
  expect(heartPaths.every(path => path.includes('opacity:1') && path.includes('pointer-events:visiblePainted'))).toBe(true)
  expect(html).toContain('Сердце и кровообращение: нет данных')
})
