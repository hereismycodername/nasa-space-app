import { spawn } from 'node:child_process'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { parseArgs } from 'node:util'
import { runPitch } from '../simulator/pitch.mjs'
import { pitchConfig } from '../simulator/pitch-scenario.mjs'
import { isWindows, requireNode, root } from './tooling.mjs'

const help = `Живая демонстрация AstroCheck — только синтетические показания.

npm run pitch
  Собирает React + Rust и открывает отдельный API на http://127.0.0.1:8082.
  Откройте этот адрес в браузере. GUI/TUI: --api-url http://127.0.0.1:8082.
  Временная SQLite удаляется при выходе; рабочая база не меняется.

Пять сцен по 30 секунд: спокойные показания → внимание → срочный сигнал
→ восстановление → прекращение передачи. По умолчанию повторяются до выхода.
Клавиши: 1–5 сцена, n следующая, r сначала, пробел/p пауза переходов, q выход.
В режиме паузы показания продолжают поступать; сцена 5 прекращает передачу.

  --manual              Переключать сцены вручную
  --port 8082           Порт отдельного стенда
  --no-build            Использовать существующие Rust binary и dist
  --stage-seconds 30    Длительность сцены (stale — не менее 30 секунд)
  --interval-ms 2000    Период синтетических показаний
  --cycles 1            Завершить стенд после одного цикла (0 — повторять)
  --api-url URL         Отправлять в существующий API, без запуска и сборки
                        Показания будут записаны в его базу с source=simulator
  --help                Эта справка

Пульс, давления, температура, SpO₂ и глюкоза имитируют приборы. Сон,
самочувствие, активность и симптомы — ускоренные синтетические самоотчёты.
Новые B/C-показатели пока не имеют правил оценки: «не оценено» — ожидаемо.
`

const shutdown = new AbortController()
const stop = () => shutdown.abort()
let backend
let backendFailure
let directory
let finishing = false

function startBackend(base, port, noBuild) {
  const child = spawn(process.execPath, [join(root, 'scripts/run.mjs'), '--no-simulator', ...(noBuild ? ['--no-build'] : [])], {
    cwd: root,
    env: {
      ...process.env,
      BIND_ADDR: `127.0.0.1:${port}`,
      DATABASE_URL: `sqlite://${join(directory, 'pitch.db')}`,
      FRONTEND_DIST: join(root, 'dist'),
      // Keep the presenter's scene controls readable; explicit diagnostics still win.
      REQUEST_LOG: process.env.REQUEST_LOG ?? '0',
      SHUTDOWN_STDIN: isWindows ? '1' : '0',
    },
    stdio: [isWindows ? 'pipe' : 'ignore', 'pipe', 'inherit'],
  })
  const state = { child, bound: false, ready: false, done: undefined }
  let pending = ''
  child.stdout.setEncoding('utf8')
  child.stdout.on('data', chunk => {
    process.stdout.write(chunk)
    pending += chunk
    let newline
    while ((newline = pending.indexOf('\n')) >= 0) {
      const line = pending.slice(0, newline).trim()
      pending = pending.slice(newline + 1)
      // Never start a producer against a different service answering /health.
      if (line === `Space Health: ${base}`) state.bound = true
      if (line.startsWith(`Ready: ${base};`)) state.ready = true
    }
    pending = pending.slice(-8192)
  })
  child.stdin?.on('error', () => {})
  state.done = new Promise(resolve => {
    child.once('error', error => resolve({ error }))
    child.once('exit', (code, signal) => resolve({ code, signal }))
  }).then(result => {
    if (!finishing && !shutdown.signal.aborted) {
      backendFailure = result.error ?? new Error(`Сервер питча завершился (${result.code ?? result.signal}); см. сообщение выше.`)
      shutdown.abort()
    }
    return result
  })
  return state
}

async function stopBackend() {
  if (!backend) return
  const { child, done } = backend
  if (!child.pid || child.exitCode !== null || child.signalCode) return
  if (isWindows && child.stdin.writable) child.stdin.end('shutdown\n')
  else child.kill('SIGTERM')
  // run.mjs gives its own server/build children up to 10 seconds to stop first.
  const exited = await Promise.race([done.then(() => true), delay(15_000, false, { ref: false })])
  if (!exited) {
    console.error('Лаунчер не завершился штатно; принудительное завершение.')
    child.kill('SIGKILL')
  }
  await done
}

try {
  const { values } = parseArgs({ options: {
    help: { type: 'boolean' }, manual: { type: 'boolean' }, 'no-build': { type: 'boolean' },
    port: { type: 'string' }, 'api-url': { type: 'string' }, cycles: { type: 'string' },
    'stage-seconds': { type: 'string' }, 'interval-ms': { type: 'string' },
  } })
  if (values.help) console.log(help)
  else {
    requireNode()
    const port = values.port ?? '8082'
    if (!/^\d+$/.test(port) || Number(port) < 1 || Number(port) > 65535) throw new Error('--port: целое число от 1 до 65535')
    if (values['api-url'] && values.port) throw new Error('Выберите --api-url для существующего API или --port для отдельного стенда.')
    const forwarded = ['--api-url', values['api-url'] ?? `http://127.0.0.1:${Number(port)}`, '--cycles', values.cycles ?? '0']
    for (const option of ['stage-seconds', 'interval-ms']) if (values[option] !== undefined) forwarded.push(`--${option}`, values[option])
    if (values.manual) forwarded.push('--manual')
    const config = pitchConfig(forwarded, process.env)
    process.once('SIGINT', stop)
    process.once('SIGTERM', stop)
    if (values['api-url']) console.log(`Питч → существующий API ${config.base}; синтетические записи попадут в его базу.`)
    else {
      directory = await mkdtemp(join(tmpdir(), 'space-health-pitch-'))
      console.log(`Изолированный питч: ${config.base}\nВременная база: ${join(directory, 'pitch.db')}`)
      backend = startBackend(config.base, Number(port), values['no-build'])
      const deadline = performance.now() + 20 * 60_000
      while (!shutdown.signal.aborted && !(backend.bound && backend.ready)) {
        if (performance.now() > deadline) throw new Error('Сборка или запуск не завершились за 20 минут; проверьте окружение через npm run doctor.')
        await delay(100, undefined, { signal: shutdown.signal }).catch(error => { if (!shutdown.signal.aborted) throw error })
      }
    }
    if (!shutdown.signal.aborted) {
      console.log(`\nОткройте ${config.base} — браузер, GUI и TUI читают эти же записи.\n`)
      await runPitch(config, { signal: shutdown.signal })
    }
    if (backendFailure) throw backendFailure
  }
} catch (error) {
  if (!shutdown.signal.aborted || backendFailure) {
    console.error(`Питч: ${error.message}`)
    process.exitCode = 1
  }
} finally {
  finishing = true
  shutdown.abort()
  await stopBackend()
  if (directory) await rm(directory, { recursive: true, force: true })
  process.off('SIGINT', stop)
  process.off('SIGTERM', stop)
}
