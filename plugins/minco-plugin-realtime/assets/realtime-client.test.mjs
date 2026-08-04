import assert from 'node:assert/strict'
import test from 'node:test'
import { createRealtimeClient } from './realtime-client.mjs'

class FakeWebSocket {
  static instances = []
  static OPEN = 1

  constructor(url, protocols) {
    this.url = url
    this.protocols = protocols
    this.readyState = 0
    this.sent = []
    FakeWebSocket.instances.push(this)
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

class FakeDocument {
  constructor() {
    this.visibilityState = 'visible'
    this.listeners = new Set()
  }

  addEventListener(type, listener) {
    if (type === 'visibilitychange')
      this.listeners.add(listener)
  }

  removeEventListener(type, listener) {
    if (type === 'visibilitychange')
      this.listeners.delete(listener)
  }

  setVisibility(state) {
    this.visibilityState = state
    for (const listener of this.listeners)
      listener()
  }
}

class FakeTimers {
  constructor() {
    this.nextId = 1
    this.tasks = new Map()
  }

  setTimeout = (callback, delay) => {
    const id = this.nextId++
    this.tasks.set(id, { callback, delay })
    return id
  }

  clearTimeout = (id) => this.tasks.delete(id)

  run(delay) {
    const task = [...this.tasks.entries()].find(([, value]) => value.delay === delay)
    assert.ok(task, `missing timer ${delay}`)
    this.tasks.delete(task[0])
    task[1].callback()
  }
}

const nextTask = () => new Promise((resolve) => setTimeout(resolve, 0))

test('subscriber buffers live events until HTTP truth resync completes', async () => {
  FakeWebSocket.instances = []
  let finishResync
  const delivered = []
  const client = createRealtimeClient({
    realtimeUrl: 'wss://example.appsync-realtime-api.ap-southeast-2.amazonaws.com/event/realtime',
    httpUrl: 'https://example.appsync-api.ap-southeast-2.amazonaws.com/event',
    namespace: 'orders',
    channel: 'tenant-42/order-7',
    getToken: async () => 'jwt-secret-token',
    resync: () => new Promise((resolve) => { finishResync = resolve }),
    onEvent: (event) => delivered.push(event),
    WebSocketImpl: FakeWebSocket,
    idFactory: () => 'subscription-7',
  })

  await client.start()
  const socket = FakeWebSocket.instances[0]
  assert.equal(socket.url.includes('jwt-secret-token'), false)
  const authorization = JSON.parse(Buffer.from(socket.protocols[1].slice('header-'.length), 'base64url'))
  assert.deepEqual(authorization, {
    Authorization: 'jwt-secret-token',
    host: 'example.appsync-api.ap-southeast-2.amazonaws.com',
  })
  assert.equal(client.publish, undefined)
  socket.open()
  assert.deepEqual(socket.sent, [{ type: 'connection_init' }])
  socket.receive({ type: 'connection_ack', connectionTimeoutMs: 300_000 })
  await nextTask()
  assert.equal(socket.sent[1].type, 'subscribe')
  assert.equal(socket.sent[1].channel, '/orders/tenant-42/order-7')
  socket.receive({ type: 'subscribe_success', id: 'subscription-7' })
  socket.receive({
    type: 'data',
    id: 'subscription-7',
    event: ['{"id":"evt-7","event_type":"order.updated"}'],
  })
  assert.deepEqual(delivered, [])

  finishResync()
  await nextTask()

  assert.deepEqual(delivered, [{ id: 'evt-7', event_type: 'order.updated' }])
  assert.deepEqual(socket.sent.map(({ type }) => type), ['connection_init', 'subscribe'])
  client.stop()
})

test('authorization failures are reported generically and retried without leaking tokens', async () => {
  FakeWebSocket.instances = []
  const timers = new FakeTimers()
  const errors = []
  const client = createRealtimeClient({
    realtimeUrl: 'wss://example.appsync-realtime-api.ap-southeast-2.amazonaws.com/event/realtime',
    httpUrl: 'https://example.appsync-api.ap-southeast-2.amazonaws.com/event',
    namespace: 'orders',
    channel: 'tenant-42/order-7',
    getToken: async () => { throw new Error('private-token-detail') },
    resync: async () => {},
    onEvent: () => {},
    onError: (error) => errors.push(error.message),
    WebSocketImpl: FakeWebSocket,
    setTimeoutImpl: timers.setTimeout,
    clearTimeoutImpl: timers.clearTimeout,
    random: () => 0.5,
    idFactory: () => 'subscription-7',
  })

  await client.start()

  assert.deepEqual(errors, ['Realtime authorization is unavailable'])
  assert.equal(FakeWebSocket.instances.length, 0)
  assert.equal(errors.join(' ').includes('private-token-detail'), false)
  timers.run(500)
  await nextTask()
  assert.equal(errors.length, 2)
  client.stop()
})

test('invalid endpoints fail closed without echoing the supplied value', () => {
  const secretBearingEndpoint = 'not-a-url-private-token-detail'

  assert.throws(
    () => createRealtimeClient({
      realtimeUrl: secretBearingEndpoint,
      httpUrl: 'https://example.appsync-api.ap-southeast-2.amazonaws.com/event',
      namespace: 'orders',
      channel: 'tenant-42/order-7',
      getToken: async () => 'token',
      resync: async () => {},
      onEvent: () => {},
      WebSocketImpl: FakeWebSocket,
    }),
    (error) => error instanceof TypeError && !error.message.includes(secretBearingEndpoint),
  )
})

test('production endpoints must be the matching AppSync API pair', () => {
  const base = {
    namespace: 'orders',
    channel: 'tenant-42/order-7',
    getToken: async () => 'token',
    resync: async () => {},
    onEvent: () => {},
    WebSocketImpl: FakeWebSocket,
  }

  assert.throws(() => createRealtimeClient({
    ...base,
    realtimeUrl: 'wss://attacker.example.com/event/realtime',
    httpUrl: 'https://api-id.appsync-api.ap-southeast-2.amazonaws.com/event',
  }), /matching regional AppSync API/)
  assert.throws(() => createRealtimeClient({
    ...base,
    realtimeUrl: 'wss://first.appsync-realtime-api.ap-southeast-2.amazonaws.com/event/realtime',
    httpUrl: 'https://second.appsync-api.ap-southeast-2.amazonaws.com/event',
  }), /matching regional AppSync API/)
})

test('hidden UI disconnects after grace and visible UI reconnects with a fresh resync', async () => {
  FakeWebSocket.instances = []
  const documentImpl = new FakeDocument()
  const timers = new FakeTimers()
  let resyncs = 0
  const client = createRealtimeClient({
    realtimeUrl: 'wss://example.appsync-realtime-api.ap-southeast-2.amazonaws.com/event/realtime',
    httpUrl: 'https://example.appsync-api.ap-southeast-2.amazonaws.com/event',
    namespace: 'orders',
    channel: 'tenant-42/order-7',
    getToken: async () => 'jwt-secret-token',
    resync: async () => { resyncs += 1 },
    onEvent: () => {},
    WebSocketImpl: FakeWebSocket,
    documentImpl,
    setTimeoutImpl: timers.setTimeout,
    clearTimeoutImpl: timers.clearTimeout,
    hiddenGraceMs: 1_000,
    idFactory: () => `subscription-${FakeWebSocket.instances.length + 1}`,
  })

  await client.start()
  const first = FakeWebSocket.instances[0]
  first.open()
  first.receive({ type: 'connection_ack', connectionTimeoutMs: 300_000 })
  await nextTask()
  first.receive({ type: 'subscribe_success', id: 'subscription-1' })
  await nextTask()
  assert.equal(resyncs, 1)

  documentImpl.setVisibility('hidden')
  timers.run(1_000)
  assert.equal(first.readyState, 3)
  assert.equal(first.sent.at(-1).type, 'unsubscribe')
  documentImpl.setVisibility('visible')
  await nextTask()
  const second = FakeWebSocket.instances[1]
  second.open()
  second.receive({ type: 'connection_ack', connectionTimeoutMs: 300_000 })
  await nextTask()
  second.receive({ type: 'subscribe_success', id: 'subscription-2' })
  await nextTask()

  assert.equal(resyncs, 2)
  client.stop()
})

test('unexpected close uses bounded jitter and never sends a client keepalive', async () => {
  FakeWebSocket.instances = []
  const timers = new FakeTimers()
  const client = createRealtimeClient({
    realtimeUrl: 'wss://example.appsync-realtime-api.ap-southeast-2.amazonaws.com/event/realtime',
    httpUrl: 'https://example.appsync-api.ap-southeast-2.amazonaws.com/event',
    namespace: 'orders',
    channel: 'tenant-42/order-7',
    getToken: async () => 'jwt-secret-token',
    resync: async () => {},
    onEvent: () => {},
    WebSocketImpl: FakeWebSocket,
    setTimeoutImpl: timers.setTimeout,
    clearTimeoutImpl: timers.clearTimeout,
    random: () => 0.5,
    reconnectBaseMs: 1_000,
    reconnectMaxMs: 8_000,
    idFactory: () => `subscription-${FakeWebSocket.instances.length}`,
  })

  await client.start()
  const first = FakeWebSocket.instances[0]
  first.open()
  first.close()

  assert.deepEqual(first.sent.map(({ type }) => type), ['connection_init'])
  timers.run(500)
  await nextTask()

  assert.equal(FakeWebSocket.instances.length, 2)
  client.stop()
})

test('AWS keepalive only resets a local stale-connection deadline', async () => {
  FakeWebSocket.instances = []
  const timers = new FakeTimers()
  const client = createRealtimeClient({
    realtimeUrl: 'wss://example.appsync-realtime-api.ap-southeast-2.amazonaws.com/event/realtime',
    httpUrl: 'https://example.appsync-api.ap-southeast-2.amazonaws.com/event',
    namespace: 'orders',
    channel: 'tenant-42/order-7',
    getToken: async () => 'jwt-secret-token',
    resync: async () => {},
    onEvent: () => {},
    WebSocketImpl: FakeWebSocket,
    setTimeoutImpl: timers.setTimeout,
    clearTimeoutImpl: timers.clearTimeout,
    idFactory: () => 'subscription-7',
  })

  await client.start()
  const socket = FakeWebSocket.instances[0]
  socket.open()
  socket.receive({ type: 'connection_ack', connectionTimeoutMs: 300_000 })
  await nextTask()
  socket.receive({ type: 'ka' })
  assert.deepEqual(socket.sent.map(({ type }) => type), ['connection_init', 'subscribe'])

  timers.run(300_000)

  assert.equal(socket.readyState, 3)
  client.stop()
})

test('an open socket without connection acknowledgement closes at the handshake deadline', async () => {
  FakeWebSocket.instances = []
  const timers = new FakeTimers()
  const client = createRealtimeClient({
    realtimeUrl: 'wss://example.appsync-realtime-api.ap-southeast-2.amazonaws.com/event/realtime',
    httpUrl: 'https://example.appsync-api.ap-southeast-2.amazonaws.com/event',
    namespace: 'orders',
    channel: 'tenant-42/order-7',
    getToken: async () => 'jwt-secret-token',
    resync: async () => {},
    onEvent: () => {},
    WebSocketImpl: FakeWebSocket,
    setTimeoutImpl: timers.setTimeout,
    clearTimeoutImpl: timers.clearTimeout,
    handshakeTimeoutMs: 1_000,
    idFactory: () => 'subscription-7',
  })

  await client.start()
  const socket = FakeWebSocket.instances[0]
  socket.open()
  timers.run(1_000)

  assert.equal(socket.readyState, 3)
  assert.deepEqual(socket.sent, [{ type: 'connection_init' }])
  client.stop()
})

test('bounded resync buffer closes instead of accumulating unbounded events', async () => {
  FakeWebSocket.instances = []
  let finishResync
  const errors = []
  const client = createRealtimeClient({
    realtimeUrl: 'wss://example.appsync-realtime-api.ap-southeast-2.amazonaws.com/event/realtime',
    httpUrl: 'https://example.appsync-api.ap-southeast-2.amazonaws.com/event',
    namespace: 'orders',
    channel: 'tenant-42/order-7',
    getToken: async () => 'jwt-secret-token',
    resync: () => new Promise((resolve) => { finishResync = resolve }),
    onEvent: () => assert.fail('overflowed buffer must not deliver'),
    onError: (error) => errors.push(error.message),
    WebSocketImpl: FakeWebSocket,
    maxBufferedEvents: 1,
    idFactory: () => 'subscription-7',
  })

  await client.start()
  const socket = FakeWebSocket.instances[0]
  socket.open()
  socket.receive({ type: 'connection_ack', connectionTimeoutMs: 300_000 })
  await nextTask()
  socket.receive({ type: 'subscribe_success', id: 'subscription-7' })
  socket.receive({
    type: 'data',
    id: 'subscription-7',
    event: ['{"id":"evt-1"}', '{"id":"evt-2"}'],
  })

  assert.equal(socket.readyState, 3)
  assert.deepEqual(errors, ['Realtime buffer limit exceeded'])
  finishResync()
  client.stop()
})

test('oversized websocket messages close before JSON handling', async () => {
  FakeWebSocket.instances = []
  const errors = []
  const client = createRealtimeClient({
    realtimeUrl: 'wss://example.appsync-realtime-api.ap-southeast-2.amazonaws.com/event/realtime',
    httpUrl: 'https://example.appsync-api.ap-southeast-2.amazonaws.com/event',
    namespace: 'orders',
    channel: 'tenant-42/order-7',
    getToken: async () => 'token',
    resync: async () => {},
    onEvent: () => {},
    onError: error => errors.push(error.message),
    WebSocketImpl: FakeWebSocket,
    maxEventBytes: 256,
    maxMessageBytes: 256,
    idFactory: () => 'subscription-7',
  })

  await client.start()
  const socket = FakeWebSocket.instances[0]
  socket.open()
  socket.onmessage({ data: 'x'.repeat(257) })

  assert.equal(socket.readyState, 3)
  assert.deepEqual(errors, ['Realtime message size limit exceeded'])
  client.stop()
})
