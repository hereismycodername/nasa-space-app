import { randomUUID } from 'node:crypto'
import { pathToFileURL } from 'node:url'
import { createInterface, emitKeypressEvents } from 'node:readline'
import { setTimeout as delay } from 'node:timers/promises'
import { PITCH_METRICS, PITCH_STAGES, pitchConfig, stageDuration, stageReadings } from './pitch-scenario.mjs'

const fatal = message => new Error(message, { cause: 'fatal' })
const cancelled = error => error?.name === 'AbortError'

async function requestJson(url, options, hooks) {
  const { signal, fetch, wait, log, retry, timeout } = hooks
  while (!signal.aborted) {
    try {
      const response = await fetch(url, { ...options, signal: AbortSignal.any([signal, AbortSignal.timeout(timeout)]) })
      if (!response.ok) {
        if (response.status < 500 && response.status !== 429) throw fatal(`API отклонил запрос: HTTP ${response.status}: ${(await response.text()).slice(0, 1000)}`)
        await response.body?.cancel()
        throw new Error(`HTTP ${response.status}`)
      }
      if (!options.method && response.status !== 200 || options.method && ![200, 201].includes(response.status)) throw fatal(`Неожиданный HTTP ${response.status}; ответ не подтверждён`)
      try { return await response.json() } catch (error) {
        if (error instanceof SyntaxError) throw fatal('API вернул некорректный JSON')
        throw error
      }
    } catch (error) {
      if (signal.aborted) return
      if (error.cause === 'fatal') throw error
      log(`[pitch] retry: ${error.message}. Повтор того же запроса через ${retry} мс.`)
      await wait(retry, signal)
    }
  }
}

/** Only a byte-for-byte equivalent payload is retried; timestamps never advance during retry. */
export async function deliverMeasurement(payload, hooks) {
  const saved = await requestJson(`${hooks.base}/api/measurements`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload),
  }, hooks)
  if (hooks.signal.aborted) return false
  if (!saved || typeof saved.id !== 'string' || !saved.id || saved.source !== 'simulator'
    || Object.keys(payload).some(key => saved[key] !== payload[key])) throw fatal('API не подтвердил именно это синтетическое измерение. Проверьте API и регистрацию simulator-01.')
  return true
}

/** Shared run loop with injectable transport/clock for bounded integration tests. */
export async function runPitch(config, hooks = {}) {
  const stop = new AbortController()
  const signal = hooks.signal ? AbortSignal.any([stop.signal, hooks.signal]) : stop.signal
  const log = hooks.log ?? console.log
  const now = hooks.now ?? (() => performance.now())
  const wallNow = hooks.wallNow ?? (() => new Date().toISOString())
  const wait = hooks.wait ?? ((ms, waitingSignal) => delay(ms, undefined, { signal: waitingSignal }))
  const transport = { base: config.base, retry: config.retry, timeout: config.timeout, signal, log, wait, fetch: hooks.fetch ?? fetch }
  let sequence = 0
  let index = 0
  let completedCycles = 0
  let paused = config.manual
  let startedAt
  let autoElapsed = 0
  let lastTick
  let entering = true
  let plateauPublished = false
  let lastValues
  let previousValues
  const sessionId = `pitch-${randomUUID()}`
  const commands = []
  const subscribe = hooks.subscribeControls === false ? undefined : hooks.subscribeControls ?? controls
  const unsubscribe = subscribe?.(command => {
    const normalized = command.trim().toLowerCase()
    if (normalized === 'q' || normalized === 'quit' || normalized === 'shutdown') stop.abort()
    else commands.push(command === ' ' ? 'pause' : normalized)
  })
  const enter = next => {
    previousValues = lastValues
    index = next
    entering = true
    plateauPublished = false
    startedAt = now()
    autoElapsed = 0
    lastTick = startedAt
  }
  try {
    log(`[pitch] СИНТЕТИЧЕСКИЕ ДАННЫЕ → ${config.base}; session=${sessionId}`)
    log('[pitch] Это демонстрация: сон, настроение, симптомы и активность — ускоренные самоотчёты, не реальные датчики. Восстановление — смена сценария, не лечение.')
    log('[pitch] Управление: 1…5 — сцена; n — следующая; r — заново; пробел/p — пауза переходов; q/Ctrl+C — выход. В pipe команды вводятся отдельными строками.')
    const latest = await requestJson(`${config.base}/api/measurements/latest`, {}, transport)
    if (signal.aborted) return { confirmed: sequence, completedCycles }
    if (!Array.isArray(latest?.metrics) || new Set(latest.metrics.map(row => row.metric)).size !== latest.metrics.length
      || !PITCH_METRICS.every(({ metric }) => latest.metrics.some(row => row.metric === metric))) throw fatal('Для питча нужен сервер B/C с 12 метриками. Обновите сервер; показания ещё не отправлены.')
    enter(0)
    while (!signal.aborted) {
      const tick = now()
      if (!paused) autoElapsed += Math.max(0, tick - lastTick)
      lastTick = tick
      while (commands.length) {
        const command = commands.shift()
        if (/^[1-5]$/.test(command)) enter(Number(command) - 1)
        else if (command === 'n' || command === 'next') enter((index + 1) % PITCH_STAGES.length)
        else if (command === 'r' || command === 'restart') { completedCycles = 0; enter(0) }
        else if (['p', 'pause', 'resume'].includes(command)) {
          paused = command === 'resume' ? false : !paused
          log(`[pitch] Переходы ${paused ? 'приостановлены; поток текущей сцены продолжается' : 'возобновлены'}.`)
        } else log(`[pitch] Неизвестная команда «${command}»: 1…5, n, r, p, q.`)
      }
      let stage = PITCH_STAGES[index]
      if (!entering && !paused && (stage.id === 'stale' || plateauPublished)
        && autoElapsed >= stageDuration(stage.id, config.stageSeconds) * 1000) {
        if (index + 1 === PITCH_STAGES.length) {
          completedCycles++
          if (config.cycles !== 0 && completedCycles >= config.cycles) break
        }
        enter((index + 1) % PITCH_STAGES.length)
        stage = PITCH_STAGES[index]
      }
      if (entering) {
        log(`[pitch] stage=${stage.id} ${index + 1}/5 · ${stage.label}`)
        log(`[pitch] ${stage.hint}${stage.id === 'stale' ? ` Пауза потока: не менее ${stageDuration(stage.id, config.stageSeconds)} с при автоматическом показе.` : ''}`)
      }
      const elapsed = Math.max(0, now() - startedAt) / 1000
      const transitionSeconds = Math.min(8, config.stageSeconds / 3)
      const frame = stageReadings(stage.id, elapsed, previousValues ?? stage.id, transitionSeconds)
      const readings = frame.filter(row => entering || row.sensor)
      const measuredAt = wallNow()
      for (const { metric, unit, value } of readings) {
        if (signal.aborted) break
        if (await deliverMeasurement({ deviceId: 'simulator-01', sessionId, sequence, metric, value, unit, measuredAt }, transport)) sequence++
      }
      if (signal.aborted) break
      if (frame.length) {
        lastValues = Object.fromEntries(frame.map(row => [row.metric, row.value]))
        plateauPublished ||= elapsed >= transitionSeconds
        log(`[pitch] ${stage.id}: пульс ${lastValues.heartRate}; АД ${lastValues.systolicPressure}/${lastValues.diastolicPressure}; t ${lastValues.temperature}; SpO₂ ${lastValues.spo2}; подтверждено ${sequence}`)
      }
      entering = false
      // Waiting for confirmation must not consume the entire scene and skip its plateau.
      // Real measurement timestamps still come from the wall clock, including retries.
      lastTick = now()
      await wait(config.interval, signal)
    }
  } catch (error) {
    if (!signal.aborted || !cancelled(error)) throw error
  } finally {
    unsubscribe?.()
    log(`[pitch] Завершено. Подтверждено ${sequence} измерений; циклов ${completedCycles}. Неподтверждённый пакет сохранялся только в памяти.`)
  }
  return { confirmed: sequence, completedCycles }
}

export const PITCH_HELP = `Синтетический поток для питча (сервер должен быть запущен).
node simulator/pitch.mjs --api-url http://127.0.0.1:8082
  --stage-seconds N  длительность сцены (30), не менее двух интервалов; stale ≥30 с
  --interval-ms N    интервал пакетов (2000; 100…60000)
  --cycles N         число циклов (1); 0 — повторять до остановки
  --manual           ручное переключение сцен с самого начала
Сцены: 1 baseline, 2 attention, 3 critical, 4 recovery, 5 stale.
Клавиши: 1…5 — сцена, n — следующая, r — начать заново,
пробел или p — пауза переходов (поток продолжается), q/Ctrl+C — выход.
При передаче stdin через pipe: одна команда на строку; shutdown завершает процесс.
Новые B/C-показатели пока не имеют правил; общий зелёный статус не обещается.`

function controls(callback) {
  if (process.stdin.isTTY) {
    emitKeypressEvents(process.stdin)
    const wasRaw = process.stdin.isRaw
    process.stdin.setRawMode(true)
    process.stdin.resume()
    const keypress = (text, key) => callback(key?.ctrl && key.name === 'c' ? 'q' : text ?? '')
    process.stdin.on('keypress', keypress)
    return () => { process.stdin.off('keypress', keypress); process.stdin.setRawMode(wasRaw ?? false); process.stdin.pause() }
  }
  const input = createInterface({ input: process.stdin, terminal: false })
  input.on('line', callback)
  const onClose = () => { if (process.env.SHUTDOWN_STDIN === '1') callback('shutdown') }
  input.once('close', onClose)
  return () => { input.off('close', onClose); input.close(); process.stdin.pause() }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const shutdown = new AbortController()
  const stop = () => shutdown.abort()
  process.once('SIGINT', stop)
  process.once('SIGTERM', stop)
  try {
    const config = pitchConfig(process.argv.slice(2))
    if (config.help) console.log(PITCH_HELP)
    else await runPitch(config, { signal: shutdown.signal, subscribeControls: controls })
  } catch (error) {
    if (!shutdown.signal.aborted) { console.error(`[pitch] ${error.message}`); process.exitCode = 1 }
  } finally {
    process.off('SIGINT', stop)
    process.off('SIGTERM', stop)
    process.stdin.pause()
  }
}
