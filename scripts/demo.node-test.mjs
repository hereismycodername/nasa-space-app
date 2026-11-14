import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { once } from 'node:events'
import { copyFile, mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises'
import { createServer } from 'node:http'
import { tmpdir } from 'node:os'
import { delimiter, join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'

const source = fileURLToPath(new URL('../', import.meta.url))
// These subprocess fixtures use executable Node shebangs, like run.node-test.mjs.
const options = { timeout: 15_000, skip: process.platform === 'win32' }

async function fixture(serverSource) {
  const directory = await mkdtemp(join(tmpdir(), 'space-health-scenario-test-'))
  for (const folder of ['scripts', 'simulator', 'demo', 'dist']) await mkdir(join(directory, folder))
  for (const path of ['scripts/demo.mjs', 'scripts/tooling.mjs', 'simulator/history.mjs', 'simulator/profile.mjs', 'simulator/sensors.mjs', 'simulator/config.mjs']) {
    await copyFile(join(source, path), join(directory, path))
  }
  await writeFile(join(directory, 'package.json'), '{"type":"module"}')
  // This private placeholder skips preparation; no user's database is touched.
  await writeFile(join(directory, 'demo/baseline.db'), '')
  const binary = join(directory, 'fixture-server')
  await writeFile(binary, `#!${process.execPath}\n${serverSource}`, { mode: 0o755 })
  const artifact = JSON.stringify({ reason: 'compiler-artifact', target: { name: 'space-health-server' }, executable: binary })
  await writeFile(join(directory, 'cargo'), `#!${process.execPath}\nif(process.argv[2]==='--version') console.log('cargo fixture');\nelse if(process.argv[2]==='build') console.log(${JSON.stringify(artifact)});\nelse process.exit(2);\n`, { mode: 0o755 })
  let child
  let completion
  let output = ''
  return {
    directory,
    get output() { return output },
    get child() { return child },
    start(port) {
      child = spawn(process.execPath, ['scripts/demo.mjs', 'start', '--scenario', 'baseline', '--port', String(port)], {
        cwd: directory, env: { ...process.env, PATH: `${directory}${delimiter}${process.env.PATH}`, SHUTDOWN_STDIN: '0' },
        stdio: ['ignore', 'pipe', 'pipe'],
      })
      completion = once(child, 'close')
      child.stdout.on('data', chunk => { output += chunk })
      child.stderr.on('data', chunk => { output += chunk })
      return completion
    },
    async close() {
      if (child?.exitCode === null) child.kill('SIGTERM')
      if (completion) await completion
      await rm(directory, { recursive: true, force: true })
    },
  }
}

async function listen(server) {
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  return server.address().port
}

async function unusedPort() {
  const server = createServer()
  const port = await listen(server)
  await new Promise(resolve => server.close(resolve))
  return port
}

function answer(request, response) {
  let body = ''
  request.on('data', chunk => { body += chunk })
  request.on('end', () => {
    response.writeHead(200, { 'Content-Type': 'application/json' })
    response.end(request.method === 'POST' ? JSON.stringify({ ...JSON.parse(body), id: 'fixture', source: 'simulator' }) : '{"status":"ok","storage":"sqlite"}')
  })
}

test('scenario launcher refuses an already healthy port without starting its simulator or sending data', options, async () => {
  let writes = 0
  const existing = createServer((request, response) => {
    if (request.method === 'POST') writes++
    answer(request, response)
  })
  const port = await listen(existing)
  const run = await fixture(`import { writeFileSync } from 'node:fs'; writeFileSync('child-started','yes'); setTimeout(()=>process.exit(1),1000);`)
  try {
    const [code] = await run.start(port)
    assert.equal(code, 1, run.output)
    assert.match(run.output, /already in use/)
    assert.doesNotMatch(run.output, /Synthetic sensor|UI:/)
    assert.equal(writes, 0, 'the existing service must receive no simulator writes')
    await assert.rejects(readFile(join(run.directory, 'child-started')), { code: 'ENOENT' })
  } finally {
    await run.close()
    await new Promise(resolve => existing.close(resolve))
  }
})

const serverFixture = `
import { createServer } from 'node:http';
import { appendFileSync } from 'node:fs';
const [host,port]=process.env.BIND_ADDR.split(':');
const server=createServer((request,response)=>{
  let body=''; request.on('data',chunk=>body+=chunk); request.on('end',()=>{
    if(request.method==='POST') appendFileSync('writes','1');
    response.writeHead(200,{'Content-Type':'application/json'});
    response.end(request.method==='POST'?JSON.stringify({...JSON.parse(body),id:'fixture',source:'simulator'}):'{"status":"ok","storage":"sqlite"}');
  });
});
const shutdown=()=>server.close(()=>console.log('Fixture stopped gracefully'));
process.once('SIGINT',shutdown); process.once('SIGTERM',shutdown);
`

test('health alone is not readiness when a listener appears after the port preflight', options, async () => {
  const run = await fixture(`${serverFixture}\nserver.listen(Number(port),host,()=>setTimeout(()=>server.close(()=>process.exit(1)),700));`)
  try {
    const [code] = await run.start(await unusedPort())
    assert.equal(code, 1, run.output)
    assert.doesNotMatch(run.output, /Synthetic sensor|UI:/)
    await assert.rejects(readFile(join(run.directory, 'writes')), { code: 'ENOENT' })
  } finally { await run.close() }
})

test('owned post-bind readiness forwards logs, permits simulation and preserves graceful shutdown', options, async () => {
  const run = await fixture(`${serverFixture}\nserver.listen(Number(port),host,()=>{ process.stdout.write('Fixture starting\\nSpace '); setTimeout(()=>console.log('Health: http://'+process.env.BIND_ADDR),25); });`)
  try {
    const completion = run.start(await unusedPort())
    let writes = ''
    for (let n = 0; n < 500 && !writes; n++) {
      assert.equal(run.child.exitCode, null, run.output)
      writes = await readFile(join(run.directory, 'writes'), 'utf8').catch(() => '')
      if (!writes) await delay(10)
    }
    assert.ok(writes, run.output)
    assert.match(run.output, /Fixture starting/)
    assert.match(run.output, /Space Health: http:/)
    assert.match(run.output, /UI: http:/)
    run.child.kill('SIGTERM')
    const [code] = await completion
    assert.equal(code, 0, run.output)
    assert.match(run.output, /Fixture stopped gracefully/)
    assert.doesNotMatch(run.output, /terminating it/)
  } finally { await run.close() }
})
