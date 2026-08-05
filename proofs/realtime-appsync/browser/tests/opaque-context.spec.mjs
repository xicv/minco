import { expect, test } from '@playwright/test'
import { readFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const currentDirectory = dirname(fileURLToPath(import.meta.url))
const modulePath = resolve(currentDirectory, '../../../../plugins/minco-plugin-realtime/assets/realtime-client.mjs')

test('packaged subscriber creates a bounded operation ID in an opaque browser context', async ({ page }) => {
  const moduleSource = await readFile(modulePath, 'utf8')
  await page.goto('about:blank')

  const result = await page.evaluate(async (source) => {
    const blob = new Blob([source], { type: 'text/javascript' })
    const moduleUrl = URL.createObjectURL(blob)
    try {
      const { createRealtimeClient } = await import(moduleUrl)
      class FakeWebSocket {
        static OPEN = 1
        static instance

        constructor() {
          this.readyState = 0
          this.sent = []
          FakeWebSocket.instance = this
        }

        open() {
          this.readyState = FakeWebSocket.OPEN
          this.onopen?.()
        }

        receive(message) {
          this.onmessage?.({ data: JSON.stringify(message) })
        }

        send(message) {
          this.sent.push(JSON.parse(message))
        }

        close() {
          this.readyState = 3
          this.onclose?.()
        }
      }

      const client = createRealtimeClient({
        realtimeUrl: 'wss://example.appsync-realtime-api.ap-southeast-2.amazonaws.com/event/realtime',
        httpUrl: 'https://example.appsync-api.ap-southeast-2.amazonaws.com/event',
        namespace: 'orders',
        channel: 'tenant-42/order-7',
        getToken: async () => 'token',
        resync: async () => {},
        onEvent: () => {},
        WebSocketImpl: FakeWebSocket,
      })
      await client.start()
      const socket = FakeWebSocket.instance
      socket.open()
      socket.receive({ type: 'connection_ack', connectionTimeoutMs: 300_000 })
      await new Promise(resolveTask => setTimeout(resolveTask, 0))
      const operationId = socket.sent.find(message => message.type === 'subscribe')?.id
      client.stop()
      return {
        operationId,
        randomUUIDAvailable: typeof globalThis.crypto.randomUUID === 'function',
      }
    }
    finally {
      URL.revokeObjectURL(moduleUrl)
    }
  }, moduleSource)

  expect(result.randomUUIDAvailable).toBe(false)
  expect(result.operationId).toMatch(/^[A-Za-z0-9-_+]{1,128}$/)
})
