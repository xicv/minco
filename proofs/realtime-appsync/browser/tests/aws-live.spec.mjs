import { expect, test } from '@playwright/test'
import { execFile } from 'node:child_process'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { createServer } from 'node:http'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { promisify } from 'node:util'

const execute = promisify(execFile)
const currentDirectory = dirname(fileURLToPath(import.meta.url))
const modulePath = resolve(currentDirectory, '../../../../plugins/minco-plugin-realtime/assets/realtime-client.mjs')
const required = [
  'MINCO_APPSYNC_PROOF_HTTP_ENDPOINT',
  'MINCO_APPSYNC_PROOF_WS_ENDPOINT',
  'MINCO_APPSYNC_PROOF_ID_TOKEN',
  'MINCO_APPSYNC_PROOF_SUB',
  'MINCO_APPSYNC_PROOF_FUNCTION',
  'MINCO_APPSYNC_PROOF_PROFILE',
  'AWS_REGION',
]
const live = required.every(name => process.env[name])

async function startTruthServer({ holdFirstResponse = false } = {}) {
  let firstResyncResponse
  let resyncCount = 0
  const server = createServer((request, response) => {
    if (request.method === 'GET' && request.url === '/proof') {
      response.setHeader('Content-Type', 'text/html; charset=utf-8')
      response.end('<!doctype html><title>Minco realtime proof</title>')
      return
    }
    if (request.method !== 'GET' || request.url !== '/authoritative-state') {
      response.statusCode = 404
      response.end()
      return
    }

    resyncCount += 1
    response.setHeader('Content-Type', 'application/json')
    const finish = () => response.end(JSON.stringify({ revision: resyncCount }))
    if (holdFirstResponse && resyncCount === 1)
      firstResyncResponse = finish
    else
      finish()
  })
  await new Promise((resolveListen, rejectListen) => {
    server.once('error', rejectListen)
    server.listen(0, '127.0.0.1', resolveListen)
  })
  const address = server.address()
  if (!address || typeof address === 'string')
    throw new Error('proof truth server did not bind a TCP port')
  const origin = `http://127.0.0.1:${address.port}`

  return {
    pageUrl: `${origin}/proof`,
    resyncUrl: `${origin}/authoritative-state`,
    get resyncCount() { return resyncCount },
    get firstResyncPending() { return typeof firstResyncResponse === 'function' },
    releaseFirstResync() {
      if (!firstResyncResponse)
        throw new Error('first truth resynchronization is not pending')
      const finish = firstResyncResponse
      firstResyncResponse = undefined
      finish()
    },
    async close() {
      if (firstResyncResponse) {
        const finish = firstResyncResponse
        firstResyncResponse = undefined
        finish()
      }
      await new Promise((resolveClose, rejectClose) => {
        server.close(error => error ? rejectClose(error) : resolveClose())
      })
    },
  }
}

test('proof browser page can reach its loopback HTTP truth boundary', async ({ page }) => {
  const truth = await startTruthServer()

  try {
    await page.goto(truth.pageUrl)
    const reachable = await page.evaluate(async () => {
      try {
        const response = await fetch('/authoritative-state')
        return response.ok && (await response.json()).revision === 1
      }
      catch {
        return false
      }
    })
    expect(reachable).toBe(true)
    expect(truth.resyncCount).toBe(1)
  }
  finally {
    await truth.close()
  }
})

test('packaged subscriber receives IAM publication after HTTP resync and rejects a mismatched channel', async ({ page }) => {
  test.skip(!live, `live AppSync proof requires ${required.join(', ')}`)
  test.setTimeout(120_000)
  const moduleSource = await readFile(modulePath, 'utf8')
  const temporary = await mkdtemp(join(tmpdir(), 'minco-appsync-live-'))
  const truth = await startTruthServer({ holdFirstResponse: true })

  const invoke = async sequence => {
    const output = join(temporary, `invoke-${sequence}.json`)
    const payload = JSON.stringify({
      channel: `${process.env.MINCO_APPSYNC_PROOF_SUB}/orders`,
      sequence,
    })
    await execute('aws', [
      '--profile', process.env.MINCO_APPSYNC_PROOF_PROFILE,
      '--region', process.env.AWS_REGION,
      '--cli-connect-timeout', '5',
      '--cli-read-timeout', '20',
      'lambda', 'invoke',
      '--function-name', process.env.MINCO_APPSYNC_PROOF_FUNCTION,
      '--cli-binary-format', 'raw-in-base64-out',
      '--payload', payload,
      '--output', 'json',
      output,
    ], { maxBuffer: 64 * 1024 })
    const result = JSON.parse(await readFile(output, 'utf8'))
    expect(result).toEqual({ published: true, id: `live-${sequence}` })
  }

  try {
    await page.goto(truth.pageUrl)
    await page.evaluate(async options => {
      const blob = new Blob([options.moduleSource], { type: 'text/javascript' })
      const { createRealtimeClient } = await import(URL.createObjectURL(blob))
      class ProofDocument extends EventTarget {
        visibilityState = 'visible'
        setVisibility(value) {
          this.visibilityState = value
          this.dispatchEvent(new Event('visibilitychange'))
        }
      }
      const documentImpl = new ProofDocument()
      const state = { events: [], errors: [], resyncs: 0 }
      const client = createRealtimeClient({
        realtimeUrl: options.realtimeUrl,
        httpUrl: options.httpUrl,
        namespace: 'orders',
        channel: `${options.subject}/orders`,
        getToken: async () => options.token,
        resync: async () => {
          const response = await fetch(options.resyncUrl)
          if (!response.ok)
            throw new Error('resync failed')
          await response.json()
          state.resyncs += 1
        },
        onEvent: event => state.events.push(event),
        onError: error => state.errors.push(error.message),
        documentImpl,
        hiddenGraceMs: 100,
      })
      globalThis.__mincoProof = { client, documentImpl, state, createRealtimeClient, options }
      await client.start()
    }, {
      moduleSource,
      realtimeUrl: process.env.MINCO_APPSYNC_PROOF_WS_ENDPOINT,
      httpUrl: process.env.MINCO_APPSYNC_PROOF_HTTP_ENDPOINT,
      token: process.env.MINCO_APPSYNC_PROOF_ID_TOKEN,
      subject: process.env.MINCO_APPSYNC_PROOF_SUB,
      resyncUrl: truth.resyncUrl,
    })

    await expect.poll(async () => {
      const errors = await page.evaluate(() => globalThis.__mincoProof.state.errors)
      if (errors.length > 0)
        throw new Error(`initial subscription failed safely: ${errors[0]}`)
      return truth.resyncCount
    }, {
      message: 'initial subscription did not start HTTP truth resynchronization',
      timeout: 20_000,
    }).toBe(1)
    expect(truth.firstResyncPending).toBe(true)
    await invoke(1)
    await page.waitForTimeout(250)
    expect(await page.evaluate(() => globalThis.__mincoProof.state.events)).toEqual([])
    truth.releaseFirstResync()
    await expect.poll(async () => {
      const state = await page.evaluate(() => globalThis.__mincoProof.state)
      if (state.errors.length > 0)
        throw new Error(`first event delivery failed safely: ${state.errors[0]}`)
      return state.events.length
    }, { message: 'first IAM publication was not delivered', timeout: 20_000 }).toBe(1)
    expect(await page.evaluate(() => globalThis.__mincoProof.state.events[0].payload.sequence)).toBe(1)

    await page.evaluate(() => globalThis.__mincoProof.documentImpl.setVisibility('hidden'))
    await page.waitForTimeout(250)
    await page.evaluate(() => globalThis.__mincoProof.documentImpl.setVisibility('visible'))
    await expect.poll(async () => {
      const errors = await page.evaluate(() => globalThis.__mincoProof.state.errors)
      if (errors.length > 0)
        throw new Error(`visibility reconnect failed safely: ${errors[0]}`)
      return truth.resyncCount
    }, { message: 'visibility reconnect did not resynchronize HTTP truth', timeout: 20_000 }).toBe(2)
    await invoke(2)
    await expect.poll(async () => {
      const state = await page.evaluate(() => globalThis.__mincoProof.state)
      if (state.errors.length > 0)
        throw new Error(`second event delivery failed safely: ${state.errors[0]}`)
      return state.events.length
    }, { message: 'second IAM publication was not delivered', timeout: 20_000 }).toBe(2)

    await page.evaluate(async () => {
      const proof = globalThis.__mincoProof
      let rejected
      rejected = proof.createRealtimeClient({
        realtimeUrl: proof.options.realtimeUrl,
        httpUrl: proof.options.httpUrl,
        namespace: 'orders',
        channel: 'wrong-subject/orders',
        getToken: async () => proof.options.token,
        resync: async () => { throw new Error('unauthorized subscription resynced') },
        onEvent: () => { throw new Error('unauthorized subscription received an event') },
        onError: (error) => {
          proof.state.errors.push(error.message)
          rejected.stop()
        },
      })
      proof.rejected = rejected
      await rejected.start()
    })
    await expect.poll(
      () => page.evaluate(() => globalThis.__mincoProof.state.errors),
      { message: 'mismatched channel subscription was not rejected', timeout: 20_000 },
    ).toContain('AppSync Events subscription failed')
    const safeState = await page.evaluate(() => {
      const proof = globalThis.__mincoProof
      proof.rejected.stop()
      proof.client.stop()
      return {
        events: proof.state.events.map(event => event.id),
        errors: proof.state.errors,
        tokenLeaked: proof.state.errors.join(' ').includes(proof.options.token),
        resyncs: proof.state.resyncs,
      }
    })
    expect(safeState).toEqual({
      events: ['live-1', 'live-2'],
      errors: ['AppSync Events subscription failed'],
      tokenLeaked: false,
      resyncs: 2,
    })
  }
  finally {
    await truth.close()
    await rm(temporary, { recursive: true, force: true })
  }
})
