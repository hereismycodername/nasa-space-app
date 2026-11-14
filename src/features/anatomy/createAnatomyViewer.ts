import {
  AmbientLight, Box3, DirectionalLight, HemisphereLight, Mesh, MeshStandardMaterial,
  PerspectiveCamera, Raycaster, Scene, Spherical, Vector2, Vector3, WebGLRenderer,
} from 'three'
import type { Color, Object3D } from 'three'
import { OrbitControls } from 'three/addons/controls/OrbitControls.js'
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js'
import { anatomySystems } from './anatomySystems'
import type { AnatomyMode } from './anatomySystems'
import { anatomyAppearance } from './anatomyAppearance'
import type { AnatomyLayer } from './anatomyAppearance'
import type { SystemId } from './types'

export interface AnatomyViewerController {
  ready: Promise<void>
  setAppearance: (mode: AnatomyMode, selected: SystemId | null, hovered?: SystemId | null) => void
  rotate: (horizontal: number, vertical: number) => void
  zoom: (factor: number) => void
  reset: () => void
  dispose: () => void
}

type BodyMesh = Mesh<import('three').BufferGeometry, MeshStandardMaterial>
const systemIds = new Set<string>(anatomySystems.map(({ system }) => system))

function disposeModel(model: Object3D) {
  model.traverse(object => {
    if (!(object instanceof Mesh)) return
    object.geometry.dispose()
    const materials = Array.isArray(object.material) ? object.material : [object.material]
    for (const material of materials) material.dispose()
  })
}

/** One local GLB, no remote textures/decoders, no animation loop when the view is idle. */
export function createAnatomyViewer(host: HTMLDivElement, onSelect: (system: SystemId) => void, onFailure: () => void,
  onHover: (system: SystemId | null) => void): AnatomyViewerController {
  const renderer = new WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'low-power' })
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.5))
  renderer.setClearColor(0x131a23, 0)
  const canvas = renderer.domElement
  canvas.setAttribute('aria-hidden', 'true')
  host.append(canvas)

  const scene = new Scene()
  const camera = new PerspectiveCamera(36, 1, 0.05, 30)
  const controls = new OrbitControls(camera, canvas)
  controls.enablePan = false
  controls.enableDamping = false
  controls.rotateSpeed = 0.7
  controls.zoomSpeed = 0.7
  controls.minPolarAngle = Math.PI / 10
  controls.maxPolarAngle = Math.PI * 0.9
  controls.minDistance = 1.3
  controls.maxDistance = 9
  controls.target.set(0, 0.05, 0)
  scene.add(new AmbientLight(0xffffff, 0.7), new HemisphereLight(0xe9f2ff, 0x394454, 2))
  const key = new DirectionalLight(0xfff2e5, 2.6)
  key.position.set(3, 3, 4)
  const fill = new DirectionalLight(0xb3d5ff, 1.8)
  fill.position.set(-3, 1, -3)
  scene.add(key, fill)

  let disposed = false
  let frame = 0
  let model: Object3D | undefined
  let bodySize = new Vector3(0.8, 2, 0.3)
  let mode: AnatomyMode = 'organs'
  let selected: SystemId | null = null
  let hovered: SystemId | null = null
  const meshes: BodyMesh[] = []
  const originalColors = new Map<BodyMesh, Color>()
  const abort = new AbortController()
  const invalidate = () => {
    if (disposed || frame) return
    frame = requestAnimationFrame(() => {
      frame = 0
      if (!disposed) renderer.render(scene, camera)
    })
  }
  controls.addEventListener('change', invalidate)

  function reset() {
    const vertical = (bodySize.y / 2 + 0.12) / Math.tan(camera.fov * Math.PI / 360)
    const horizontal = (bodySize.x / 2 + 0.06) / (Math.tan(camera.fov * Math.PI / 360) * camera.aspect)
    const distance = Math.max(vertical, horizontal) + bodySize.z / 2
    controls.target.set(0, 0.02, 0)
    camera.position.set(0, 0.02, Math.min(controls.maxDistance, distance))
    controls.update()
    invalidate()
  }

  const resize = () => {
    if (disposed || !host.clientWidth || !host.clientHeight) return
    camera.aspect = host.clientWidth / host.clientHeight
    camera.updateProjectionMatrix()
    renderer.setSize(host.clientWidth, host.clientHeight, false)
    invalidate()
  }
  const observer = new ResizeObserver(resize)
  observer.observe(host)
  resize()
  reset()

  const setAppearance = (nextMode: AnatomyMode, nextSelected: SystemId | null, nextHovered: SystemId | null = null) => {
    mode = nextMode
    selected = nextSelected
    hovered = nextHovered
    for (const mesh of meshes) {
      const appearance = anatomyAppearance(mode, mesh.userData.system, mesh.userData.layer, selected, hovered)
      const material = mesh.material
      mesh.visible = true
      material.color.copy(originalColors.get(mesh)!).multiplyScalar(appearance.light)
      material.opacity = appearance.opacity
      const transparent = material.opacity < 1
      if (material.transparent !== transparent) {
        material.transparent = transparent
        material.needsUpdate = true
      }
      material.depthWrite = !transparent
      material.emissive.set(appearance.active ? 0xc8d7e8 : 0x000000)
      material.emissiveIntensity = appearance.active ? 0.07 : 0
    }
    invalidate()
  }

  const raycaster = new Raycaster()
  const pointer = new Vector2()
  const touches = new Map<number, { x: number; y: number }>()
  let dragged = false
  const updateHover = (system: SystemId | null) => {
    if (system === hovered) return
    setAppearance(mode, selected, system)
    onHover(system)
  }
  const pick = (event: PointerEvent) => {
    const rect = canvas.getBoundingClientRect()
    if (!rect.width || !rect.height) return null
    pointer.set((event.clientX - rect.left) / rect.width * 2 - 1, -(event.clientY - rect.top) / rect.height * 2 + 1)
    scene.updateMatrixWorld(true)
    camera.updateMatrixWorld(true)
    raycaster.setFromCamera(pointer, camera)
    const pickable = meshes.filter(mesh => mesh.visible && anatomyAppearance(mode,
      mesh.userData.system as SystemId, mesh.userData.layer as AnatomyLayer, selected, hovered).pickable)
    const hits = raycaster.intersectObjects(pickable, false)
    // A ghosted foreground organ must not intercept a click on the selected volume.
    const hit = hits.find(entry => entry.object.userData.system === selected) ?? hits[0]
    return hit ? hit.object.userData.system as SystemId : null
  }
  const pointerDown = (event: PointerEvent) => {
    if (!touches.size) dragged = event.button !== 0
    touches.set(event.pointerId, { x: event.clientX, y: event.clientY })
    if (touches.size > 1) dragged = true
    updateHover(null)
  }
  const pointerMove = (event: PointerEvent) => {
    const start = touches.get(event.pointerId)
    if (start && Math.hypot(event.clientX - start.x, event.clientY - start.y) > 6) dragged = true
    if (!touches.size && event.pointerType !== 'touch') updateHover(pick(event))
  }
  const pointerUp = (event: PointerEvent) => {
    const start = touches.get(event.pointerId)
    touches.delete(event.pointerId)
    if (!start || dragged || touches.size || Math.hypot(event.clientX - start.x, event.clientY - start.y) > 6) return
    const system = pick(event)
    if (system) onSelect(system)
  }
  const pointerCancel = (event: PointerEvent) => { touches.delete(event.pointerId); dragged = true }
  const pointerLeave = () => updateHover(null)
  const contextLost = (event: Event) => { event.preventDefault(); if (!disposed) onFailure() }
  canvas.addEventListener('pointerdown', pointerDown, true)
  canvas.addEventListener('pointermove', pointerMove, true)
  canvas.addEventListener('pointerup', pointerUp, true)
  canvas.addEventListener('pointercancel', pointerCancel, true)
  canvas.addEventListener('pointerleave', pointerLeave)
  canvas.addEventListener('lostpointercapture', pointerCancel)
  canvas.addEventListener('webglcontextlost', contextLost)

  const timeout = window.setTimeout(() => abort.abort(), 30_000)
  const ready = (async () => {
    try {
      const response = await fetch(`${import.meta.env.BASE_URL}anatomy/body.glb`, { signal: abort.signal })
      if (!response.ok) throw new Error(`Anatomy model: HTTP ${response.status}`)
      const buffer = await response.arrayBuffer()
      if (disposed) return
      const gltf = await new GLTFLoader().parseAsync(buffer, '')
      if (disposed) { disposeModel(gltf.scene); return }
      model = gltf.scene
      model.traverse(object => {
        if (!(object instanceof Mesh) || !(object.material instanceof MeshStandardMaterial)
          || !systemIds.has(object.userData.system)
          || !['skin', 'skeleton', 'organ'].includes(object.userData.layer)) return
        meshes.push(object as BodyMesh)
        originalColors.set(object as BodyMesh, object.material.color.clone())
      })
      if (!meshes.length) throw new Error('Anatomy model has no selectable meshes')
      const bounds = new Box3().setFromObject(model)
      const size = bounds.getSize(new Vector3())
      if (!(size.y > 0) || !Number.isFinite(size.length())) throw new Error('Invalid anatomy bounds')
      const center = bounds.getCenter(new Vector3())
      const scale = 2 / size.y
      model.scale.setScalar(scale)
      model.position.copy(center).multiplyScalar(-scale)
      bodySize = size.multiplyScalar(scale)
      scene.add(model)
      setAppearance(mode, selected, hovered)
      reset()
    } finally {
      clearTimeout(timeout)
    }
  })()

  return {
    ready, setAppearance, reset,
    rotate(horizontal, vertical) {
      const offset = camera.position.clone().sub(controls.target)
      const spherical = new Spherical().setFromVector3(offset)
      spherical.theta += horizontal
      spherical.phi = Math.max(controls.minPolarAngle, Math.min(controls.maxPolarAngle, spherical.phi + vertical))
      camera.position.copy(controls.target).add(offset.setFromSpherical(spherical))
      controls.update()
    },
    zoom(factor) {
      const offset = camera.position.clone().sub(controls.target)
      const distance = Math.max(controls.minDistance, Math.min(controls.maxDistance, offset.length() * factor))
      camera.position.copy(controls.target).add(offset.setLength(distance))
      controls.update()
    },
    dispose() {
      if (disposed) return
      disposed = true
      abort.abort()
      clearTimeout(timeout)
      cancelAnimationFrame(frame)
      observer.disconnect()
      controls.removeEventListener('change', invalidate)
      controls.dispose()
      canvas.removeEventListener('pointerdown', pointerDown, true)
      canvas.removeEventListener('pointermove', pointerMove, true)
      canvas.removeEventListener('pointerup', pointerUp, true)
      canvas.removeEventListener('pointercancel', pointerCancel, true)
      canvas.removeEventListener('pointerleave', pointerLeave)
      canvas.removeEventListener('lostpointercapture', pointerCancel)
      canvas.removeEventListener('webglcontextlost', contextLost)
      if (model) disposeModel(model)
      renderer.dispose()
      renderer.forceContextLoss()
      canvas.remove()
    },
  }
}
