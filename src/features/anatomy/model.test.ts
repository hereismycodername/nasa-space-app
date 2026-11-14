/// <reference types="node" />
import { readFile } from 'node:fs/promises'
import { Box3, Mesh, MeshStandardMaterial, Vector3 } from 'three'
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js'
import { expect, it } from 'vitest'

it('ships a complete, local, lightweight 3D model that the production loader can select and style', async () => {
  // Read the shipped file so a missing/untracked asset fails npm run check in CI.
  const bytes = await readFile(new URL('../../../public/anatomy/body.glb', import.meta.url))
  expect(bytes.length).toBeLessThanOrEqual(3_000_000)
  expect(bytes.readUInt32LE(0)).toBe(0x46546c67)
  expect(bytes.readUInt32LE(4)).toBe(2)
  expect(bytes.readUInt32LE(8)).toBe(bytes.length)
  const json = JSON.parse(bytes.subarray(20, 20 + bytes.readUInt32LE(12)).toString()) as {
    buffers: { uri?: string }[]
    images?: unknown[]
    extensionsRequired?: string[]
  }
  expect(json.buffers.every(buffer => buffer.uri === undefined)).toBe(true)
  expect(json.images ?? []).toEqual([])
  expect(json.extensionsRequired ?? []).toEqual([])

  const { scene } = await new GLTFLoader().parseAsync(Uint8Array.from(bytes).buffer, '')
  const meshes: Mesh[] = []
  scene.traverse(object => { if (object instanceof Mesh) meshes.push(object) })
  expect(meshes).toHaveLength(40)
  expect([...new Set(meshes.map(mesh => mesh.userData.system))].sort())
    .toEqual(['brain', 'eyes', 'gut', 'heart', 'immune', 'lungs', 'msk'])
  expect([...new Set(meshes.map(mesh => mesh.userData.layer))].sort())
    .toEqual(['organ', 'skeleton', 'skin'])
  // Selection and layer opacity must never mutate another organ's material.
  expect(new Set(meshes.map(mesh => mesh.material)).size).toBe(meshes.length)
  let triangles = 0
  for (const mesh of meshes) {
    expect(mesh.material).toBeInstanceOf(MeshStandardMaterial)
    const positions = mesh.geometry.getAttribute('position')
    const indices = mesh.geometry.index!
    expect(Array.from(positions.array).every(Number.isFinite)).toBe(true)
    expect(Array.from(indices.array).every(index => index >= 0 && index < positions.count)).toBe(true)
    triangles += indices.count / 3
  }
  expect(triangles).toBeLessThanOrEqual(100_000)
  const size = new Box3().setFromObject(scene).getSize(new Vector3())
  expect(size.toArray().every(Number.isFinite)).toBe(true)
  expect(size.x).toBeGreaterThan(0.3)
  expect(size.y).toBeCloseTo(2, 2)
  expect(size.z).toBeGreaterThan(0.1) // The delivered geometry has depth as well as height/width.

  const center = (concept: string) => new Box3()
    .setFromObject(meshes.find(mesh => mesh.userData.concept === concept)!)
    .getCenter(new Vector3())
  expect(center('FMA7310').x).toBeGreaterThan(center('FMA7309').x) // Patient's left is screen right.
  expect(center('FMA7485').z).toBeGreaterThan(center('FMA13478').z) // Sternum in front of spine.
  expect(center('FMA7088').x).toBeGreaterThan(0) // Heart is on the patient's left.
})
