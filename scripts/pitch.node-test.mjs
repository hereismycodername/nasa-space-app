import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { once } from 'node:events'
import { copyFile, mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'

const source = fileURLToPath(new URL('../', import.meta.url))
const options = { timeout: 10_000 }

// Test orchestration with ordinary Node subprocesses on every supported OS.
// No Cargo build, working database, listener or user-owned service is needed.
async function fixture(mode = 'success') {
  const directory = await mkdtemp(join(tmpdir(), 'space-health-pitch-launcher-test-'))
  for (const name of ['scripts', 'simulator']) await mkdir(join(directory, name))
  for (const name of ['scripts/pitch.mjs', 'scripts/tooling.mjs', 'simulator/pitch-scenario.mjs', 'simulator/config.mjs']) {
    await copyFile(join(source, name), join(directory, name))
  }
  await writeFile(join(directory, 'package.json'), '{"type":"module"}')
  const working = join(directory, 'working.db')
  await writeFile(working, 'keep the existing working database')
  await writeFile(join(directory, 'simulator/pitch.mjs'), `
import {writeFile} from 'node:fs/promises';
import {setTimeout as delay} from 'node:timers/promises';
export async function runPitch(config,{signal}) {
  await writeFile('producer.json',JSON.stringify(config));
  if(process.env.PITCH_FIXTURE_MODE==='exit-during-production') {
    await new Promise(resolve=>{if(signal.aborted) resolve();else signal.addEventListener('abort',resolve,{once:true})});
    await writeFile('producer-aborted','yes');
  } else await delay(20);
}
`)
  await writeFile(join(directory, 'scripts/run.mjs'), `
import {writeFileSync} from 'node:fs';
const base='http://'+process.env.BIND_ADDR;
writeFileSync('backend.json',JSON.stringify({database:process.env.DATABASE_URL,args:process.argv.slice(2),stdin:process.env.SHUTDOWN_STDIN}));
writeFileSync(process.env.DATABASE_URL.slice('sqlite://'.length),'temporary pitch data');
if(process.env.PITCH_FIXTURE_MODE==='unowned-ready') {
  console.log('Ready: '+base+'; another health endpoint is available');
  setTimeout(()=>process.exit(1),180);
} else {
  const keepAlive=setInterval(()=>{},1000);
  const stop=()=>{clearInterval(keepAlive);writeFileSync('backend-stopped','yes');process.stdin.pause();};
  process.once('SIGINT',stop);process.once('SIGTERM',stop);
  if(process.env.SHUTDOWN_STDIN==='1') { process.stdin.on('data',stop);process.stdin.on('end',stop); }
  process.stdout.write('Space ');
  setTimeout(()=>{console.log('Health: '+base);console.log('Ready: '+base+'; owned listener ready');},25);
  if(process.env.PITCH_FIXTURE_MODE==='exit-during-production') setTimeout(()=>process.exit(7),220);
}
`)
  let child
  let completion
  let output = ''
  return {
    directory,
    get output() { return output },
    start(args = []) {
      child = spawn(process.execPath, ['scripts/pitch.mjs', ...args], {
        cwd: directory,
        env: { ...process.env, DATABASE_URL: `sqlite://${working}`, API_URL: 'http://not-the-pitch-server.invalid', PITCH_FIXTURE_MODE: mode },
        stdio: ['ignore', 'pipe', 'pipe'],
      })
      completion = once(child, 'close')
      child.stdout.on('data', chunk => { output += chunk })
      child.stderr.on('data', chunk => { output += chunk })
      return completion
    },
    async close() {
      if (child?.exitCode === null) child.kill('SIGTERM')
      if (completion) await Promise.race([completion, delay(1000)])
      if (child?.exitCode === null) { child.kill('SIGKILL'); await completion }
      await rm(directory, { recursive: true, force: true })
    },
  }
}

test('pitch waits for its own post-bind output, not a Ready line from another health response', options, async () => {
  const run = await fixture('unowned-ready')
  try {
    const [code] = await run.start(['--no-build', '--cycles', '1'])
    assert.equal(code, 1, run.output)
    await assert.rejects(readFile(join(run.directory, 'producer.json')), { code: 'ENOENT' })
    const backend = JSON.parse(await readFile(join(run.directory, 'backend.json'), 'utf8'))
    await assert.rejects(readFile(backend.database.slice('sqlite://'.length)), { code: 'ENOENT' })
  } finally { await run.close() }
})

test('pitch owns an isolated database, forwards scene options and cleans up only its backend', options, async () => {
  const run = await fixture()
  try {
    const [code] = await run.start(['--no-build', '--manual', '--port', '18082', '--stage-seconds', '45', '--interval-ms', '500'])
    assert.equal(code, 0, run.output)
    const backend = JSON.parse(await readFile(join(run.directory, 'backend.json'), 'utf8'))
    assert.deepEqual(backend.args, ['--no-simulator', '--no-build'])
    assert.equal(backend.stdin, process.platform === 'win32' ? '1' : '0')
    assert.equal(await readFile(join(run.directory, 'backend-stopped'), 'utf8'), 'yes')
    assert.notEqual(backend.database, `sqlite://${join(run.directory, 'working.db')}`)
    await assert.rejects(readFile(backend.database.slice('sqlite://'.length)), { code: 'ENOENT' })
    await assert.rejects(readFile(dirname(backend.database.slice('sqlite://'.length))), { code: 'ENOENT' })
    assert.equal(await readFile(join(run.directory, 'working.db'), 'utf8'), 'keep the existing working database')
    const producer = JSON.parse(await readFile(join(run.directory, 'producer.json'), 'utf8'))
    assert.equal(producer.base, 'http://127.0.0.1:18082')
    assert.equal(producer.cycles, 0, 'launcher repeats by default')
    assert.equal(producer.stageSeconds, 45)
    assert.equal(producer.interval, 500)
    assert.equal(producer.manual, true)
  } finally { await run.close() }
})

test('pitch stops production and reports an owned backend failure', options, async () => {
  const run = await fixture('exit-during-production')
  try {
    const [code] = await run.start(['--no-build'])
    assert.equal(code, 1, run.output)
    assert.match(run.output, /завершился \(7\)/)
    assert.equal(await readFile(join(run.directory, 'producer-aborted'), 'utf8'), 'yes')
    const backend = JSON.parse(await readFile(join(run.directory, 'backend.json'), 'utf8'))
    await assert.rejects(readFile(backend.database.slice('sqlite://'.length)), { code: 'ENOENT' })
  } finally { await run.close() }
})

test('pitch rejects invalid options before starting a backend and attaches only when explicitly requested', options, async () => {
  for (const args of [['--port', '0'], ['--unknown'], ['--stage-seconds', 'no'], ['--api-url', 'http://localhost:8080', '--port', '8082']]) {
    const run = await fixture()
    try {
      assert.equal((await run.start(args))[0], 1, run.output)
      await assert.rejects(readFile(join(run.directory, 'backend.json')), { code: 'ENOENT' })
      await assert.rejects(readFile(join(run.directory, 'producer.json')), { code: 'ENOENT' })
    } finally { await run.close() }
  }
  const run = await fixture()
  try {
    assert.equal((await run.start(['--api-url', 'http://localhost:8080/api', '--cycles', '1']))[0], 0, run.output)
    await assert.rejects(readFile(join(run.directory, 'backend.json')), { code: 'ENOENT' })
    const producer = JSON.parse(await readFile(join(run.directory, 'producer.json'), 'utf8'))
    assert.equal(producer.base, 'http://localhost:8080')
    assert.equal(producer.cycles, 1)
  } finally { await run.close() }
})
