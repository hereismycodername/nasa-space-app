#!/usr/bin/env node
/** Build-time reduction only. The resulting GLB needs no meshopt runtime decoder. */
import { createHash } from 'node:crypto'
import { readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const [input, output, simplifierModule] = process.argv.slice(2)
if (!input || !output || !simplifierModule || resolve(input) === resolve(output)) {
  throw new Error('Usage: node scripts/simplify-anatomy-model.mjs input.glb output.glb /path/to/meshopt_simplifier.js')
}
const packageInfo = JSON.parse(await readFile(resolve(dirname(simplifierModule), 'package.json'), 'utf8'))
if (packageInfo.name !== 'meshoptimizer' || packageInfo.version !== '1.3.0') {
  throw new Error('This asset pipeline requires meshoptimizer@1.3.0 for reproducible output')
}
const { MeshoptSimplifier } = await import(pathToFileURL(resolve(simplifierModule)).href)
await MeshoptSimplifier.ready
const source = await readFile(input)
if (source.readUInt32LE(0) !== 0x46546c67 || source.readUInt32LE(4) !== 2) throw new Error('Expected glTF 2 GLB')
const jsonLength = source.readUInt32LE(12)
const gltf = JSON.parse(source.subarray(20, 20 + jsonLength).toString())
const binary = source.subarray(28 + jsonLength)
const provenance = JSON.parse(await readFile(input.replace(/\.glb$/, '.provenance.json'), 'utf8'))
const buffers = []
const views = []
const accessors = []
let byteLength = 0

function readAccessor(index) {
  const accessor = gltf.accessors[index]
  const view = gltf.bufferViews[accessor.bufferView]
  if (view.byteStride || accessor.sparse || view.buffer !== 0) throw new Error('Unexpected generated accessor')
  const components = accessor.type === 'VEC3' ? 3 : 1
  const ArrayType = { 5123: Uint16Array, 5125: Uint32Array, 5126: Float32Array }[accessor.componentType]
  const size = accessor.count * components * ArrayType.BYTES_PER_ELEMENT
  const start = (view.byteOffset ?? 0) + (accessor.byteOffset ?? 0)
  return new ArrayType(binary.subarray(start, start + size).buffer.slice(
    binary.byteOffset + start, binary.byteOffset + start + size))
}

function bounds(positions) {
  const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity]
  for (let index = 0; index < positions.length; index++) {
    const axis = index % 3
    min[axis] = Math.min(min[axis], positions[index])
    max[axis] = Math.max(max[axis], positions[index])
  }
  return { min, max }
}

function addAccessor(data, componentType, type, target, withBounds = false) {
  const padding = (4 - byteLength % 4) % 4
  if (padding) { buffers.push(Buffer.alloc(padding)); byteLength += padding }
  const bytes = Buffer.from(data.buffer, data.byteOffset, data.byteLength)
  views.push({ buffer: 0, byteOffset: byteLength, byteLength: bytes.length, target })
  buffers.push(bytes)
  byteLength += bytes.length
  accessors.push({ bufferView: views.length - 1, componentType,
    count: data.length / (type === 'VEC3' ? 3 : 1), type,
    ...(withBounds ? bounds(data) : {}),
  })
  return accessors.length - 1
}

function weld(positions, indices) {
  const unique = new Map(), points = [], remap = new Uint32Array(positions.length / 3)
  for (let index = 0; index < remap.length; index++) {
    const x = positions[index * 3], y = positions[index * 3 + 1], z = positions[index * 3 + 2]
    const key = `${x},${y},${z}`
    let target = unique.get(key)
    if (target === undefined) {
      target = points.length / 3
      unique.set(key, target)
      points.push(x, y, z)
    }
    remap[index] = target
  }
  const faces = []
  for (let index = 0; index < indices.length; index += 3) {
    const a = remap[indices[index]], b = remap[indices[index + 1]], c = remap[indices[index + 2]]
    if (a !== b && b !== c && c !== a) faces.push(a, b, c)
  }
  return { positions: new Float32Array(points), indices: new Uint32Array(faces) }
}

function normalsFor(positions, indices) {
  const sums = new Float64Array(positions.length)
  for (let index = 0; index < indices.length; index += 3) {
    const a = indices[index] * 3, b = indices[index + 1] * 3, c = indices[index + 2] * 3
    const ax = positions[b] - positions[a], ay = positions[b + 1] - positions[a + 1], az = positions[b + 2] - positions[a + 2]
    const bx = positions[c] - positions[a], by = positions[c + 1] - positions[a + 1], bz = positions[c + 2] - positions[a + 2]
    const x = ay * bz - az * by, y = az * bx - ax * bz, z = ax * by - ay * bx
    for (const vertex of [a, b, c]) { sums[vertex] += x; sums[vertex + 1] += y; sums[vertex + 2] += z }
  }
  const normals = new Float32Array(sums.length)
  for (let index = 0; index < sums.length; index += 3) {
    const length = Math.hypot(sums[index], sums[index + 1], sums[index + 2])
    if (length) {
      normals[index] = sums[index] / length
      normals[index + 1] = sums[index + 1] / length
      normals[index + 2] = sums[index + 2] / length
    } else normals[index + 1] = 1
  }
  return normals
}

const stats = []
for (const node of gltf.nodes) {
  if (node.mesh === undefined) continue
  const mesh = gltf.meshes[node.mesh]
  if (mesh.primitives.length !== 1) throw new Error('Expected one generated primitive per organ')
  const primitive = mesh.primitives[0]
  const originalPositions = readAccessor(primitive.attributes.POSITION)
  const originalIndices = readAccessor(primitive.indices)
  const welded = weld(originalPositions, originalIndices)
  const { system, layer } = node.extras
  const targetTriangles = layer === 'skin' ? 15_000
    : node.name === 'Skull' || node.name === 'Brain' ? 4_000
      : layer === 'skeleton' ? 1_500 : system === 'eyes' ? 500 : system === 'heart' ? 2_500 : 2_000
  const [indices, error] = MeshoptSimplifier.simplify(welded.indices, welded.positions, 3,
    Math.min(welded.indices.length, targetTriangles * 3), 0.01, ['Prune'])
  if (!indices.length) throw new Error(`Simplification removed ${node.name}`)
  const [remap, count] = MeshoptSimplifier.compactMesh(indices)
  const positions = new Float32Array(count * 3)
  for (let index = 0; index < remap.length; index++) {
    if (remap[index] === 0xffffffff) continue
    positions.set(welded.positions.subarray(index * 3, index * 3 + 3), remap[index] * 3)
  }
  const compactIndices = count <= 65536 ? new Uint16Array(indices) : indices
  primitive.attributes.POSITION = addAccessor(positions, 5126, 'VEC3', 34962, true)
  primitive.attributes.NORMAL = addAccessor(normalsFor(positions, indices), 5126, 'VEC3', 34962)
  primitive.indices = addAccessor(compactIndices, count <= 65536 ? 5123 : 5125, 'SCALAR', 34963)
  stats.push({ ...node.extras, name: node.name, sourceVertices: originalPositions.length / 3,
    sourceTriangles: originalIndices.length / 3, vertices: count, triangles: indices.length / 3,
    targetTriangles, relativeError: error })
}

const finalPadding = (4 - byteLength % 4) % 4
buffers.push(Buffer.alloc(finalPadding))
byteLength += finalPadding
gltf.accessors = accessors
gltf.bufferViews = views
gltf.buffers = [{ byteLength }]
gltf.asset.generator += ' + scripts/simplify-anatomy-model.mjs (meshoptimizer 1.3.0)'
const rawJson = Buffer.from(JSON.stringify(gltf))
const json = Buffer.concat([rawJson, Buffer.alloc((4 - rawJson.length % 4) % 4, 0x20)])
const header = Buffer.alloc(20), binHeader = Buffer.alloc(8)
header.writeUInt32LE(0x46546c67, 0); header.writeUInt32LE(2, 4)
header.writeUInt32LE(28 + json.length + byteLength, 8)
header.writeUInt32LE(json.length, 12); header.writeUInt32LE(0x4e4f534a, 16)
binHeader.writeUInt32LE(byteLength, 0); binHeader.writeUInt32LE(0x004e4942, 4)
const result = Buffer.concat([header, json, binHeader, ...buffers])
if (result.length > 3_000_000 || stats.reduce((sum, part) => sum + part.triangles, 0) > 100_000) {
  throw new Error('Model exceeds the 3 MB / 100,000 triangle delivery budget')
}
await writeFile(output, result)
provenance.parts = stats
provenance.sourceMeshNotices = provenance.sourceMeshNotices.filter(line => /licen[cs]e|copyright|bodyparts3d|database|https?:/i.test(line))
provenance.simplification = { tool: 'meshoptimizer', version: '1.3.0', errorLimit: 0.01,
  flags: ['Prune'], vertexWelding: 'exact float32 coordinates', recomputedNormals: true,
  description: 'Build-time quadric-error simplification, compact vertices, uncompressed glTF output.' }
provenance.output = { file: output.split('/').at(-1), bytes: result.length,
  sha256: createHash('sha256').update(result).digest('hex'),
  vertices: stats.reduce((sum, part) => sum + part.vertices, 0),
  triangles: stats.reduce((sum, part) => sum + part.triangles, 0) }
await writeFile(output.replace(/\.glb$/, '.provenance.json'), JSON.stringify(provenance, null, 2) + '\n')
console.log(JSON.stringify(provenance.output))
