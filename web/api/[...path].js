const MAX_REQUEST_BYTES = 32 * 1024
const MAX_RESPONSE_BYTES = 4 * 1024 * 1024
const PROXY_SECRET_HEADER = 'x-codexpt-proxy-key'
const FORWARDED_HEADERS = [
  'accept',
  'content-type',
  'cookie',
  'origin',
  'user-agent',
  'x-csrf-token',
  'x-codexpt-workspace-id',
]
const HOP_BY_HOP_HEADERS = new Set([
  'connection',
  'keep-alive',
  'proxy-authenticate',
  'proxy-authorization',
  'te',
  'trailer',
  'transfer-encoding',
  'upgrade',
])
const RESPONSE_HEADERS = new Set([
  'content-security-policy',
  'content-type',
  'location',
  'permissions-policy',
  'referrer-policy',
  'retry-after',
  'strict-transport-security',
  'vary',
  'www-authenticate',
  'x-content-type-options',
  'x-frame-options',
])
const ALLOWED_METHODS = new Set(['GET', 'HEAD', 'POST', 'PATCH', 'DELETE', 'OPTIONS'])

export const config = {
  api: { bodyParser: { sizeLimit: '32kb' } },
}

function responseError(res, status, error) {
  res.statusCode = status
  res.setHeader('Content-Type', 'application/json; charset=utf-8')
  res.setHeader('Cache-Control', 'no-store')
  res.end(JSON.stringify({ error }))
}

function configuredBackendOrigin(value, expectedHostname) {
  if (!value || !expectedHostname) return null
  try {
    const origin = new URL(value)
    if (
      origin.protocol !== 'https:' ||
      value !== origin.origin ||
      origin.username ||
      origin.password ||
      origin.hostname !== expectedHostname ||
      origin.pathname !== '/' ||
      origin.search ||
      origin.hash ||
      !/^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.up\.railway\.app$/.test(origin.hostname)
    ) return null
    return origin
  } catch {
    return null
  }
}

function headerString(value, separator = ', ') {
  if (Array.isArray(value)) return value.join(separator)
  return typeof value === 'string' ? value : undefined
}

async function readBody(req) {
  if (req.body !== undefined) {
    const value = Buffer.isBuffer(req.body)
      ? req.body
      : typeof req.body === 'string'
        ? Buffer.from(req.body)
        : Buffer.from(JSON.stringify(req.body))
    return value.length <= MAX_REQUEST_BYTES ? value : null
  }

  const chunks = []
  let size = 0
  for await (const chunk of req) {
    const bytes = Buffer.from(chunk)
    size += bytes.length
    if (size > MAX_REQUEST_BYTES) return null
    chunks.push(bytes)
  }
  return Buffer.concat(chunks, size)
}

async function readResponseBody(response) {
  const declaredLength = Number(response.headers.get('content-length'))
  if (Number.isFinite(declaredLength) && declaredLength > MAX_RESPONSE_BYTES) return null
  if (!response.body) return Buffer.alloc(0)

  const chunks = []
  let size = 0
  for await (const chunk of response.body) {
    const bytes = Buffer.from(chunk)
    size += bytes.length
    if (size > MAX_RESPONSE_BYTES) return null
    chunks.push(bytes)
  }
  return Buffer.concat(chunks, size)
}

export async function proxyHandler(req, res, dependencies = {}) {
  const env = dependencies.env ?? process.env
  const fetchImpl = dependencies.fetchImpl ?? fetch
  const backendOrigin = configuredBackendOrigin(
    env.CODEXPT_RAILWAY_ORIGIN,
    env.CODEXPT_RAILWAY_HOST,
  )
  const proxySecret = env.CODEXPT_PROXY_SECRET

  if (!backendOrigin || typeof proxySecret !== 'string' || proxySecret.length < 32 || proxySecret.trim() !== proxySecret) {
    return responseError(res, 503, 'API proxy is not configured')
  }

  const method = (req.method ?? 'GET').toUpperCase()
  if (!ALLOWED_METHODS.has(method)) {
    res.setHeader('Allow', [...ALLOWED_METHODS].join(', '))
    return responseError(res, 405, 'method not allowed')
  }

  let incoming
  try {
    incoming = new URL(req.url ?? '/', 'https://vercel.invalid')
  } catch {
    return responseError(res, 400, 'invalid request path')
  }
  if (!incoming.pathname.startsWith('/api/')) {
    return responseError(res, 404, 'not found')
  }

  const headers = new Headers()
  for (const name of FORWARDED_HEADERS) {
    const value = headerString(req.headers?.[name], name === 'cookie' ? '; ' : ', ')
    if (value !== undefined) headers.set(name, value)
  }
  // Never forward a client-supplied internal credential; set only the server secret.
  headers.set(PROXY_SECRET_HEADER, proxySecret)

  const options = { method, headers, redirect: 'manual', cache: 'no-store' }
  if (!['GET', 'HEAD'].includes(method)) {
    const declaredLength = Number(req.headers?.['content-length'])
    if (Number.isFinite(declaredLength) && declaredLength > MAX_REQUEST_BYTES) {
      return responseError(res, 413, 'request body is too large')
    }
    const body = await readBody(req)
    if (body === null) return responseError(res, 413, 'request body is too large')
    if (body.length) options.body = body
  }

  const target = new URL(backendOrigin)
  target.pathname = incoming.pathname
  target.search = incoming.search

  let upstream
  try {
    upstream = await fetchImpl(target, { ...options, signal: AbortSignal.timeout(25_000) })
  } catch (error) {
    const timedOut = error?.name === 'TimeoutError' || error?.name === 'AbortError'
    return responseError(res, timedOut ? 504 : 502, timedOut ? 'backend timed out' : 'backend unavailable')
  }

  if (typeof upstream.headers.getSetCookie !== 'function') {
    return responseError(res, 502, 'backend response is unsupported')
  }

  const bodyless = method === 'HEAD' || upstream.status === 204 || upstream.status === 304
  let body = Buffer.alloc(0)
  if (!bodyless) {
    try {
      body = await readResponseBody(upstream)
    } catch (error) {
      const timedOut = error?.name === 'TimeoutError' || error?.name === 'AbortError'
      return responseError(res, timedOut ? 504 : 502, timedOut ? 'backend timed out' : 'backend unavailable')
    }
    if (body === null) return responseError(res, 502, 'backend response is too large')
  }

  res.statusCode = upstream.status
  for (const [name, value] of upstream.headers) {
    if (RESPONSE_HEADERS.has(name) && !HOP_BY_HOP_HEADERS.has(name)) res.setHeader(name, value)
  }
  const cookies = upstream.headers.getSetCookie()
  if (cookies.length) res.setHeader('Set-Cookie', cookies)
  res.setHeader('Cache-Control', 'no-store')

  return res.end(body)
}

export default async function handler(req, res) {
  return proxyHandler(req, res)
}
