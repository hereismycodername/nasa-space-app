import { accessSync, constants, existsSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
export const isWindows = process.platform === 'win32'

export function executable(name, extra = []) {
  const names = isWindows ? [`${name}.exe`, `${name}.cmd`, name] : [name]
  const directories = [...(process.env.PATH ?? '').split(isWindows ? ';' : ':'), ...extra]
  for (const directory of directories) {
    for (const candidate of names) {
      const path = join(directory, candidate)
      try { accessSync(path, isWindows ? constants.F_OK : constants.X_OK); return path } catch { /* next path */ }
    }
  }
  return null
}

export function cargoPath() {
  const cargo = executable('cargo', [join(process.env.CARGO_HOME ?? join(homedir(), '.cargo'), 'bin')])
  if (!cargo) throw new Error('Cargo not found. Install Rust with rustup, reopen the terminal, then run npm run doctor. ~/.cargo/bin is also checked automatically.')
  const probe = spawnSync(cargo, ['--version'], { encoding: 'utf8', timeout: 10_000 })
  if (probe.status !== 0) throw new Error(`Cargo exists but cannot run: ${probe.stderr || probe.error?.message || 'configure a stable Rust toolchain with rustup'}`)
  return cargo
}

export function requireNode() {
  const [major, minor, patch] = process.versions.node.split('.').map(Number)
  if (major !== 24 || minor < 13 || (minor === 13 && patch < 1)) {
    throw new Error(`Node ${process.versions.node} is installed; this project requires Node 24.13.1 or newer 24.x (.nvmrc).`)
  }
}

export function npmCommand() {
  if (process.env.npm_execpath && existsSync(process.env.npm_execpath)) return [process.execPath, [process.env.npm_execpath]]
  const npm = executable('npm')
  if (!npm || isWindows) throw new Error('Run this command through npm: npm run demo')
  return [npm, []]
}
