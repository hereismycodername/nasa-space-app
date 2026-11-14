import { useEffect, useId, useRef, useState } from 'react'
import type { KeyboardEvent, ReactNode } from 'react'
import { ArrowLeft, ArrowRight, Minus, Plus, RotateCcw } from 'lucide-react'
import type { AnatomyMode } from './anatomySystems'
import type { AnatomyViewerController } from './createAnatomyViewer'
import type { SystemId } from './types'

export function AnatomyViewer({ mode, selected, hovered, onHover, onSelect, fallback }: {
  mode: AnatomyMode
  selected: SystemId | null
  hovered: SystemId | null
  onHover: (system: SystemId | null) => void
  onSelect: (system: SystemId | null) => void
  fallback: ReactNode
}) {
  const host = useRef<HTMLDivElement>(null)
  const viewer = useRef<AnatomyViewerController | null>(null)
  const current = useRef({ mode, selected, hovered, onHover, onSelect })
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading')
  const [attempt, setAttempt] = useState(0)
  const helpId = useId()

  useEffect(() => {
    current.current = { mode, selected, hovered, onHover, onSelect }
    viewer.current?.setAppearance(mode, selected, hovered)
  }, [mode, selected, hovered, onHover, onSelect])

  useEffect(() => {
    const container = host.current!
    let cancelled = false
    let failed = false
    let instance: AnatomyViewerController | undefined
    const fail = () => {
      if (cancelled || failed) return
      failed = true
      instance?.dispose()
      viewer.current = null
      setStatus('error')
    }
    // Keep WebGL and its loader out of the initial React bundle and the 2D path.
    void import('./createAnatomyViewer').then(({ createAnatomyViewer }) => {
      if (cancelled) return
      instance = createAnatomyViewer(container, system => {
        const latest = current.current
        latest.onSelect(latest.selected === system ? null : system)
      }, fail, system => current.current.onHover(system))
      viewer.current = instance
      instance.setAppearance(current.current.mode, current.current.selected, current.current.hovered)
      return instance.ready.then(() => { if (!cancelled && !failed) setStatus('ready') })
    }).catch(fail)
    return () => {
      cancelled = true
      instance?.dispose()
      viewer.current = null
    }
  }, [attempt])

  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const controls = viewer.current
    if (!controls || status !== 'ready') return
    switch (event.key) {
      case 'ArrowLeft': controls.rotate(-Math.PI / 12, 0); break
      case 'ArrowRight': controls.rotate(Math.PI / 12, 0); break
      case 'ArrowUp': controls.rotate(0, -Math.PI / 12); break
      case 'ArrowDown': controls.rotate(0, Math.PI / 12); break
      case '+': case '=': controls.zoom(0.8); break
      case '-': controls.zoom(1.25); break
      case 'Home': controls.reset(); break
      default: return
    }
    event.preventDefault()
  }

  return <>
    <div className="anatomy-viewport" hidden={status === 'error'}>
      <div ref={host} className="anatomy-webgl" role="group" tabIndex={0}
        aria-label="3D-модель человека" aria-describedby={helpId} aria-busy={status === 'loading'}
        onKeyDown={keyDown} />
      {status === 'loading' ? <p className="anatomy-loading" role="status">Загрузка 3D-модели…</p> : null}
    </div>
    {status === 'error' ? <>
      <div className="anatomy-fallback-message" role="status">
        <p>3D недоступно. Можно продолжить на 2D-карте.</p>
        <button type="button" onClick={() => { setStatus('loading'); setAttempt(value => value + 1) }}>Повторить загрузку 3D</button>
      </div>
      {fallback}
    </> : <>
      <div className="anatomy-camera-controls" role="group" aria-label="Управление ракурсом">
        <button type="button" disabled={status !== 'ready'} aria-label="Повернуть влево" title="Повернуть влево" onClick={() => viewer.current?.rotate(-Math.PI / 6, 0)}><ArrowLeft size={16} /></button>
        <button type="button" disabled={status !== 'ready'} aria-label="Повернуть вправо" title="Повернуть вправо" onClick={() => viewer.current?.rotate(Math.PI / 6, 0)}><ArrowRight size={16} /></button>
        <button type="button" disabled={status !== 'ready'} aria-label="Исходный ракурс" title="Исходный ракурс" onClick={() => viewer.current?.reset()}><RotateCcw size={16} /></button>
        <button type="button" disabled={status !== 'ready'} aria-label="Приблизить" title="Приблизить" onClick={() => viewer.current?.zoom(0.8)}><Plus size={16} /></button>
        <button type="button" disabled={status !== 'ready'} aria-label="Отдалить" title="Отдалить" onClick={() => viewer.current?.zoom(1.25)}><Minus size={16} /></button>
      </div>
      <p className="anatomy-gesture-hint" id={helpId}>Тяните для поворота. Колесо или два пальца — масштаб. Стрелки — поворот, +/− — масштаб, Home — сброс.</p>
    </>}
  </>
}
