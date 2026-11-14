import { readFileSync, existsSync } from 'node:fs'
import { arch, freemem, platform, totalmem } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { cargoPath, executable, requireNode, root } from './tooling.mjs'
import { simulatorConfig } from '../simulator/config.mjs'

const checks = []
function check(name, fn) {
  try { const detail = fn(); checks.push({ name, ok: true, detail }) }
  catch (error) { checks.push({ name, ok: false, detail: error.message }) }
}
check('Node', () => { requireNode(); return `${process.version} (${process.execPath})` })
check('Cargo', cargoPath)
check('C compiler', () => {
  const cc = executable('cc') ?? executable('gcc') ?? executable('clang') ?? executable('cl')
  if (!cc) throw new Error('Compiler not found. Pi/Debian: install build-essential; macOS: Xcode Command Line Tools; Windows: MSVC Build Tools.')
  return cc
})
check('npm dependencies', () => { if (!existsSync(join(root, 'node_modules'))) throw new Error('Run npm ci'); return 'node_modules exists; npm ci restores the locked versions' })
check('Simulator configuration', () => JSON.stringify(simulatorConfig()))
check('Python bridge', () => {
  const python = executable('python3') ?? executable('python')
  if (!python) throw new Error('Python 3.10+ is needed only for the optional USB/stdin bridge')
  const result = spawnSync(python, ['--version'], { encoding: 'utf8', timeout: 3000 })
  if (result.status !== 0) throw new Error('Python could not run')
  return result.stdout.trim()
})
const report = { platform: platform(), architecture: arch(), memoryMiB: Math.round(totalmem() / 1048576), freeMiB: Math.round(freemem() / 1048576), checks }
if (process.platform === 'linux') {
  try { report.board = readFileSync('/proc/device-tree/model', 'utf8').replaceAll('\0', '').trim() } catch { /* not a Pi */ }
  try { report.os = readFileSync('/etc/os-release', 'utf8').split('\n').find(line => line.startsWith('PRETTY_NAME=')) } catch { /* optional */ }
}
console.log(JSON.stringify(report, null, 2))
console.log('Pi deployment targets Raspberry Pi OS 64-bit (arm64). Pico/ESP/Arduino are measurement producers, not this server. See docs/hardware.md.')
if (checks.some(result => !result.ok && result.name !== 'Python bridge')) process.exitCode = 1
