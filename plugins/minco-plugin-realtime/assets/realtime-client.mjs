const CHANNEL_SEGMENT = /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,48}[A-Za-z0-9])?$/

export function createRealtimeClient(options) {
  return new RealtimeSubscriber(options)
}

class RealtimeSubscriber {
  constructor(options) {
    if (!options || typeof options !== 'object')
      throw new TypeError('realtime options are required')
    this.realtimeUrl = validateEndpoint(options.realtimeUrl, 'realtimeUrl', 'wss:', '/event/realtime')
    this.httpUrl = validateEndpoint(options.httpUrl, 'httpUrl', 'https:', '/event')
    validateEndpointPair(this.realtimeUrl, this.httpUrl)
    this.namespace = validateSegment(options.namespace, 'namespace')
    this.channel = validateChannel(options.channel)
    this.fullChannel = `/${this.namespace}/${this.channel}`
    this.getToken = requireFunction(options.getToken, 'getToken')
    this.resync = requireFunction(options.resync, 'resync')
    this.onEvent = requireFunction(options.onEvent, 'onEvent')
    this.onError = options.onError ?? (() => {})
    this.WebSocketImpl = options.WebSocketImpl ?? globalThis.WebSocket
    this.idFactory = options.idFactory ?? createOperationId
    this.documentImpl = options.documentImpl ?? globalThis.document
    this.setTimeoutImpl = options.setTimeoutImpl ?? globalThis.setTimeout.bind(globalThis)
    this.clearTimeoutImpl = options.clearTimeoutImpl ?? globalThis.clearTimeout.bind(globalThis)
    this.hiddenGraceMs = options.hiddenGraceMs ?? 15_000
    this.handshakeTimeoutMs = options.handshakeTimeoutMs ?? 15_000
    this.random = options.random ?? Math.random
    this.reconnectBaseMs = options.reconnectBaseMs ?? 1_000
    this.reconnectMaxMs = options.reconnectMaxMs ?? 30_000
    this.maxBufferedEvents = options.maxBufferedEvents ?? 1_000
    this.maxEventBytes = options.maxEventBytes ?? 5 * 1024
    this.maxMessageBytes = options.maxMessageBytes ?? Math.min(2 * 1024 * 1024, this.maxEventBytes * 5 + 4096)
    if (typeof this.WebSocketImpl !== 'function')
      throw new TypeError('WebSocket is unavailable')
    requireFunction(this.onError, 'onError')
    requireFunction(this.idFactory, 'idFactory')
    requireFunction(this.setTimeoutImpl, 'setTimeoutImpl')
    requireFunction(this.clearTimeoutImpl, 'clearTimeoutImpl')
    requireFunction(this.random, 'random')
    if (!Number.isInteger(this.hiddenGraceMs) || this.hiddenGraceMs < 0 || this.hiddenGraceMs > 300_000)
      throw new TypeError('hiddenGraceMs must be between 0 and 300000')
    if (!Number.isInteger(this.handshakeTimeoutMs)
      || this.handshakeTimeoutMs < 1_000
      || this.handshakeTimeoutMs > 60_000)
      throw new TypeError('handshakeTimeoutMs must be between 1000 and 60000')
    if (!Number.isInteger(this.reconnectBaseMs)
      || !Number.isInteger(this.reconnectMaxMs)
      || this.reconnectBaseMs < 250
      || this.reconnectMaxMs < this.reconnectBaseMs
      || this.reconnectMaxMs > 300_000)
      throw new TypeError('reconnect bounds are invalid')
    if (!Number.isInteger(this.maxBufferedEvents)
      || this.maxBufferedEvents < 1
      || this.maxBufferedEvents > 10_000)
      throw new TypeError('maxBufferedEvents must be between 1 and 10000')
    if (!Number.isInteger(this.maxEventBytes)
      || this.maxEventBytes < 256
      || this.maxEventBytes > 240 * 1024)
      throw new TypeError('maxEventBytes must be between 256 and 245760')
    if (!Number.isInteger(this.maxMessageBytes)
      || this.maxMessageBytes < this.maxEventBytes
      || this.maxMessageBytes > 2 * 1024 * 1024)
      throw new TypeError('maxMessageBytes must cover one event and be at most 2097152')
    this.running = false
    this.socket = undefined
    this.subscriptionId = undefined
    this.subscribed = false
    this.resyncing = false
    this.buffer = []
    this.generation = 0
    this.hiddenTimer = undefined
    this.listeningForVisibility = false
    this.visibilityHandler = () => this.handleVisibility()
    this.reconnectAttempt = 0
    this.reconnectTimer = undefined
    this.keepaliveTimer = undefined
    this.connectionTimeoutMs = 300_000
  }

  async start() {
    if (this.running)
      return
    this.running = true
    if (this.documentImpl && !this.listeningForVisibility) {
      this.documentImpl.addEventListener('visibilitychange', this.visibilityHandler)
      this.listeningForVisibility = true
    }
    if (this.documentImpl?.visibilityState === 'hidden')
      return
    await this.connect()
  }

  stop() {
    this.running = false
    this.clearHiddenTimer()
    this.clearReconnectTimer()
    if (this.documentImpl && this.listeningForVisibility) {
      this.documentImpl.removeEventListener('visibilitychange', this.visibilityHandler)
      this.listeningForVisibility = false
    }
    this.disconnect()
  }

  disconnect() {
    this.clearReconnectTimer()
    this.clearKeepaliveTimer()
    this.generation += 1
    const socket = this.socket
    this.socket = undefined
    if (!socket)
      return
    if (this.subscribed && socket.readyState === this.WebSocketImpl.OPEN) {
      socket.send(JSON.stringify({ type: 'unsubscribe', id: this.subscriptionId }))
    }
    socket.onclose = undefined
    socket.close()
    this.subscribed = false
    this.resyncing = false
    this.buffer = []
  }

  handleVisibility() {
    if (!this.running)
      return
    if (this.documentImpl?.visibilityState === 'hidden') {
      this.clearHiddenTimer()
      this.hiddenTimer = this.setTimeoutImpl(() => {
        this.hiddenTimer = undefined
        if (this.running && this.documentImpl?.visibilityState === 'hidden')
          this.disconnect()
      }, this.hiddenGraceMs)
      return
    }
    this.clearHiddenTimer()
    if (!this.socket)
      void this.connect()
  }

  clearHiddenTimer() {
    if (this.hiddenTimer !== undefined) {
      this.clearTimeoutImpl(this.hiddenTimer)
      this.hiddenTimer = undefined
    }
  }

  clearReconnectTimer() {
    if (this.reconnectTimer !== undefined) {
      this.clearTimeoutImpl(this.reconnectTimer)
      this.reconnectTimer = undefined
    }
  }

  clearKeepaliveTimer() {
    if (this.keepaliveTimer !== undefined) {
      this.clearTimeoutImpl(this.keepaliveTimer)
      this.keepaliveTimer = undefined
    }
  }

  resetConnectionDeadline(socket, generation, timeoutMs) {
    this.clearKeepaliveTimer()
    this.keepaliveTimer = this.setTimeoutImpl(() => {
      this.keepaliveTimer = undefined
      if (this.isCurrent(socket, generation))
        socket.close()
    }, timeoutMs)
  }

  scheduleReconnect() {
    if (!this.running || this.documentImpl?.visibilityState === 'hidden' || this.reconnectTimer !== undefined)
      return
    const cap = Math.min(
      this.reconnectMaxMs,
      this.reconnectBaseMs * (2 ** Math.min(this.reconnectAttempt, 16)),
    )
    const sample = Number(this.random())
    const delay = Math.floor(Math.max(0, Math.min(1, Number.isFinite(sample) ? sample : 1)) * cap)
    this.reconnectAttempt += 1
    this.reconnectTimer = this.setTimeoutImpl(() => {
      this.reconnectTimer = undefined
      if (this.running && this.documentImpl?.visibilityState !== 'hidden')
        void this.connect()
    }, delay)
  }

  async connect() {
    const generation = ++this.generation
    let token
    try {
      token = await readToken(this.getToken)
    }
    catch {
      if (this.running && generation === this.generation) {
        this.fail('Realtime authorization is unavailable')
        this.scheduleReconnect()
      }
      return
    }
    if (!this.running || generation !== this.generation)
      return
    const authorization = {
      Authorization: token,
      host: this.httpUrl.host,
    }
    const protocols = [
      'aws-appsync-event-ws',
      authProtocol(authorization),
    ]
    this.subscriptionId = String(this.idFactory())
    validateOperationId(this.subscriptionId)
    const socket = new this.WebSocketImpl(this.realtimeUrl.toString(), protocols)
    this.socket = socket
    this.subscribed = false
    this.resyncing = false
    this.buffer = []
    socket.onopen = () => {
      if (this.isCurrent(socket, generation)) {
        socket.send(JSON.stringify({ type: 'connection_init' }))
        this.resetConnectionDeadline(socket, generation, this.handshakeTimeoutMs)
      }
    }
    socket.onmessage = (message) => {
      if (!this.isCurrent(socket, generation))
        return
      if (typeof message.data !== 'string'
        || message.data.length > this.maxMessageBytes
        || new TextEncoder().encode(message.data).byteLength > this.maxMessageBytes) {
        this.fail('Realtime message size limit exceeded', true)
        return
      }
      try {
        this.handleMessage(socket, generation, JSON.parse(message.data))
      }
      catch {
        this.fail('AppSync Events sent an invalid message', true)
      }
    }
    socket.onerror = () => this.fail('AppSync Events connection failed', true)
    socket.onclose = () => {
      if (!this.isCurrent(socket, generation))
        return
      this.socket = undefined
      this.clearKeepaliveTimer()
      this.subscribed = false
      this.resyncing = false
      this.buffer = []
      this.scheduleReconnect()
    }
  }

  handleMessage(socket, generation, message) {
    switch (message?.type) {
      case 'connection_ack':
        this.connectionTimeoutMs = Number.isInteger(message.connectionTimeoutMs)
          && message.connectionTimeoutMs >= 1_000
          && message.connectionTimeoutMs <= 600_000
          ? message.connectionTimeoutMs
          : 300_000
        this.resetConnectionDeadline(socket, generation, this.connectionTimeoutMs)
        void this.subscribe(socket, generation)
        break
      case 'subscribe_success':
        if (message.id === this.subscriptionId)
          void this.completeSubscription(socket, generation)
        break
      case 'data':
        if (message.id === this.subscriptionId)
          this.acceptEvents(message.event)
        break
      case 'subscribe_error':
      case 'broadcast_error':
        this.fail('AppSync Events subscription failed', true)
        break
      case 'ka':
        this.resetConnectionDeadline(socket, generation, this.connectionTimeoutMs)
        break
      default:
        break
    }
  }

  async subscribe(socket, generation) {
    let token
    try {
      token = await readToken(this.getToken)
    }
    catch {
      if (this.isCurrent(socket, generation))
        this.fail('Realtime authorization is unavailable', true)
      return
    }
    if (!this.isCurrent(socket, generation))
      return
    socket.send(JSON.stringify({
      type: 'subscribe',
      id: this.subscriptionId,
      channel: this.fullChannel,
      authorization: { Authorization: token },
    }))
  }

  async completeSubscription(socket, generation) {
    this.subscribed = true
    this.resyncing = true
    try {
      await this.resync()
      if (!this.isCurrent(socket, generation))
        return
      this.resyncing = false
      this.reconnectAttempt = 0
      const buffered = this.buffer
      this.buffer = []
      for (const event of buffered)
        this.onEvent(event)
    }
    catch {
      this.fail('Realtime truth resync failed', true)
    }
  }

  acceptEvents(events) {
    if (typeof events === 'string')
      events = [events]
    if (!Array.isArray(events))
      throw new TypeError('data event must contain an array')
    for (const encoded of events) {
      if (typeof encoded !== 'string')
        throw new TypeError('data event payload must be encoded JSON')
      if (new TextEncoder().encode(encoded).byteLength > this.maxEventBytes) {
        this.fail('Realtime event size limit exceeded', true)
        return
      }
      const event = JSON.parse(encoded)
      if (this.resyncing || !this.subscribed) {
        if (this.buffer.length >= this.maxBufferedEvents) {
          this.fail('Realtime buffer limit exceeded', true)
          return
        }
        this.buffer.push(event)
      }
      else {
        this.onEvent(event)
      }
    }
  }

  isCurrent(socket, generation) {
    return this.running && this.socket === socket && this.generation === generation
  }

  fail(message, close = false) {
    try {
      this.onError(new Error(message))
    }
    catch {
      // Application error reporting must not break connection cleanup.
    }
    finally {
      if (close && this.socket)
        this.socket.close()
    }
  }
}

function validateEndpoint(value, field, requiredProtocol, requiredPath) {
  let url
  try {
    url = new URL(value)
  }
  catch {
    throw new TypeError(`${field} must be the exact secure AppSync Events endpoint`)
  }
  const loopback = ['localhost', '127.0.0.1', '::1'].includes(url.hostname)
  const protocolAllowed = url.protocol === requiredProtocol
    || (loopback && requiredProtocol === 'wss:' && url.protocol === 'ws:')
    || (loopback && requiredProtocol === 'https:' && url.protocol === 'http:')
  if (!protocolAllowed || url.pathname !== requiredPath || url.search || url.hash || url.username || url.password)
    throw new TypeError(`${field} must be the exact secure AppSync Events endpoint`)
  return url
}

function validateEndpointPair(realtimeUrl, httpUrl) {
  const realtimeLoopback = isLoopback(realtimeUrl.hostname)
  const httpLoopback = isLoopback(httpUrl.hostname)
  if (realtimeLoopback && httpLoopback)
    return
  const realtime = appsyncEndpointIdentity(realtimeUrl.hostname, 'realtime')
  const http = appsyncEndpointIdentity(httpUrl.hostname, 'http')
  if (!realtime
    || !http
    || realtime.apiId !== http.apiId
    || realtime.region !== http.region
    || realtime.partition !== http.partition)
    throw new TypeError('realtimeUrl and httpUrl must identify the matching regional AppSync API')
}

function appsyncEndpointIdentity(hostname, kind) {
  const service = kind === 'realtime' ? 'appsync-realtime-api' : 'appsync-api'
  const match = hostname.match(new RegExp(
    `^([a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)\\.${service}\\.([a-z0-9](?:[a-z0-9-]{1,30}[a-z0-9]))\\.(amazonaws\\.com(?:\\.cn)?)$`,
  ))
  if (!match)
    return undefined
  return { apiId: match[1], region: match[2], partition: match[3] }
}

function isLoopback(hostname) {
  return ['localhost', '127.0.0.1', '::1'].includes(hostname)
}

function validateSegment(value, field) {
  if (typeof value !== 'string' || !CHANNEL_SEGMENT.test(value))
    throw new TypeError(`${field} is not a portable AppSync channel segment`)
  return value
}

function validateChannel(value) {
  if (typeof value !== 'string')
    throw new TypeError('channel is required')
  const segments = value.split('/')
  if (!(1 <= segments.length && segments.length <= 4) || segments.some(segment => !CHANNEL_SEGMENT.test(segment)))
    throw new TypeError('channel must contain one to four portable segments')
  return value
}

function validateOperationId(value) {
  if (!/^[A-Za-z0-9-_+]{1,128}$/.test(value))
    throw new TypeError('subscription operation id is invalid')
}

function createOperationId() {
  const crypto = globalThis.crypto
  if (typeof crypto?.randomUUID === 'function')
    return crypto.randomUUID()
  if (typeof crypto?.getRandomValues !== 'function')
    throw new TypeError('secure random operation ID generation is unavailable')
  const bytes = crypto.getRandomValues(new Uint8Array(16))
  return Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('')
}

function requireFunction(value, field) {
  if (typeof value !== 'function')
    throw new TypeError(`${field} must be a function`)
  return value
}

async function readToken(getToken) {
  const token = await getToken()
  if (typeof token !== 'string' || token.length === 0 || token.length > 16 * 1024 || /[^\x20-\x7E]/.test(token))
    throw new TypeError('getToken must return a bounded printable token')
  return token
}

function authProtocol(authorization) {
  const bytes = new TextEncoder().encode(JSON.stringify(authorization))
  let binary = ''
  for (const byte of bytes)
    binary += String.fromCharCode(byte)
  return `header-${btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')}`
}
