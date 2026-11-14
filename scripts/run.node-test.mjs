import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { once } from 'node:events'
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { delimiter, join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { test } from 'node:test'

test('launcher uses Cargo executable artifact from a configured target and reuses it', { timeout: 10_000, skip: process.platform === 'win32' }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'space-health-launcher-test-'))
  const target = join(directory, 'target')
  const binary = join(target, 'custom-triple', 'debug', 'space-health-server')
  await mkdir(join(target, 'custom-triple', 'debug'), { recursive: true })
  await writeFile(binary, `#!${process.execPath}\nimport { createServer } from 'node:http';\nconst [host,port]=process.env.BIND_ADDR.split(':');\nconst server=createServer((_q,r)=>{r.writeHead(200,{'Content-Type':'application/json'});r.end('{"status":"ok"}')});\nserver.listen(Number(port),host);\nprocess.on('SIGINT',()=>server.close());\n`, { mode: 0o755 })
  const cargo = join(directory, 'cargo')
  await writeFile(cargo, `#!${process.execPath}\nconst command=process.argv[2];\nif(command==='--version') console.log('cargo test-stub');\nelse if(command==='metadata') console.log(${JSON.stringify(JSON.stringify({ target_directory: target }))});\nelse if(command==='build') console.log(${JSON.stringify(JSON.stringify({ reason: 'compiler-artifact', target: { name: 'space-health-server' }, executable: binary }))});\nelse process.exit(2);\n`, { mode: 0o755 })
  let child
  try {
    for (const args of [[], ['--no-build']]) {
      const reservation = createServer()
      reservation.listen(0, '127.0.0.1'); await once(reservation, 'listening')
      const port = reservation.address().port
      await new Promise(resolve => reservation.close(resolve))
      child = spawn(process.execPath, ['scripts/run.mjs', '--server-only', ...args], {
        env: { ...process.env, PATH: `${directory}${delimiter}${process.env.PATH}`, CARGO_BUILD_TARGET: '', SERVER_BINARY: '', BIND_ADDR: `127.0.0.1:${port}` }, stdio: ['ignore', 'pipe', 'pipe'],
      })
      const done = once(child, 'exit')
      let output = ''
      child.stdout.on('data', chunk => { output += chunk })
      child.stderr.on('data', chunk => { output += chunk })
      for (let n = 0; n < 200 && !output.includes('Ready:'); n++) {
        assert.equal(child.exitCode, null, output)
        await delay(10)
      }
      assert.match(output, /Ready:/)
      child.kill('SIGTERM')
      assert.equal((await done)[0], 0, output)
    }
  } finally {
    if (child && child.exitCode === null) { child.kill('SIGTERM'); await once(child, 'exit') }
    await rm(directory, { recursive: true, force: true })
  }
})
