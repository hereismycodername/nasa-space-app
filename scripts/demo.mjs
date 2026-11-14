import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { lstat, mkdir, readFile, readdir, realpath, rm, writeFile } from 'node:fs/promises'
import { createServer } from 'node:net'
import { join, resolve } from 'node:path'
import { parseArgs } from 'node:util'
import { setTimeout as delay } from 'node:timers/promises'
import { generateHistory, HISTORY_METRICS, SCENARIOS } from '../simulator/history.mjs'
import { cargoPath, isWindows, root } from './tooling.mjs'

const demoDirectory = join(root, 'demo')
let binary
const controller = new AbortController()
const children = new Set()
let interrupted = false
for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, () => {
    interrupted = true
    controller.abort(new Error('Demo interrupted'))
  })
}
if (isWindows && process.env.SHUTDOWN_STDIN === '1') {
  process.stdin.on('data', () => process.emit('SIGINT'))
  process.stdin.on('end', () => process.emit('SIGINT'))
}

function launch(command, args, env = {}, capture = false) {
  controller.signal.throwIfAborted()
  const controlled = isWindows && env.SHUTDOWN_STDIN === '1'
  const child = spawn(command, args, {
    cwd: root, env: { ...process.env, ...env, SHUTDOWN_STDIN: controlled ? '1' : '0' },
    stdio: [controlled ? 'pipe' : 'inherit', capture ? 'pipe' : 'inherit', 'inherit'], windowsHide: true,
    detached: !isWindows,
  })
  child.demoShutdown = controlled
  child.stdin?.on('error', () => { /* child may exit before shutdown is written */ })
  children.add(child)
  child.completion = new Promise(resolve => {
    child.once('error', error => resolve({ error }))
    child.once('exit', (code, signal) => resolve({ code, signal }))
  })
  return child
}

async function waitForChild(child) {
  const result = await child.completion
  if (result.error) throw result.error
  if (result.code !== 0) throw new Error(`Process exited: ${result.code ?? result.signal}`)
}

async function stop(child) {
  if (child.exitCode !== null || child.signalCode !== null || !child.pid) return
  // Unix supervisors signal the process group; Windows uses the private pipe.
  if (child.demoShutdown) child.stdin.end('shutdown\n')
  else signal(child, 'SIGINT')
  let timer
  const exited = await Promise.race([
    child.completion.then(() => true),
    new Promise(resolve => { timer = setTimeout(() => resolve(false), 10_000) }),
  ])
  clearTimeout(timer)
  if (!exited) {
    console.warn(`Process ${child.pid} did not stop after shutdown; terminating it.`)
    signal(child, 'SIGKILL')
    await child.completion
  }
}

function signal(child, value) {
  try {
    if (isWindows) child.kill(value)
    else process.kill(-child.pid, value)
  } catch (error) { if (error.code !== 'ESRCH') console.error(error.message) }
}

async function ensureBinary() {
  if (binary) return
  console.log('Building space-health-server...')
  const child = launch(cargoPath(), ['build', '-p', 'space-health-server', '--bin', 'space-health-server', '--locked', '--message-format=json'], {}, true)
  let pending = ''
  child.stdout.on('data', chunk => {
    pending += chunk
    let newline
    while ((newline = pending.indexOf('\n')) >= 0) {
      const line = pending.slice(0, newline)
      pending = pending.slice(newline + 1)
      try {
        const message = JSON.parse(line)
        if (message.reason === 'compiler-artifact' && message.target?.name === 'space-health-server' && message.executable) binary = message.executable
        if (message.reason === 'compiler-message' && message.message?.rendered) process.stderr.write(message.message.rendered)
      } catch { if (line.trim()) console.log(line) }
    }
  })
  await waitForChild(child)
  if (!binary) throw new Error('Cargo build did not report the server executable.')
}

async function ensureDemoDirectory() {
  await mkdir(demoDirectory, { recursive: true })
  // Reject junctions/symlinks: both creation and --force must stay in demo/.
  if ((await lstat(demoDirectory)).isSymbolicLink()
    || resolve(await realpath(demoDirectory)) !== join(await realpath(root), 'demo')) {
    throw new Error('demo/ must be a real directory inside the repository')
  }
}

async function validateDatabaseFiles(scenario) {
  for (const name of await readdir(demoDirectory)) {
    if (!name.startsWith(`${scenario}.db`)) continue
    const stat = await lstat(join(demoDirectory, name))
    if (!stat.isFile() || stat.isSymbolicLink()) throw new Error(`Unsupported demo path: ${name}`)
  }
}

async function freePort() {
  const listener = createServer()
  await new Promise((resolve, reject) => {
    listener.once('error', reject)
    listener.listen(0, '127.0.0.1', resolve)
  })
  const port = listener.address().port
  await new Promise((resolve, reject) => listener.close(error => error ? reject(error) : resolve()))
  return port
}

async function startServer(scenario, port) {
  const base = `http://127.0.0.1:${port}`
  const child = launch(binary, [], {
    DATABASE_URL: `sqlite://demo/${scenario}.db`, BIND_ADDR: `127.0.0.1:${port}`, FRONTEND_DIST: 'dist',
    SHUTDOWN_STDIN: isWindows ? '1' : '0',
  })
  for (let attempt = 0; attempt < 100; attempt++) {
    controller.signal.throwIfAborted()
    if (child.exitCode !== null || child.signalCode !== null || !child.pid) {
      await waitForChild(child)
      throw new Error('Server exited before becoming ready')
    }
    try {
      const response = await fetch(`${base}/api/health`, {
        signal: AbortSignal.any([controller.signal, AbortSignal.timeout(1000)]),
      })
      if (response.ok) return { child, base }
    } catch { /* listener not ready yet */ }
    await delay(100, undefined, { signal: controller.signal })
  }
  throw new Error('Server did not become ready within 10 seconds')
}

async function prepare(scenario, { seed = '42', reference, force = false } = {}) {
  const now = Date.now()
  reference ??= new Date(Math.floor(now / 60_000) * 60_000).toISOString()
  if (Date.parse(reference) > now) {
    throw new Error(`--reference must not be in the future (now: ${new Date(now).toISOString()})`)
  }
  const numericSeed = Number(seed)
  const payloads = generateHistory({ scenario, seed: numericSeed, reference })
  reference = new Date(reference).toISOString()
  await ensureDemoDirectory()
  await validateDatabaseFiles(scenario)
  const database = join(demoDirectory, `${scenario}.db`)
  if (existsSync(database) && !force) {
    throw new Error(`demo/${scenario}.db already exists. Use --force to restore this scenario.`)
  }
  await ensureBinary()
  if (force) {
    for (const name of await readdir(demoDirectory)) {
      if (name.startsWith(`${scenario}.db`)) await rm(join(demoDirectory, name))
    }
  }
  console.log(`Synthetic demo: scenario=${scenario} seed=${numericSeed} reference=${reference}`)
  const { child, base } = await startServer(scenario, await freePort())
  const summary = Object.fromEntries(HISTORY_METRICS
    .map(metric => [metric, { created: 0, repeated: 0, errors: 0 }]))
  try {
    for (const payload of payloads) {
      const counts = summary[payload.metric]
      try {
        const response = await fetch(`${base}/api/measurements`, {
          method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload),
          signal: AbortSignal.any([controller.signal, AbortSignal.timeout(10_000)]),
        })
        const body = await response.text()
        if (response.status === 201) counts.created++
        else if (response.status === 200) counts.repeated++
        else {
          counts.errors++
          throw new Error(`sequence=${payload.sequence} HTTP ${response.status}: ${body}`)
        }
      } catch (error) {
        // Stop at the first failure; report it under the affected metric.
        if (!counts.errors) counts.errors++
        throw error
      }
    }
    await writeFile(`${database}.json`, `${JSON.stringify({ seed: numericSeed, reference }, null, 2)}\n`)
  } finally {
    for (const [metric, counts] of Object.entries(summary)) {
      console.log(`${metric}: 201=${counts.created} 200=${counts.repeated} errors=${counts.errors}`)
    }
    await stop(child)
  }
  console.log(`Prepared demo/${scenario}.db`)
}

async function start(scenario, port) {
  await ensureDemoDirectory()
  await validateDatabaseFiles(scenario)
  if (!existsSync(join(demoDirectory, `${scenario}.db`))) await prepare(scenario)
  await ensureBinary()
  if (!existsSync(join(root, 'dist'))) console.warn('dist/ is missing. Run npm run build for the UI.')
  const { child, base } = await startServer(scenario, port)
  let reference = new Date().toISOString()
  try {
    reference = JSON.parse(await readFile(join(demoDirectory, `${scenario}.db.json`), 'utf8')).reference
  } catch { /* existing demo DB without metadata: show the current seven days */ }
  const from = new Date(Date.parse(reference) - 7 * 86400_000).toISOString()
  console.log(`UI: ${base}/`)
  console.log(`Latest: ${base}/api/measurements/latest`)
  console.log(`Series (7 days): ${base}/api/measurements/series?${new URLSearchParams({ metric: 'heartRate', from, to: reference })}`)
  console.log('Ctrl+C stops the demo server and simulator.')
  const simulator = scenario === 'stale' ? null : launch(process.execPath, [join(root, 'simulator', 'sensors.mjs')], {
    API_URL: base, SIM_COUNT: '0', SIM_INTERVAL_MS: '2000', SHUTDOWN_STDIN: isWindows ? '1' : '0',
    SIM_PROFILE: scenario === 'attention' ? 'attention' : 'normal',
  })
  await Promise.race([
    waitForChild(child).then(() => { throw new Error('Demo server stopped unexpectedly') }),
    ...(simulator ? [waitForChild(simulator).then(() => { throw new Error('Live simulator stopped unexpectedly') })] : []),
    new Promise(resolve => {
      if (controller.signal.aborted) resolve()
      else controller.signal.addEventListener('abort', resolve, { once: true })
    }),
  ])
}

try {
  const [command, ...args] = process.argv.slice(2)
  if (!['prepare', 'start'].includes(command)) throw new Error('Usage: demo.mjs prepare|start --scenario baseline|attention|stale|delayed')
  const { values } = parseArgs({ args, options: {
    scenario: { type: 'string' },
    ...(command === 'prepare'
      ? { seed: { type: 'string' }, reference: { type: 'string' }, force: { type: 'boolean' } }
      : { port: { type: 'string', default: '8080' } }),
  } })
  if (!SCENARIOS.includes(values.scenario)) throw new Error(`--scenario must be one of: ${SCENARIOS.join(', ')}`)
  if (command === 'prepare') await prepare(values.scenario, values)
  else {
    const port = Number(values.port)
    if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error('--port must be an integer from 1 to 65535')
    await start(values.scenario, port)
  }
} catch (error) {
  if (!interrupted) console.error(`Demo failed: ${error.message}`)
  process.exitCode = interrupted ? 0 : 1
} finally {
  // Stop producers before storage, including on failed readiness/build/HTTP.
  for (const child of [...children].reverse()) await stop(child)
  process.stdin.pause()
}
