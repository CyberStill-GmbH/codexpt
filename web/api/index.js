import { proxyHandler } from './[...path].js'

export default function handler(req, res, dependencies) {
  const incoming = new URL(req.url ?? '/', 'https://vercel.invalid')
  const apiPath = incoming.searchParams.get('__codexpt_path')
  if (!apiPath) {
    res.statusCode = 404
    res.setHeader('Content-Type', 'application/json; charset=utf-8')
    res.setHeader('Cache-Control', 'no-store')
    return res.end(JSON.stringify({ error: 'not found' }))
  }
  incoming.searchParams.delete('__codexpt_path')
  req.url = `/api/${apiPath}${incoming.search}`
  return proxyHandler(req, res, dependencies)
}
