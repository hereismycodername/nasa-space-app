export function apiOrigin(value = 'http://127.0.0.1:8080') {
  let url
  try { url = new URL(value) } catch { throw new Error('API_URL must be an http(s) URL, e.g. http://127.0.0.1:8080') }
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.search || url.hash
    || !['', '/', '/api', '/api/'].includes(url.pathname)) {
    throw new Error('API_URL must be an http(s) server origin, optionally ending in /api; no credentials, query or other path')
  }
  return url.origin
}

function integer(env, name, fallback, minimum, maximum) {
  const raw = env[name] ?? String(fallback)
  const value = Number(raw)
  if (!/^\d+$/.test(raw) || !Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`${name} must be an integer in ${minimum}..${maximum}`)
  }
  return value
}

export function simulatorConfig(env = process.env) {
  const interval = integer(env, 'SIM_INTERVAL_MS', 2000, 100, 2_147_483_647)
  const profile = env.SIM_PROFILE ?? 'normal'
  if (!['normal', 'attention'].includes(profile)) throw new Error('SIM_PROFILE must be normal or attention (synthetic test data)')
  return {
    base: apiOrigin(env.API_URL), interval, profile,
    count: integer(env, 'SIM_COUNT', 0, 0, Number.MAX_SAFE_INTEGER),
    retry: integer(env, 'SIM_RETRY_MS', interval, 100, 2_147_483_647),
    timeout: integer(env, 'SIM_REQUEST_TIMEOUT_MS', 5000, 100, 60_000),
  }
}
