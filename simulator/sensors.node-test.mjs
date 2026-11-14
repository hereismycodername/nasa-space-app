import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { once } from 'node:events'
import { createServer } from 'node:http'
import { test } from 'node:test'
import { setTimeout as delay } from 'node:timers/promises'
import { apiOrigin, simulatorConfig } from './config.mjs'

test('API origin accepts /api without duplicating it; malformed configuration is rejected', () => {
  for (const suffix of ['', '/', '/api', '/api/']) assert.equal(apiOrigin(`http://localhost:8080${suffix}`), 'http://localhost:8080')
  for (const value of ['localhost:8080', 'ftp://host', 'http://host/api/measurements', 'http://host?x=1', 'http://user:pass@host']) assert.throws(() => apiOrigin(value))
  for (const env of [{ SIM_COUNT: '-1' }, { SIM_COUNT: '' }, { SIM_INTERVAL_MS: '99' }, { SIM_INTERVAL_MS: '9999999999' }, { SIM_COUNT: '1.5' }]) assert.throws(() => simulatorConfig(env))
})

function simulator(env) {
  const child = spawn(process.execPath, ['simulator/sensors.mjs'], { env: { ...process.env, SIM_REQUEST_TIMEOUT_MS: '500', SIM_RETRY_MS: '100', ...env }, stdio: ['ignore', 'pipe', 'pipe'] })
  let output = ''
  child.stdout.on('data', chunk => { output += chunk })
  child.stderr.on('data', chunk => { output += chunk })
  return { child, output: () => output, done: once(child, 'exit') }
}

test('finite simulator retries exact payload, posts to /api once and exits', { timeout: 8000 }, async () => {
  const received = []
  const server = createServer(async (request, response) => {
    const chunks = []
    for await (const chunk of request) chunks.push(chunk)
    received.push({ path: request.url, body: Buffer.concat(chunks).toString() })
    response.writeHead(received.length === 1 ? 503 : 201, { 'Content-Type': 'application/json' })
    response.end(JSON.stringify({ ...JSON.parse(received.at(-1).body), id: 'saved-id', source: 'simulator' }))
  })
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  const run = simulator({ API_URL: `http://127.0.0.1:${server.address().port}/api/`, SIM_COUNT: '2', SIM_INTERVAL_MS: '100' })
  try {
    assert.equal((await run.done)[0], 0, run.output())
    assert.equal(received.length, 3)
    assert.ok(received.every(row => row.path === '/api/measurements'))
    assert.equal(received[0].body, received[1].body)
    assert.equal(JSON.parse(received[2].body).sequence, 1)
  } finally { run.child.kill('SIGKILL'); server.closeAllConnections(); await new Promise(resolve => server.close(resolve)) }
})

test('SIGTERM interrupts a long retry wait', { timeout: 5000 }, async () => {
  const server = createServer((_request, response) => { response.writeHead(503); response.end() })
  server.listen(0, '127.0.0.1'); await once(server, 'listening')
  const run = simulator({ API_URL: `http://127.0.0.1:${server.address().port}`, SIM_RETRY_MS: '60000', SIM_INTERVAL_MS: '60000' })
  try {
    for (let n = 0; n < 100 && !run.output().includes('retrying'); n++) await delay(10)
    assert.match(run.output(), /retrying/)
    run.child.kill('SIGTERM')
    const result = await Promise.race([run.done, delay(1000, undefined, { ref: false }).then(() => { throw new Error('Stop did not cancel retry wait') })])
    assert.equal(result[0], 0)
  } finally { run.child.kill('SIGKILL'); server.closeAllConnections(); await new Promise(resolve => server.close(resolve)) }
})

test('4xx is actionable and stops instead of retrying forever', { timeout: 5000 }, async () => {
  let calls = 0
  const server = createServer((_request, response) => { calls++; response.writeHead(400); response.end('{"code":"UNKNOWN_DEVICE"}') })
  server.listen(0, '127.0.0.1'); await once(server, 'listening')
  const run = simulator({ API_URL: `http://127.0.0.1:${server.address().port}`, SIM_COUNT: '1' })
  try {
    assert.equal((await run.done)[0], 1)
    assert.equal(calls, 1)
    assert.match(run.output(), /UNKNOWN_DEVICE/)
  } finally { run.child.kill('SIGKILL'); server.closeAllConnections(); await new Promise(resolve => server.close(resolve)) }
})

test('200 without a matching saved measurement is not accepted', { timeout: 5000 }, async () => {
  const server = createServer((_request, response) => { response.writeHead(200); response.end('{}') })
  server.listen(0, '127.0.0.1'); await once(server, 'listening')
  const run = simulator({ API_URL: `http://127.0.0.1:${server.address().port}`, SIM_COUNT: '1' })
  try {
    assert.equal((await run.done)[0], 1)
    assert.match(run.output(), /did not confirm/)
    assert.doesNotMatch(run.output(), /0: 72 bpm/)
  } finally { run.child.kill('SIGKILL'); server.closeAllConnections(); await new Promise(resolve => server.close(resolve)) }
})
