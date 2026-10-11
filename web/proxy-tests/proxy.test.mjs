import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { proxyHandler } from '../api/[...path].js'
import apiRootHandler from '../api/index.js'

class MockResponse {
  headers = new Map()
  statusCode = 200
  body

  setHeader(name, value) { this.headers.set(name.toLowerCase(), value) }
  end(body = Buffer.alloc(0)) { this.body = Buffer.from(body); return this }
}

const configured = {
  CODEXPT_RAILWAY_ORIGIN: 'https://codexpt-production.up.railway.app',
  CODEXPT_RAILWAY_HOST: 'codexpt-production.up.railway.app',
  CODEXPT_PROXY_SECRET: 'test-secret-0123456789-0123456789-abcdef',
}

test('Vercel SPA fallback serves app routes without intercepting API functions', () => {
  const config = JSON.parse(readFileSync(new URL('../vercel.json', import.meta.url), 'utf8'))
  assert.deepEqual(config.rewrites[0], { source: '/api/:path*', destination: '/api?__codexpt_path=:path*' })
  const fallback = config.rewrites.find((rewrite) => rewrite.destination === '/index.html' && rewrite.source.includes('(?!api/'))
  assert.ok(fallback)
  const source = new RegExp(`^${fallback.source}$`)
  assert.equal(source.test('/login'), true)
  assert.equal(source.test('/ruta-profunda'), true)
  assert.equal(source.test('/api'), false)
  assert.equal(source.test('/api/auth/status'), false)
})

test('proxy forwards same-origin OIDC callback without following redirects or losing cookies', async () => {
  let called
  const req = {
    method: 'GET',
    url: '/api/auth/callback?code=opaque&state=opaque-state',
    headers: {
      host: 'codexpt.jappi.net.pe',
      cookie: '__Host-codexpt_oidc=state-cookie',
      origin: undefined,
      'x-codexpt-proxy-key': 'attacker-controlled',
    },
  }
  const res = new MockResponse()
  const responseHeaders = new Headers({
    location: 'https://codexpt.jappi.net.pe/',
    'x-content-type-options': 'nosniff',
    'content-security-policy': "default-src 'self'",
  })
  responseHeaders.append('set-cookie', '__Host-codexpt_session=session; Path=/; Secure; HttpOnly; SameSite=Lax')
  responseHeaders.append('set-cookie', '__Host-codexpt_oidc=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0')
  await proxyHandler(req, res, {
    env: configured,
    fetchImpl: async (url, options) => {
      called = { url: String(url), options }
      return new Response(null, { status: 303, headers: responseHeaders })
    },
  })

  assert.equal(called.url, 'https://codexpt-production.up.railway.app/api/auth/callback?code=opaque&state=opaque-state')
  assert.equal(called.options.redirect, 'manual')
  assert.equal(called.options.headers.get('cookie'), '__Host-codexpt_oidc=state-cookie')
  assert.equal(called.options.headers.get('x-codexpt-proxy-key'), configured.CODEXPT_PROXY_SECRET)
  assert.equal(res.statusCode, 303)
  assert.equal(res.headers.get('location'), 'https://codexpt.jappi.net.pe/')
  assert.equal(res.headers.get('set-cookie').length, 2)
  assert.match(res.headers.get('set-cookie')[0], /HttpOnly/)
  assert.equal(res.headers.get('cache-control'), 'no-store')
  assert.equal(res.headers.get('x-content-type-options'), 'nosniff')
  assert.equal(res.headers.get('content-security-policy'), "default-src 'self'")
})

test('proxy forwards CSRF and workspace headers but strips client host and proxy credentials', async () => {
  let called
  const req = {
    method: 'POST',
    url: '/api/tasks?limit=10',
    headers: {
      host: 'codexpt.jappi.net.pe',
      'content-type': 'application/json',
      cookie: '__Host-codexpt_session=session',
      origin: 'https://codexpt.jappi.net.pe',
      'x-csrf-token': 'csrf',
      'x-codexpt-workspace-id': 'workspace-1',
      'x-codexpt-proxy-key': ['attacker-controlled', 'duplicate-attacker-value'],
    },
    body: { title: 'Review', description: '' },
  }
  const res = new MockResponse()
  await proxyHandler(req, res, {
    env: configured,
    fetchImpl: async (url, options) => {
      called = { url: String(url), options }
      return Response.json({ data: { task_id: 'task-1' } }, { status: 201 })
    },
  })

  assert.equal(called.url, 'https://codexpt-production.up.railway.app/api/tasks?limit=10')
  assert.equal(called.options.headers.get('origin'), 'https://codexpt.jappi.net.pe')
  assert.equal(called.options.headers.get('x-csrf-token'), 'csrf')
  assert.equal(called.options.headers.get('x-codexpt-workspace-id'), 'workspace-1')
  assert.equal(called.options.headers.has('host'), false)
  assert.equal(called.options.body.toString(), '{"title":"Review","description":""}')
  assert.equal(res.statusCode, 201)
  assert.equal(res.headers.get('cache-control'), 'no-store')
})

test('proxy rejects missing configuration, unsafe origins, methods and oversized bodies', async () => {
  const cases = [
    { req: { method: 'GET', url: '/api/auth/status', headers: {} }, env: {} , status: 503 },
    { req: { method: 'GET', url: '/api/auth/status', headers: {} }, env: { ...configured, CODEXPT_RAILWAY_ORIGIN: 'https://evil.example' }, status: 503 },
    { req: { method: 'GET', url: '/api/auth/status', headers: {} }, env: { ...configured, CODEXPT_RAILWAY_ORIGIN: `${configured.CODEXPT_RAILWAY_ORIGIN}/api` }, status: 503 },
    { req: { method: 'GET', url: '/api/auth/status', headers: {} }, env: { ...configured, CODEXPT_RAILWAY_ORIGIN: `${configured.CODEXPT_RAILWAY_ORIGIN}:443` }, status: 503 },
    { req: { method: 'GET', url: '/api/auth/status', headers: {} }, env: { ...configured, CODEXPT_RAILWAY_HOST: 'other-service.up.railway.app' }, status: 503 },
    { req: { method: 'GET', url: '/api/auth/status', headers: {} }, env: { ...configured, CODEXPT_RAILWAY_ORIGIN: 'https://other-service.up.railway.app' }, status: 503 },
    { req: { method: 'PUT', url: '/api/tasks/1', headers: {} }, env: configured, status: 405 },
    { req: { method: 'POST', url: '/api/tasks', headers: { 'content-length': String(32 * 1024 + 1) }, body: 'x'.repeat(32 * 1024 + 1) }, env: configured, status: 413 },
  ]
  for (const item of cases) {
    const res = new MockResponse()
    await proxyHandler(item.req, res, { env: item.env, fetchImpl: async () => { throw new Error('unexpected proxy request') } })
    assert.equal(res.statusCode, item.status)
    assert.equal(res.headers.get('cache-control'), 'no-store')
  }
})

test('proxy maps backend failure to a generic response without leaking origin or secret', async () => {
  const res = new MockResponse()
  await proxyHandler(
    { method: 'GET', url: '/api/auth/status', headers: {} },
    res,
    { env: configured, fetchImpl: async () => { throw new Error('upstream secret detail') } },
  )
  assert.equal(res.statusCode, 502)
  assert.equal(res.body.toString(), '{"error":"backend unavailable"}')
  assert.equal(res.body.includes(Buffer.from(configured.CODEXPT_PROXY_SECRET)), false)
})

test('proxy rejects oversized backend responses before forwarding headers or cookies', async () => {
  const res = new MockResponse()
  await proxyHandler(
    { method: 'GET', url: '/api/tasks', headers: {} },
    res,
    {
      env: configured,
      fetchImpl: async () => new Response('x'.repeat(4 * 1024 * 1024 + 1), {
        status: 200,
        headers: { 'set-cookie': '__Host-codexpt_session=should-not-forward; Secure; HttpOnly' },
      }),
    },
  )
  assert.equal(res.statusCode, 502)
  assert.equal(res.headers.has('set-cookie'), false)
  assert.equal(res.headers.has('location'), false)
  assert.equal(res.body.toString(), '{"error":"backend response is too large"}')
})

test('proxy maps upstream and response-stream timeouts to controlled 504 errors', async () => {
  const timeoutError = Object.assign(new Error('upstream timeout'), { name: 'TimeoutError' })
  const cases = [
    async () => { throw timeoutError },
    async () => new Response(new ReadableStream({ start(controller) { controller.error(timeoutError) } }), { status: 200 }),
  ]

  for (const fetchImpl of cases) {
    const res = new MockResponse()
    await proxyHandler(
      { method: 'GET', url: '/api/tasks', headers: {} },
      res,
      { env: configured, fetchImpl },
    )
    assert.equal(res.statusCode, 504)
    assert.equal(res.body.toString(), '{"error":"backend timed out"}')
  }
})

test('unknown /api root paths stay JSON 404 responses instead of SPA HTML', () => {
  for (const url of ['/api', '/api/']) {
    const res = new MockResponse()
    apiRootHandler({ method: 'GET', url }, res)
    assert.equal(res.statusCode, 404)
    assert.equal(res.headers.get('content-type'), 'application/json; charset=utf-8')
    assert.equal(res.headers.get('cache-control'), 'no-store')
    assert.equal(res.body.toString(), '{"error":"not found"}')
  }
})

test('Vercel API entrypoint preserves nested API paths and original query parameters', async () => {
  let forwardedUrl
  const res = new MockResponse()
  await apiRootHandler(
    { method: 'GET', url: '/api?__codexpt_path=auth/status&source=login', headers: {} },
    res,
    {
      env: configured,
      fetchImpl: async (url) => {
        forwardedUrl = String(url)
        return Response.json({ configured: false })
      },
    },
  )
  assert.equal(forwardedUrl, 'https://codexpt-production.up.railway.app/api/auth/status?source=login')
  assert.equal(res.statusCode, 200)
  assert.equal(res.body.toString(), '{"configured":false}')
})
