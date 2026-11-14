import { spawn, spawnSync } from 'node:child_process'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { createConnection } from 'node:net'
import { setTimeout as delay } from 'node:timers/promises'
import { cargoPath, isWindows, npmCommand, requireNode, root } from './tooling.mjs'
import { simulatorConfig } from '../simulator/config.mjs'

const args = new Set(process.argv.slice(2))
if (args.has('--help')) {
  console.log('npm run demo [-- --no-build|--no-simulator]\nSIM_COUNT=N makes the demo finish after N confirmed samples.\nnpm run server: backend only, for Vite development.\nnpm run start: existing server binary + dist, without simulator.')
  process.exit(0)
}
const shutdown = new AbortController()
const stop = () => shutdown.abort()
process.once('SIGINT', stop)
process.once('SIGTERM', stop)
const children = new Set()

function launch(command, arguments_, env = process.env, capture = false) {
  const child = spawn(command, arguments_, { cwd: root, env, stdio: capture ? ['inherit', 'pipe', 'inherit'] : 'inherit', detached: !isWindows })
  children.add(child)
  child.done = new Promise(resolve => {
    child.once('error', error => { console.error(`Cannot launch ${command}: ${error.message}`); resolve({ code: 1, error }) })
    child.once('exit', (code, signal) => resolve({ code, signal }))
  }).then(result => { children.delete(child); return result })
  return child
}
function signal(child, value) {
  if (!child.pid || child.exitCode !== null || child.signalCode) return
  try { if (isWindows) child.kill(value); else process.kill(-child.pid, value) } catch (error) { if (error.code !== 'ESRCH') console.error(error.message) }
}
async function stopChild(child) {
  signal(child, 'SIGINT')
  await Promise.race([child.done, delay(4000, undefined, { ref: false })])
  signal(child, 'SIGKILL')
  await child.done
}
async function command(command_, arguments_, onOutput) {
  if (shutdown.signal.aborted) throw new Error('Stopped')
  const child = launch(command_, arguments_, process.env, Boolean(onOutput))
  if (onOutput) child.stdout.on('data', onOutput)
  const abort = () => { void stopChild(child) }
  shutdown.signal.addEventListener('abort', abort, { once: true })
  const result = await child.done
  shutdown.signal.removeEventListener('abort', abort)
  if (result.code !== 0) throw result.error ?? new Error(`${command_} failed (${result.code ?? result.signal})`)
}
function occupied(host, port) {
  return new Promise(resolve => {
    const socket = createConnection({ host, port })
    const done = result => { socket.destroy(); resolve(result) }
    socket.once('connect', () => done(true))
    socket.once('error', () => done(false))
    socket.setTimeout(500, () => done(false))
  })
}

try {
  for (const arg of args) if (!['--no-build', '--no-simulator', '--server-only'].includes(arg)) throw new Error(`Unknown option: ${arg}`)
  requireNode()
  const serverOnly = args.has('--server-only')
  const bind = process.env.BIND_ADDR ?? '127.0.0.1:8080'
  const bindParts = /^(\[[^\]]+\]|[^:\s/?#@]+):(\d+)$/.exec(bind)
  if (!bindParts || Number(bindParts[2]) < 1 || Number(bindParts[2]) > 65535) throw new Error('BIND_ADDR must be host:port, e.g. 127.0.0.1:8080 or 0.0.0.0:8080')
  const port = String(Number(bindParts[2]))
  const listen = new URL(`http://${bind}`)
  const host = listen.hostname === '0.0.0.0' ? '127.0.0.1' : listen.hostname === '[::]' ? '::1' : listen.hostname.replace(/^\[|\]$/g, '')
  const base = `http://${host.includes(':') ? `[${host}]` : host}:${port}`
  // The demo's producer always targets the server started by this process.
  const simEnv = { ...process.env, API_URL: base }
  if (!serverOnly && !args.has('--no-simulator')) simulatorConfig(simEnv)
  if (await occupied(host, Number(port))) throw new Error(`${bind} is already in use. Stop the existing server or choose another BIND_ADDR; no process was killed.`)
  const cargo = args.has('--no-build') && process.env.SERVER_BINARY ? null : cargoPath()
  let compiledBinary
  if (!args.has('--no-build')) {
    if (!serverOnly) {
      if (!existsSync(join(root, 'node_modules'))) throw new Error('Dependencies are missing. Run npm ci first.')
      const [npm, prefix] = npmCommand()
      await command(npm, [...prefix, 'run', 'build'])
    }
    let pendingOutput = ''
    await command(cargo, ['build', '--locked', '--manifest-path', join(root, 'server/Cargo.toml'), '--bin', 'space-health-server', '--message-format=json'], chunk => {
      pendingOutput += chunk
      let newline
      while ((newline = pendingOutput.indexOf('\n')) >= 0) {
        const line = pendingOutput.slice(0, newline)
        pendingOutput = pendingOutput.slice(newline + 1)
        try {
          const message = JSON.parse(line)
          if (message.reason === 'compiler-artifact' && message.target?.name === 'space-health-server' && message.executable) compiledBinary = message.executable
          if (message.reason === 'compiler-message' && message.message?.rendered) process.stderr.write(message.message.rendered)
        } catch { if (line.trim()) console.log(line) }
      }
    })
    if (!compiledBinary) throw new Error('Cargo build did not report the server executable.')
  }
  if (shutdown.signal.aborted) throw new Error('Stopped')
  let binary
  if (args.has('--no-build') && process.env.SERVER_BINARY) binary = resolve(root, process.env.SERVER_BINARY)
  else {
    const metadata = spawnSync(cargo, ['metadata', '--no-deps', '--format-version', '1', '--manifest-path', join(root, 'server/Cargo.toml')], { cwd: root, encoding: 'utf8', timeout: 15_000 })
    if (metadata.status !== 0) throw new Error(`Cannot locate Cargo build output: ${metadata.stderr || metadata.error?.message}`)
    const target = JSON.parse(metadata.stdout).target_directory
    const marker = join(target, 'space-health-demo-executable.json')
    if (compiledBinary) {
      binary = compiledBinary
      writeFileSync(marker, JSON.stringify({ executable: binary }) + '\n')
    } else if (process.env.CARGO_BUILD_TARGET) {
      const triple = process.env.CARGO_BUILD_TARGET
      if (!/^[a-zA-Z0-9_-]+$/.test(triple)) throw new Error('For a custom JSON build target, specify SERVER_BINARY with --no-build.')
      binary = join(target, triple, 'debug', `space-health-server${isWindows ? '.exe' : ''}`)
    } else if (existsSync(marker)) {
      binary = JSON.parse(readFileSync(marker, 'utf8')).executable
    } else binary = join(target, 'debug', `space-health-server${isWindows ? '.exe' : ''}`)
  }
  if (!existsSync(binary)) throw new Error('Server binary missing. Run npm run demo without --no-build first.')
  const dist = process.env.FRONTEND_DIST ?? join(root, 'dist')
  if (!serverOnly && !existsSync(join(dist, 'index.html'))) throw new Error(`Built UI missing in ${dist}. Run npm run build or set FRONTEND_DIST.`)
  const server = launch(binary, [], { ...process.env, BIND_ADDR: bind, FRONTEND_DIST: dist })
  let ready = false
  for (let attempt = 0; attempt < 100 && !shutdown.signal.aborted; attempt++) {
    if (!children.has(server)) throw new Error('Server exited before readiness; see its error above.')
    try { ready = (await fetch(`${base}/api/health`, { signal: AbortSignal.any([shutdown.signal, AbortSignal.timeout(500)]) })).ok } catch { /* wait for startup */ }
    if (ready) break
    await delay(100, undefined, { signal: shutdown.signal }).catch(() => {})
  }
  if (!ready) throw new Error('Server did not become ready. Check DATABASE_URL, write permissions and the error above.')
  console.log(`Ready: ${base}${serverOnly ? '/api/health (run npm run dev separately)' : ''}; Ctrl+C stops all demo processes.`)
  if (bind.startsWith('0.0.0.0:') || bind.startsWith('[::]:')) console.log(`LAN clients: http://<this computer's LAN IP>:${port} (trusted network only).`)
  const simulator = !serverOnly && !args.has('--no-simulator') ? launch(process.execPath, [join(root, 'simulator/sensors.mjs')], simEnv) : null
  const stopped = new Promise(resolve => { if (shutdown.signal.aborted) resolve({ code: 0 }); else shutdown.signal.addEventListener('abort', () => resolve({ code: 0 }), { once: true }) })
  const result = await Promise.race([server.done, stopped, ...(simulator ? [simulator.done] : [])])
  if (result.code !== 0 && !shutdown.signal.aborted) throw result.error ?? new Error(`Demo process exited (${result.code ?? result.signal})`)
} catch (error) {
  if (!shutdown.signal.aborted) { console.error(`Demo: ${error.message}\nRun npm run doctor for environment diagnostics.`); process.exitCode = 1 }
} finally {
  await Promise.all([...children].map(stopChild))
  process.off('SIGINT', stop)
  process.off('SIGTERM', stop)
}
