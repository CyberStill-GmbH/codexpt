export interface Env {
  DB: D1Database;
  PROXY_SECRET: string;
  API_HOST: string;
  WEB_ORIGIN: string;
  OIDC_ISSUER?: string;
  OIDC_CLIENT_ID?: string;
  OIDC_REDIRECT_URI?: string;
  LOGIN_SUCCESS_URL?: string;
  BOOTSTRAP_EMAIL?: string;
  BOOTSTRAP_WORKSPACE_NAME?: string;
}

const INTERNAL_PROXY_HEADER = "x-codexpt-proxy-key";
const MAX_BODY_BYTES = 32 * 1024;
const ALLOWED_METHODS = new Set(["GET", "HEAD", "POST", "PATCH", "DELETE", "OPTIONS"]);

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    const method = request.method.toUpperCase();
    const path = url.pathname;

    if (!ALLOWED_METHODS.has(method)) {
      return json({ error: "method not allowed" }, 405, { Allow: [...ALLOWED_METHODS].join(", ") });
    }
    if (path !== "/api/health") {
      const proxyKey = request.headers.get(INTERNAL_PROXY_HEADER);
      if (!constantTimeEqual(proxyKey, env.PROXY_SECRET) || !env.PROXY_SECRET || env.PROXY_SECRET.length < 32) {
        return json({ error: "trusted application proxy required" }, 403);
      }
      if (url.host !== env.API_HOST) return json({ error: "invalid API host" }, 403);
      if (["POST", "PATCH", "DELETE"].includes(method)) {
        if (request.headers.get("origin") !== env.WEB_ORIGIN) {
          return json({ error: "request origin is not allowed" }, 403);
        }
        if (!isJson(request.headers.get("content-type"))) {
          return json({ error: "application/json is required" }, 415);
        }
        if (!await bodyWithinLimit(request, MAX_BODY_BYTES)) {
          return json({ error: "request body is too large" }, 413);
        }
        const sessionToken = cookieValue(request.headers, SESSION_COOKIE);
        const csrfCookie = cookieValue(request.headers, CSRF_COOKIE);
        const csrfHeader = request.headers.get("x-csrf-token");
        if (!sessionToken || !csrfCookie || !csrfHeader
          || !constantTimeEqual(csrfCookie, csrfHeader)
          || !constantTimeEqual(csrfCookie, await csrfToken(sessionToken))) {
          return json({ error: "CSRF validation failed" }, 403);
        }
      }
    }

    if (path === "/api/health" && method === "GET") {
      return json({ status: "ok" });
    }
    if (path === "/api/health/ready" && method === "GET") {
      const oidcReady = Boolean(
        env.OIDC_ISSUER && env.OIDC_CLIENT_ID && env.OIDC_REDIRECT_URI && env.LOGIN_SUCCESS_URL,
      );
      if (!env.DB || !oidcReady || !isValidHostedConfig(env)) {
        return json({ error: "service is not ready" }, 503);
      }
      try {
        await env.DB.prepare("SELECT 1").first();
      } catch {
        return json({ error: "service is not ready" }, 503);
      }
      return json({ status: "ready", mode: "hosted" });
    }

    if (path === "/api/auth/status" && method === "GET") {
      const configured = Boolean(oidcConfig(env) && isValidHostedConfig(env));
      return json({
        configured,
        provider: providerLabel(env.OIDC_ISSUER),
      });
    }
    if (path === "/api/auth/login" && method === "GET") return startLogin(request, env);
    if (path === "/api/auth/callback" && method === "GET") return finishLogin(request, env, url);
    if (path === "/api/auth/me" && method === "GET") return currentSession(request, env);
    if (path === "/api/auth/logout" && method === "POST") return endSession(request, env);

    if (path.startsWith("/api/workspaces") || path.startsWith("/api/tasks") || path === "/api/invites/accept") {
      try {
        return await routeProductApi(request, env, url);
      } catch {
        return json({ error: "request could not be completed" }, 500);
      }
    }

    return json({ error: "not found" }, 404);
  },
};

function json(value: unknown, status = 200, headers: Record<string, string> = {}): Response {
  const responseHeaders = new Headers({
    "content-type": "application/json; charset=utf-8",
    "cache-control": "no-store",
    "x-content-type-options": "nosniff",
    "x-frame-options": "DENY",
    "referrer-policy": "no-referrer",
    "permissions-policy": "camera=(), microphone=(), geolocation=()",
    "content-security-policy": "default-src 'none'; frame-ancestors 'none'; base-uri 'none'",
    ...headers,
  });
  return new Response(JSON.stringify(value), { status, headers: responseHeaders });
}

function isJson(value: string | null): boolean {
  return value?.split(";", 1)[0]?.trim().toLowerCase() === "application/json";
}

function constantTimeEqual(left: string | null, right: string | undefined): boolean {
  if (left === null || right === undefined || left.length !== right.length) return false;
  let difference = 0;
  for (let i = 0; i < left.length; i += 1) difference |= left.charCodeAt(i) ^ right.charCodeAt(i);
  return difference === 0;
}

function isValidHostedConfig(env: Env): boolean {
  try {
    const origin = new URL(env.WEB_ORIGIN);
    const callback = new URL(env.OIDC_REDIRECT_URI ?? "");
    const success = new URL(env.LOGIN_SUCCESS_URL ?? "");
    return origin.protocol === "https:" && origin.origin === env.WEB_ORIGIN
      && callback.href === `${origin.origin}/api/auth/callback`
      && success.origin === origin.origin
      && success.pathname === "/"
      && success.search === ""
      && success.hash === ""
      && isValidIssuer(env.OIDC_ISSUER);
  } catch {
    return false;
  }
}

const SESSION_COOKIE = "__Host-codexpt_session";
const CSRF_COOKIE = "__Host-codexpt_csrf";
const OIDC_COOKIE = "__Host-codexpt_oidc";
const SESSION_IDLE_MS = 30 * 60 * 1000;
const SESSION_MAX_MS = 12 * 60 * 60 * 1000;
const AUTH_TRANSACTION_MS = 10 * 60 * 1000;
const ID_TOKEN_MAX_BYTES = 64 * 1024;

interface OidcConfig {
  issuer: string;
  clientId: string;
  redirectUri: string;
  successUrl: string;
}

interface OidcMetadata {
  issuer: string;
  authorization_endpoint: string;
  token_endpoint: string;
  jwks_uri: string;
}

interface IdTokenClaims {
  iss?: unknown;
  sub?: unknown;
  aud?: unknown;
  exp?: unknown;
  nbf?: unknown;
  iat?: unknown;
  nonce?: unknown;
  azp?: unknown;
  email?: unknown;
  email_verified?: unknown;
}

interface JwkSet { keys?: Array<JsonWebKey & { kid?: string; use?: string; kty?: string }> }

async function startLogin(request: Request, env: Env): Promise<Response> {
  const config = oidcConfig(env);
  if (!config || !isValidHostedConfig(env)) return json({ error: "La conexión de identidad todavía no está configurada." }, 503);
  try {
    const metadata = await discoverProvider(config.issuer);
    const state = randomToken();
    const nonce = randomToken();
    const verifier = randomToken() + randomToken();
    const challenge = base64Url(await digest(new TextEncoder().encode(verifier)));
    const now = new Date();
    await env.DB.prepare(
      `INSERT INTO auth_transactions(state_hash, nonce, pkce_verifier, created_at, expires_at)
       VALUES (?, ?, ?, ?, ?)`,
    ).bind(
      await sha256Hex(state), nonce, verifier, now.toISOString(), new Date(now.getTime() + AUTH_TRANSACTION_MS).toISOString(),
    ).run();
    const authorize = new URL(metadata.authorization_endpoint);
    authorize.searchParams.set("response_type", "code");
    authorize.searchParams.set("client_id", config.clientId);
    authorize.searchParams.set("redirect_uri", config.redirectUri);
    authorize.searchParams.set("scope", "openid profile email");
    authorize.searchParams.set("state", state);
    authorize.searchParams.set("nonce", nonce);
    authorize.searchParams.set("code_challenge", challenge);
    authorize.searchParams.set("code_challenge_method", "S256");
    const response = new Response(null, { status: 303, headers: { location: authorize.toString() } });
    response.headers.append("set-cookie", cookie(OIDC_COOKIE, state, { httpOnly: true, maxAge: 600 }));
    return withSecurityHeaders(response);
  } catch {
    return json({ error: "identity provider unavailable" }, 503);
  }
}

async function finishLogin(request: Request, env: Env, url: URL): Promise<Response> {
  const config = oidcConfig(env);
  if (!config || !isValidHostedConfig(env)) return json({ error: "identity provider unavailable" }, 503);
  const state = url.searchParams.get("state");
  const code = url.searchParams.get("code");
  if (!state || state.length > 256 || cookieValue(request.headers, OIDC_COOKIE) !== state) {
    return json({ error: "invalid sign-in transaction" }, 403);
  }
  const error = url.searchParams.get("error");
  const now = new Date().toISOString();
  const transaction = await env.DB.prepare(
    `UPDATE auth_transactions SET consumed_at = ?
     WHERE state_hash = ? AND consumed_at IS NULL AND expires_at > ?
     RETURNING nonce, pkce_verifier`,
  ).bind(now, await sha256Hex(state), now).first<{ nonce: string; pkce_verifier: string }>();
  if (!transaction) return json({ error: "expired sign-in transaction" }, 403);
  if (error) return clearOidcAndRedirect(config.successUrl, null);
  if (!code || code.length > 4096) return json({ error: "invalid sign-in response" }, 400);

  try {
    const metadata = await discoverProvider(config.issuer);
    const tokenResponse = await fetchBounded(metadata.token_endpoint, {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded", accept: "application/json" },
      body: new URLSearchParams({
        grant_type: "authorization_code",
        code,
        client_id: config.clientId,
        redirect_uri: config.redirectUri,
        code_verifier: transaction.pkce_verifier,
      }),
    }, 8_000, 64 * 1024);
    if (!tokenResponse.ok) return json({ error: "identity verification failed" }, 401);
    const tokens = await tokenResponse.json() as { id_token?: unknown };
    if (typeof tokens.id_token !== "string" || tokens.id_token.length > ID_TOKEN_MAX_BYTES) {
      return json({ error: "identity verification failed" }, 401);
    }
    const claims = await verifyIdToken(tokens.id_token, config, metadata, transaction.nonce);
    const user = await resolveIdentity(env, config, claims);
    if (!user) return json({ error: "This identity is not invited to CodeXPT." }, 403);
    return createSessionResponse(env, user.user_id, config.successUrl);
  } catch {
    return json({ error: "identity verification failed" }, 401);
  }
}

async function currentSession(request: Request, env: Env): Promise<Response> {
  const raw = cookieValue(request.headers, SESSION_COOKIE);
  if (!raw || raw.length > 128) return json({ error: "sign in required" }, 401);
  const now = new Date();
  const session = await env.DB.prepare(
    `SELECT s.session_id, s.user_id, s.absolute_expires_at, u.email
     FROM auth_sessions s JOIN identity_users u ON u.user_id = s.user_id
     WHERE s.token_hash = ? AND s.revoked_at IS NULL AND u.disabled_at IS NULL
       AND s.idle_expires_at > ? AND s.absolute_expires_at > ?`,
  ).bind(await sha256Hex(raw), now.toISOString(), now.toISOString())
    .first<{ session_id: string; user_id: string; absolute_expires_at: string; email: string | null }>();
  if (!session) return json({ error: "sign in required" }, 401);
  const idleExpires = new Date(Math.min(now.getTime() + SESSION_IDLE_MS, Date.parse(session.absolute_expires_at))).toISOString();
  const refreshed = await env.DB.prepare("UPDATE auth_sessions SET last_seen_at = ?, idle_expires_at = ? WHERE session_id = ? AND revoked_at IS NULL")
    .bind(now.toISOString(), idleExpires, session.session_id).run();
  if (refreshed.meta.changes !== 1) return json({ error: "sign in required" }, 401);
  return json({ user_id: session.user_id, email: session.email, csrf_token: await csrfToken(raw) });
}

async function endSession(request: Request, env: Env): Promise<Response> {
  const raw = cookieValue(request.headers, SESSION_COOKIE);
  if (raw) {
    await env.DB.prepare("UPDATE auth_sessions SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL")
      .bind(new Date().toISOString(), await sha256Hex(raw)).run();
  }
  const response = new Response(null, { status: 204 });
  response.headers.append("set-cookie", cookie(SESSION_COOKIE, "", { httpOnly: true, maxAge: 0 }));
  response.headers.append("set-cookie", cookie(CSRF_COOKIE, "", { maxAge: 0 }));
  return withSecurityHeaders(response);
}

async function createSessionResponse(env: Env, userId: string, target: string): Promise<Response> {
  const sessionId = crypto.randomUUID();
  const raw = randomToken();
  const now = new Date();
  const createdAt = now.toISOString();
  const absoluteExpiry = new Date(now.getTime() + SESSION_MAX_MS).toISOString();
  const idleExpiry = new Date(now.getTime() + SESSION_IDLE_MS).toISOString();
  await env.DB.batch([
    env.DB.prepare(
      `INSERT INTO auth_sessions(session_id, user_id, token_hash, created_at, last_seen_at, idle_expires_at, absolute_expires_at)
       VALUES (?, ?, ?, ?, ?, ?, ?)`
    ).bind(sessionId, userId, await sha256Hex(raw), createdAt, createdAt, idleExpiry, absoluteExpiry),
    env.DB.prepare(
      `INSERT INTO identity_audit_events(event_id, occurred_at, actor_user_id, actor_session_id,
       action, resource_type, resource_id, result, correlation_id, details_json)
       VALUES (?, ?, ?, ?, 'auth.login', 'auth_session', ?, 'allowed', ?, '{}')`
    ).bind(crypto.randomUUID(), createdAt, userId, sessionId, sessionId, crypto.randomUUID()),
  ]);
  const response = new Response(null, { status: 303, headers: { location: target } });
  response.headers.append("set-cookie", cookie(OIDC_COOKIE, "", { httpOnly: true, maxAge: 0 }));
  response.headers.append("set-cookie", cookie(CSRF_COOKIE, await csrfToken(raw), { maxAge: 12 * 60 * 60 }));
  response.headers.append("set-cookie", cookie(SESSION_COOKIE, raw, { httpOnly: true, maxAge: 12 * 60 * 60 }));
  return withSecurityHeaders(response);
}

async function clearOidcAndRedirect(target: string, session: null): Promise<Response> {
  void session;
  const response = new Response(null, { status: 303, headers: { location: target } });
  response.headers.append("set-cookie", cookie(OIDC_COOKIE, "", { httpOnly: true, maxAge: 0 }));
  return withSecurityHeaders(response);
}

async function resolveIdentity(env: Env, config: OidcConfig, claims: IdTokenClaims): Promise<{ user_id: string } | null> {
  const subject = claims.sub as string;
  const email = typeof claims.email === "string" ? claims.email.trim() : null;
  const verified = claims.email_verified === true;
  const existing = await env.DB.prepare(
    "SELECT user_id, disabled_at FROM identity_users WHERE issuer = ? AND subject = ?",
  ).bind(config.issuer, subject).first<{ user_id: string; disabled_at: string | null }>();
  if (existing) {
    if (existing.disabled_at) return null;
    await env.DB.prepare("UPDATE identity_users SET email = ?, email_verified = ? WHERE user_id = ?")
      .bind(email, verified ? 1 : 0, existing.user_id).run();
    return { user_id: existing.user_id };
  }
  if (!email || !verified || email.length > 320) return null;
  const normalized = email.toLowerCase();
  const now = new Date().toISOString();
  const invite = await env.DB.prepare(
    `SELECT invite_id, workspace_id, role FROM workspace_invites
     WHERE email_normalized = ? AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > ?
     ORDER BY created_at ASC LIMIT 1`,
  ).bind(normalized, now).first<{ invite_id: string; workspace_id: string; role: "EDITOR" | "VIEWER" }>();
  const userId = crypto.randomUUID();

  if (!invite && env.BOOTSTRAP_EMAIL?.trim().toLowerCase() === normalized && env.BOOTSTRAP_WORKSPACE_NAME?.trim()) {
    const workspaceId = crypto.randomUUID();
    const eventId = crypto.randomUUID();
    try {
      await env.DB.batch([
        env.DB.prepare("INSERT INTO identity_bootstrap(singleton, completed_at) VALUES (1, ?)").bind(now),
        env.DB.prepare("INSERT INTO identity_users(user_id, issuer, subject, email, email_verified, created_at) VALUES (?, ?, ?, ?, 1, ?)")
          .bind(userId, config.issuer, subject, normalized, now),
        env.DB.prepare("INSERT INTO workspaces(workspace_id, name, created_at) VALUES (?, ?, ?)")
          .bind(workspaceId, env.BOOTSTRAP_WORKSPACE_NAME.trim().slice(0, 120), now),
        env.DB.prepare("INSERT INTO workspace_memberships(workspace_id, user_id, role, created_at, role_updated_at) VALUES (?, ?, 'OWNER', ?, ?)")
          .bind(workspaceId, userId, now, now),
        env.DB.prepare(`INSERT INTO identity_audit_events(event_id, occurred_at, actor_user_id, workspace_id,
          action, resource_type, resource_id, result, correlation_id, details_json)
          VALUES (?, ?, ?, ?, 'identity.bootstrap.owner', 'workspace_membership', ?, 'allowed', ?, '{}')`)
          .bind(eventId, now, userId, workspaceId, userId, crypto.randomUUID()),
      ]);
      return { user_id: userId };
    } catch {
      return null;
    }
  }
  if (!invite) return null;
  try {
    const accepted = await env.DB.batch([
      env.DB.prepare("INSERT INTO identity_users(user_id, issuer, subject, email, email_verified, created_at) VALUES (?, ?, ?, ?, 1, ?)")
        .bind(userId, config.issuer, subject, normalized, now),
      env.DB.prepare(`UPDATE workspace_invites SET accepted_at = ?, accepted_by = ?
        WHERE invite_id = ? AND email_normalized = ? AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > ?`)
        .bind(now, userId, invite.invite_id, normalized, now),
      env.DB.prepare(`INSERT INTO identity_audit_events(event_id, occurred_at, actor_user_id, workspace_id,
        action, resource_type, resource_id, result, correlation_id, details_json)
        SELECT ?, ?, ?, ?, 'member.invite.accept', 'workspace_membership', ?, 'allowed', ?, '{}'
        WHERE EXISTS(SELECT 1 FROM workspace_invites WHERE invite_id = ? AND accepted_by = ? AND accepted_at = ?)`)
        .bind(crypto.randomUUID(), now, userId, invite.workspace_id, userId, crypto.randomUUID(), invite.invite_id, userId, now),
      env.DB.prepare(`DELETE FROM identity_users WHERE user_id = ? AND NOT EXISTS (
        SELECT 1 FROM workspace_memberships WHERE user_id = ? AND revoked_at IS NULL
      )`).bind(userId, userId),
    ]);
    return accepted[1].meta.changes > 0 ? { user_id: userId } : null;
  } catch {
    return null;
  }
}

async function verifyIdToken(token: string, config: OidcConfig, metadata: OidcMetadata, expectedNonce: string): Promise<IdTokenClaims> {
  const parts = token.split(".");
  if (parts.length !== 3 || token.length > ID_TOKEN_MAX_BYTES) throw new Error("invalid token");
  const header = decodeJson<{ alg?: string; kid?: string }>(parts[0]);
  if (header.alg !== "RS256" || !header.kid) throw new Error("unsupported ID token algorithm");
  const jwksResponse = await fetchBounded(metadata.jwks_uri, { headers: { accept: "application/json" } }, 8_000, 1024 * 1024);
  if (!jwksResponse.ok) throw new Error("JWKS unavailable");
  const jwks = await jwksResponse.json() as JwkSet;
  const jwk = jwks.keys?.find((key) => key.kid === header.kid && key.kty === "RSA" && (!key.use || key.use === "sig"));
  if (!jwk) throw new Error("signing key not found");
  const publicKey = await crypto.subtle.importKey("jwk", jwk as JsonWebKey, { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, ["verify"]);
  const signature = Uint8Array.from(base64UrlDecode(parts[2])).buffer;
  const signingInput = new TextEncoder().encode(`${parts[0]}.${parts[1]}`).buffer;
  const validSignature = await crypto.subtle.verify(
    "RSASSA-PKCS1-v1_5", publicKey, signature, signingInput,
  );
  if (!validSignature) throw new Error("invalid signature");
  const claims = decodeJson<IdTokenClaims>(parts[1]);
  const audience = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
  const now = Math.floor(Date.now() / 1000);
  if (claims.iss !== config.issuer || typeof claims.sub !== "string" || claims.sub.length < 1 || claims.sub.length > 255
    || !audience.includes(config.clientId) || (audience.length > 1 && claims.azp !== config.clientId)
    || typeof claims.exp !== "number" || claims.exp < now
    || (typeof claims.nbf === "number" && claims.nbf > now)
    || (typeof claims.iat === "number" && claims.iat > now + 60)
    || claims.nonce !== expectedNonce) throw new Error("ID token claims invalid");
  return claims;
}

async function discoverProvider(issuer: string): Promise<OidcMetadata> {
  if (!isValidIssuer(issuer)) throw new Error("invalid configured issuer");
  const discoveryUrl = `${issuer.replace(/\/$/, "")}/.well-known/openid-configuration`;
  const response = await fetchBounded(discoveryUrl, { headers: { accept: "application/json" } }, 8_000, 64 * 1024);
  if (!response.ok) throw new Error("OIDC discovery failed");
  const metadata = await response.json() as OidcMetadata;
  if (metadata.issuer !== issuer) throw new Error("issuer mismatch");
  for (const endpoint of [metadata.authorization_endpoint, metadata.token_endpoint, metadata.jwks_uri]) {
    const parsed = new URL(endpoint);
    if (parsed.protocol !== "https:") throw new Error("OIDC endpoint must use HTTPS");
  }
  return metadata;
}

async function fetchBounded(input: string, init: RequestInit, timeoutMs: number, maxBytes: number): Promise<Response> {
  const response = await fetch(input, { ...init, redirect: "manual", signal: AbortSignal.timeout(timeoutMs) });
  const declaredLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > maxBytes) throw new Error("upstream response too large");
  const body = await response.arrayBuffer();
  if (body.byteLength > maxBytes) throw new Error("upstream response too large");
  return new Response(body, { status: response.status, headers: response.headers });
}

function oidcConfig(env: Env): OidcConfig | null {
  if (!env.OIDC_ISSUER || !env.OIDC_CLIENT_ID || !env.OIDC_REDIRECT_URI || !env.LOGIN_SUCCESS_URL || !isValidIssuer(env.OIDC_ISSUER)) return null;
  return { issuer: env.OIDC_ISSUER, clientId: env.OIDC_CLIENT_ID, redirectUri: env.OIDC_REDIRECT_URI, successUrl: env.LOGIN_SUCCESS_URL };
}

function isValidIssuer(value: string | undefined): boolean {
  if (!value) return false;
  try {
    const url = new URL(value);
    return url.protocol === "https:" && url.origin === value && !url.username && !url.password && !url.search && !url.hash;
  } catch { return false; }
}

function providerLabel(issuer: string | undefined): string {
  if (issuer === "https://auth.openai.com") return "ChatGPT";
  if (issuer?.startsWith("https://cognito-idp.") && issuer.includes(".amazonaws.com/")) return "Amazon Cognito";
  return "proveedor de identidad";
}

function randomToken(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return base64Url(bytes);
}

async function digest(bytes: BufferSource): Promise<Uint8Array> {
  return new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
}

async function sha256Hex(value: string): Promise<string> {
  return [...await digest(new TextEncoder().encode(value))].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function csrfToken(sessionToken: string): Promise<string> {
  const value = `codexpt-csrf-v1:${sessionToken}`;
  return base64Url(await digest(new TextEncoder().encode(value)));
}

async function bodyWithinLimit(request: Request, maxBytes: number): Promise<boolean> {
  const declaredLength = Number(request.headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > maxBytes) return false;
  const reader = request.clone().body?.getReader();
  if (!reader) return true;
  let total = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) return true;
      total += value.byteLength;
      if (total > maxBytes) {
        await reader.cancel();
        return false;
      }
    }
  } finally {
    reader.releaseLock();
  }
}

function base64Url(value: Uint8Array): string {
  let binary = "";
  for (const byte of value) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

function base64UrlDecode(value: string): Uint8Array {
  const padded = value.replaceAll("-", "+").replaceAll("_", "/") + "===".slice((value.length + 3) % 4);
  return Uint8Array.from(atob(padded), (character) => character.charCodeAt(0));
}

function decodeJson<T>(value: string): T {
  return JSON.parse(new TextDecoder().decode(base64UrlDecode(value))) as T;
}

function cookieValue(headers: Headers, name: string): string | null {
  for (const item of (headers.get("cookie") ?? "").split(";")) {
    const separator = item.indexOf("=");
    if (separator >= 0 && item.slice(0, separator).trim() === name) return item.slice(separator + 1).trim();
  }
  return null;
}

function cookie(name: string, value: string, options: { httpOnly?: boolean; maxAge: number }): string {
  return `${name}=${value}; Path=/; SameSite=Lax; Max-Age=${options.maxAge}; Secure${options.httpOnly ? "; HttpOnly" : ""}`;
}

function withSecurityHeaders(response: Response): Response {
  response.headers.set("cache-control", "no-store");
  response.headers.set("x-content-type-options", "nosniff");
  response.headers.set("x-frame-options", "DENY");
  response.headers.set("referrer-policy", "no-referrer");
  response.headers.set("permissions-policy", "camera=(), microphone=(), geolocation=()");
  response.headers.set("content-security-policy", "default-src 'none'; frame-ancestors 'none'; base-uri 'none'");
  return response;
}

interface Principal {
  session_id: string;
  user_id: string;
  email: string | null;
  issuer: string;
  subject: string;
}

interface WorkspaceAccess extends Principal {
  workspaceId: string;
  role: "OWNER" | "EDITOR" | "VIEWER";
}

interface TaskRecord {
  task_id: string;
  title: string;
  description: string;
  status: "open" | "proposed" | "under_review" | "resolved" | "approved";
  pending_decision: number;
  version: number;
  created_at: string;
  updated_at: string;
}

async function routeProductApi(request: Request, env: Env, url: URL): Promise<Response> {
  const path = url.pathname;
  const method = request.method.toUpperCase();
  if (path === "/api/workspaces" && method === "GET") {
    const principal = await requirePrincipal(request, env);
    if (principal instanceof Response) return principal;
    const rows = await env.DB.prepare(
      `SELECT w.workspace_id, w.name, m.role FROM workspaces w
       JOIN workspace_memberships m ON m.workspace_id = w.workspace_id
       WHERE m.user_id = ? AND m.revoked_at IS NULL ORDER BY w.created_at, w.workspace_id`,
    ).bind(principal.user_id).all<{ workspace_id: string; name: string; role: string }>();
    return json(rows.results);
  }
  if (path === "/api/invites/accept" && method === "POST") return acceptInvite(request, env);
  const membersRoute = path.match(/^\/api\/workspaces\/([^/]+)\/members(?:\/([^/]+))?$/);
  if (membersRoute) {
    const workspaceId = decodePathPart(membersRoute[1]);
    const targetUserId = membersRoute[2] ? decodePathPart(membersRoute[2]) : null;
    if (!workspaceId || (membersRoute[2] && !targetUserId)) return json({ error: "invalid workspace member path" }, 400);
    const write = method !== "GET";
    if (request.headers.get("x-codexpt-workspace-id") !== workspaceId) return json({ error: "workspace not found" }, 404);
    const access = await requireWorkspaceAccess(request, env, workspaceId, write, write);
    if (access instanceof Response) return access;
    if (!targetUserId && method === "GET") {
      const members = await env.DB.prepare(`SELECT m.user_id, u.email, m.role FROM workspace_memberships m
        JOIN identity_users u ON u.user_id = m.user_id
        WHERE m.workspace_id = ? AND m.revoked_at IS NULL ORDER BY m.created_at, m.user_id`)
        .bind(workspaceId).all<{ user_id: string; email: string | null; role: string }>();
      return json(members.results);
    }
    if (targetUserId && method === "PATCH") return changeMemberRole(request, env, access, targetUserId);
    if (targetUserId && method === "DELETE") return revokeMember(env, access, targetUserId);
  }
  const invitesRoute = path.match(/^\/api\/workspaces\/([^/]+)\/invites$/);
  if (invitesRoute && method === "POST") {
    const workspaceId = decodePathPart(invitesRoute[1]);
    if (!workspaceId) return json({ error: "invalid workspace id" }, 400);
    if (request.headers.get("x-codexpt-workspace-id") !== workspaceId) return json({ error: "workspace not found" }, 404);
    const access = await requireWorkspaceAccess(request, env, workspaceId, true, true);
    if (access instanceof Response) return access;
    return createInvite(request, env, access);
  }
  const transferRoute = path.match(/^\/api\/workspaces\/([^/]+)\/transfer-ownership$/);
  if (transferRoute && method === "POST") {
    const workspaceId = decodePathPart(transferRoute[1]);
    if (!workspaceId) return json({ error: "invalid workspace id" }, 400);
    if (request.headers.get("x-codexpt-workspace-id") !== workspaceId) return json({ error: "workspace not found" }, 404);
    const access = await requireWorkspaceAccess(request, env, workspaceId, true, true);
    if (access instanceof Response) return access;
    return transferOwnership(request, env, access);
  }
  if (path === "/api/tasks" && (method === "GET" || method === "POST")) {
    const access = await requireWorkspaceAccess(request, env, request.headers.get("x-codexpt-workspace-id"), method !== "GET");
    if (access instanceof Response) return access;
    if (method === "GET") {
      const rows = await env.DB.prepare(
        `SELECT t.task_id, t.title, t.description, t.status,
          EXISTS(SELECT 1 FROM human_decisions d WHERE d.task_id = t.task_id AND d.status = 'pending') AS pending_decision,
          t.version, t.created_at, t.updated_at
         FROM tasks t WHERE t.workspace_id = ? ORDER BY t.updated_at DESC, t.task_id ASC`,
      ).bind(access.workspaceId).all<TaskRecord>();
      return json(rows.results.map(taskDto));
    }
    const input = await jsonBody<{ title?: unknown; description?: unknown }>(request);
    if (input instanceof Response) return input;
    if (typeof input.title !== "string" || typeof input.description !== "string"
      || !input.title.trim() || [...input.title].length > 160 || [...input.description].length > 4_000) {
      return json({ error: "invalid task input" }, 400);
    }
    const now = new Date().toISOString();
    const taskId = crypto.randomUUID();
    const correlation = crypto.randomUUID();
    const mutationId = crypto.randomUUID();
    const actor = `human:${access.user_id}`;
    const results = await env.DB.batch([
      env.DB.prepare(
        `INSERT INTO tasks(task_id, title, description, status, created_at, updated_at, workspace_id, version, last_mutation_id)
         SELECT ?, ?, ?, 'open', ?, ?, ?, 1, ? WHERE EXISTS (
           SELECT 1 FROM workspace_memberships m JOIN auth_sessions s ON s.session_id = ?
           JOIN identity_users u ON u.user_id = m.user_id
           WHERE m.workspace_id = ? AND m.user_id = ? AND m.revoked_at IS NULL AND m.role IN ('OWNER','EDITOR')
             AND s.user_id = m.user_id AND s.revoked_at IS NULL AND s.idle_expires_at > ? AND s.absolute_expires_at > ?
             AND u.disabled_at IS NULL
         )`,
      ).bind(taskId, input.title.trim(), input.description, now, now, access.workspaceId, mutationId,
        access.session_id, access.workspaceId, access.user_id, now, now),
      env.DB.prepare(
        `INSERT INTO task_events(task_id, event_type, actor, correlation_id, details_json, occurred_at)
         SELECT ?, 'task_created', ?, ?, ?, ? WHERE EXISTS (
           SELECT 1 FROM tasks WHERE task_id = ? AND last_mutation_id = ?
         )`,
      ).bind(taskId, actor, correlation, JSON.stringify({ status: "open", verified_human: { user_id: access.user_id, session_id: access.session_id } }), now, taskId, mutationId),
    ]);
    if (results[0].meta.changes !== 1) return json({ error: "workspace access changed; reload and retry" }, 403);
    const task = await getTaskRecord(env, taskId, access.workspaceId);
    return task ? json({ data: taskDto(task) }, 201) : json({ error: "task creation failed" }, 500);
  }
  const taskMatch = path.match(/^\/api\/tasks\/([^/]+)$/);
  if (taskMatch && method === "GET") {
    const taskId = decodePathPart(taskMatch[1]);
    if (!taskId) return json({ error: "invalid task id" }, 400);
    const access = await requireWorkspaceAccess(request, env, request.headers.get("x-codexpt-workspace-id"), false);
    if (access instanceof Response) return access;
    const task = await getTaskRecord(env, taskId, access.workspaceId);
    if (!task) return json({ error: "task not found" }, 404);
    const [messages, events, pending] = await Promise.all([
      env.DB.prepare(`SELECT schema_version, message_id, task_id, sender, recipient, correlation_id, parent_id,
        status, body, evidence_refs_json, created_at FROM messages WHERE task_id = ? ORDER BY created_at, message_id`)
        .bind(taskId).all<{ schema_version: number; message_id: string; task_id: string; sender: string; recipient: string; correlation_id: string; parent_id: string | null; status: string; body: string; evidence_refs_json: string; created_at: string }>(),
      env.DB.prepare(`SELECT sequence, task_id, event_type, actor, correlation_id, details_json, occurred_at
        FROM task_events WHERE task_id = ? ORDER BY sequence`).bind(taskId)
        .all<{ sequence: number; task_id: string; event_type: string; actor: string; correlation_id: string; details_json: string; occurred_at: string }>(),
      env.DB.prepare(`SELECT decision_id, task_id, question, requested_by, status, created_at
        FROM human_decisions WHERE task_id = ? AND status = 'pending'`).bind(taskId)
        .first<Record<string, unknown>>(),
    ]);
    return json({
      task: taskDto(task),
      messages: messages.results.map(({ evidence_refs_json, ...message }) => ({ ...message, evidence_refs: parseJsonArray(evidence_refs_json) })),
      events: events.results.map(({ details_json, ...event }) => ({ ...event, details: parseJsonObject(details_json) })),
      pending_decision: pending ?? null,
    });
  }
  const taskSubroute = path.match(/^\/api\/tasks\/([^/]+)\/(messages|transition|decisions|checkpoints)(?:\/([^/]+)(?:\/(answer|resume))?)?$/);
  if (taskSubroute) {
    const taskId = decodePathPart(taskSubroute[1]);
    if (!taskId) return json({ error: "invalid task id" }, 400);
    const resource = taskSubroute[2];
    const childId = taskSubroute[3] ? decodePathPart(taskSubroute[3]) : null;
    const action = taskSubroute[4];
    const access = await requireWorkspaceAccess(request, env, request.headers.get("x-codexpt-workspace-id"), method !== "GET");
    if (access instanceof Response) return access;
    const task = await getTaskRecord(env, taskId, access.workspaceId);
    if (!task) return json({ error: "task not found" }, 404);
    if (resource === "messages" && method === "POST" && !childId) return createMessage(request, env, access, taskId);
    if (resource === "transition" && method === "POST" && !childId) return transitionTask(request, env, access, task);
    if (resource === "decisions" && method === "POST" && !childId) return requestDecision(request, env, access, task);
    if (resource === "decisions" && childId && action === "answer" && method === "POST") {
      return answerDecision(request, env, access, task, childId);
    }
    if (resource === "checkpoints" && method === "GET" && !childId) return listCheckpoints(env, taskId, access.workspaceId);
    if (resource === "checkpoints" && method === "POST" && !childId) return createCheckpoint(request, env, access, task);
    if (resource === "checkpoints" && childId && action === "resume" && method === "POST") {
      return resumeCheckpoint(env, access, taskId, childId);
    }
  }
  return json({ error: "not found" }, 404);
}

async function requirePrincipal(request: Request, env: Env): Promise<Principal | Response> {
  const raw = cookieValue(request.headers, SESSION_COOKIE);
  if (!raw || raw.length > 128) return json({ error: "sign in required" }, 401);
  const now = new Date();
  const principal = await env.DB.prepare(
    `SELECT s.session_id, u.user_id, u.email, u.issuer, u.subject, s.absolute_expires_at
     FROM auth_sessions s JOIN identity_users u ON u.user_id = s.user_id
     WHERE s.token_hash = ? AND s.revoked_at IS NULL AND u.disabled_at IS NULL
       AND s.idle_expires_at > ? AND s.absolute_expires_at > ?`,
  ).bind(await sha256Hex(raw), now.toISOString(), now.toISOString())
    .first<Principal & { absolute_expires_at: string }>();
  if (!principal) return json({ error: "sign in required" }, 401);
  const nextIdleExpiry = new Date(Math.min(now.getTime() + SESSION_IDLE_MS, Date.parse(principal.absolute_expires_at))).toISOString();
  const refreshed = await env.DB.prepare(
    `UPDATE auth_sessions SET last_seen_at = ?, idle_expires_at = ?
     WHERE session_id = ? AND revoked_at IS NULL AND absolute_expires_at > ?`,
  ).bind(now.toISOString(), nextIdleExpiry, principal.session_id, now.toISOString()).run();
  if (refreshed.meta.changes !== 1) return json({ error: "sign in required" }, 401);
  const { absolute_expires_at: _absoluteExpiry, ...identity } = principal;
  return identity;
}

async function requireWorkspaceAccess(
  request: Request, env: Env, workspaceId: string | null, write: boolean, ownerOnly = false,
): Promise<WorkspaceAccess | Response> {
  if (!workspaceId || workspaceId.length > 64) return json({ error: "workspace selection required" }, 400);
  const principal = await requirePrincipal(request, env);
  if (principal instanceof Response) return principal;
  const membership = await env.DB.prepare(
    `SELECT role FROM workspace_memberships WHERE workspace_id = ? AND user_id = ? AND revoked_at IS NULL`,
  ).bind(workspaceId, principal.user_id).first<{ role: "OWNER" | "EDITOR" | "VIEWER" }>();
  if (!membership) return json({ error: "workspace not found" }, 404);
  if (write && membership.role === "VIEWER") return json({ error: "workspace role cannot make changes" }, 403);
  if (ownerOnly && membership.role !== "OWNER") return json({ error: "workspace owner role required" }, 403);
  return { ...principal, workspaceId, role: membership.role };
}

async function jsonBody<T>(request: Request): Promise<T | Response> {
  try {
    const value: unknown = await request.json();
    if (!value || typeof value !== "object" || Array.isArray(value)) return json({ error: "invalid JSON body" }, 400);
    return value as T;
  } catch {
    return json({ error: "invalid JSON body" }, 400);
  }
}

async function getTaskRecord(env: Env, taskId: string, workspaceId: string): Promise<TaskRecord | null> {
  return env.DB.prepare(
    `SELECT t.task_id, t.title, t.description, t.status,
      EXISTS(SELECT 1 FROM human_decisions d WHERE d.task_id = t.task_id AND d.status = 'pending') AS pending_decision,
      t.version, t.created_at, t.updated_at
     FROM tasks t WHERE t.task_id = ? AND t.workspace_id = ?`,
  ).bind(taskId, workspaceId).first<TaskRecord>();
}

function taskDto(task: TaskRecord) {
  return { ...task, pending_decision: Boolean(task.pending_decision) };
}

function decodePathPart(value: string): string | null {
  try { return decodeURIComponent(value); } catch { return null; }
}

function parseJsonArray(value: string): unknown[] {
  const parsed: unknown = JSON.parse(value);
  return Array.isArray(parsed) ? parsed : [];
}

function parseJsonObject(value: string): Record<string, unknown> {
  const parsed: unknown = JSON.parse(value);
  return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed as Record<string, unknown> : {};
}

async function createMessage(request: Request, env: Env, access: WorkspaceAccess, taskId: string): Promise<Response> {
  const input = await jsonBody<{ recipient?: unknown; body?: unknown; evidence_refs?: unknown }>(request);
  if (input instanceof Response) return input;
  if (typeof input.recipient !== "string" || !input.recipient.trim() || [...input.recipient].length > 120
    || typeof input.body !== "string" || !input.body.trim() || [...input.body].length > 12_000) {
    return json({ error: "invalid message input" }, 400);
  }
  const evidenceRefs = input.evidence_refs ?? [];
  if (!Array.isArray(evidenceRefs) || evidenceRefs.length > 32 || !evidenceRefs.every(validEvidenceRef)) {
    return json({ error: "invalid evidence references" }, 400);
  }
  const createdAt = new Date().toISOString();
  const message = {
    schema_version: 1,
    message_id: crypto.randomUUID(),
    task_id: taskId,
    sender: `human:${access.user_id}`,
    recipient: input.recipient.trim(),
    correlation_id: crypto.randomUUID(),
    parent_id: null,
    status: "recorded",
    body: input.body,
    evidence_refs: evidenceRefs,
    created_at: createdAt,
  };
  const hashable = { ...message, created_at: "" };
  const payloadHash = await sha256Hex(JSON.stringify(hashable));
  const mutationId = crypto.randomUUID();
  const results = await env.DB.batch([
    env.DB.prepare(`UPDATE tasks SET updated_at = ?, version = version + 1, last_mutation_id = ?
      WHERE task_id = ? AND workspace_id = ? AND EXISTS (
        SELECT 1 FROM workspace_memberships m JOIN auth_sessions s ON s.session_id = ?
        JOIN identity_users u ON u.user_id = m.user_id
        WHERE m.workspace_id = ? AND m.user_id = ? AND m.revoked_at IS NULL AND m.role IN ('OWNER','EDITOR')
          AND s.user_id = m.user_id AND s.revoked_at IS NULL AND s.idle_expires_at > ? AND s.absolute_expires_at > ?
          AND u.disabled_at IS NULL
      )`)
      .bind(createdAt, mutationId, taskId, access.workspaceId, access.session_id, access.workspaceId, access.user_id, createdAt, createdAt),
    env.DB.prepare(`INSERT INTO messages(message_id, schema_version, task_id, sender, recipient, correlation_id,
      parent_id, status, body, evidence_refs_json, created_at, payload_hash)
      SELECT ?, ?, ?, ?, ?, ?, ?, 'recorded', ?, ?, ?, ? WHERE EXISTS (
        SELECT 1 FROM tasks WHERE task_id = ? AND workspace_id = ? AND last_mutation_id = ?
      )`)
      .bind(message.message_id, message.schema_version, taskId, message.sender, message.recipient, message.correlation_id,
        message.parent_id, message.body, JSON.stringify(evidenceRefs), createdAt, payloadHash,
        taskId, access.workspaceId, mutationId),
    env.DB.prepare(`INSERT INTO task_events(task_id, event_type, actor, correlation_id, details_json, occurred_at)
      SELECT ?, 'message_recorded', ?, ?, ?, ? WHERE EXISTS (
        SELECT 1 FROM tasks WHERE task_id = ? AND workspace_id = ? AND last_mutation_id = ?
      ) AND EXISTS(SELECT 1 FROM messages WHERE message_id = ? AND task_id = ?)`)
      .bind(taskId, message.sender, message.correlation_id, JSON.stringify({ message_id: message.message_id, recipient: message.recipient,
        verified_human: { user_id: access.user_id, session_id: access.session_id } }), createdAt, taskId, access.workspaceId, mutationId, message.message_id, taskId),
  ]);
  if (results[0].meta.changes !== 1 || results[1].meta.changes !== 1) return json({ error: "workspace access changed; reload and retry" }, 403);
  return json({ data: message }, 201);
}

async function transitionTask(request: Request, env: Env, access: WorkspaceAccess, task: TaskRecord): Promise<Response> {
  const input = await jsonBody<{ expected?: unknown; next?: unknown; expected_version?: unknown }>(request);
  if (input instanceof Response) return input;
  const allowed: Record<string, string[]> = {
    open: ["proposed"], proposed: ["under_review"], under_review: ["proposed", "resolved"], resolved: ["approved"], approved: [],
  };
  if (typeof input.expected !== "string" || typeof input.next !== "string" || !allowed[input.expected]?.includes(input.next)
    || !Number.isSafeInteger(input.expected_version)) return json({ error: "invalid task transition" }, 400);
  if (input.next === "approved" && access.role !== "OWNER") {
    return json({ error: "a verified workspace owner must perform this decision" }, 403);
  }
  if (task.status !== input.expected || task.version !== input.expected_version) {
    return json({ error: `stale task state: expected ${String(input.expected)}, found ${task.status}` }, 409);
  }
  if (task.pending_decision) return json({ error: "task has an unanswered human decision" }, 409);
  const now = new Date().toISOString();
  const correlation = crypto.randomUUID();
  const mutationId = crypto.randomUUID();
  const actor = `human:${access.user_id}`;
  const details = { from: input.expected, to: input.next, expected_version: input.expected_version,
    verified_human: { user_id: access.user_id, session_id: access.session_id } };
  const results = await env.DB.batch([
    env.DB.prepare(`UPDATE tasks SET status = ?, updated_at = ?, version = version + 1, last_mutation_id = ?
      WHERE task_id = ? AND workspace_id = ? AND status = ? AND version = ?
      AND NOT EXISTS(SELECT 1 FROM human_decisions WHERE task_id = ? AND status = 'pending')
      AND EXISTS(SELECT 1 FROM workspace_memberships m JOIN auth_sessions s ON s.session_id = ?
        JOIN identity_users u ON u.user_id = m.user_id WHERE m.workspace_id = ? AND m.user_id = ?
          AND m.revoked_at IS NULL AND m.role <> 'VIEWER' AND (? = 0 OR m.role = 'OWNER')
          AND s.user_id = m.user_id AND s.revoked_at IS NULL AND s.idle_expires_at > ?
          AND s.absolute_expires_at > ? AND u.disabled_at IS NULL)`)
      .bind(input.next, now, mutationId, task.task_id, access.workspaceId, input.expected, input.expected_version, task.task_id,
        access.session_id, access.workspaceId, access.user_id, input.next === "approved" ? 1 : 0, now, now),
    env.DB.prepare(`INSERT INTO task_events(task_id, event_type, actor, correlation_id, details_json, occurred_at)
      SELECT ?, 'task_transitioned', ?, ?, ?, ? WHERE EXISTS (
        SELECT 1 FROM tasks WHERE task_id = ? AND workspace_id = ? AND status = ? AND version = ? AND last_mutation_id = ?
      )`)
      .bind(task.task_id, actor, correlation, JSON.stringify(details), now, task.task_id, access.workspaceId, input.next,
        Number(input.expected_version) + 1, mutationId),
  ]);
  if (results[0].meta.changes !== 1) return json({ error: "task state changed; reload and retry" }, 409);
  const updated = await getTaskRecord(env, task.task_id, access.workspaceId);
  return updated ? json({ data: taskDto(updated) }) : json({ error: "task not found" }, 404);
}

async function requestDecision(request: Request, env: Env, access: WorkspaceAccess, task: TaskRecord): Promise<Response> {
  const input = await jsonBody<{ question?: unknown }>(request);
  if (input instanceof Response) return input;
  if (typeof input.question !== "string" || !input.question.trim() || [...input.question].length > 2_000) {
    return json({ error: "invalid decision question" }, 400);
  }
  if (["open", "approved"].includes(task.status)) {
    return json({ error: "human decisions can only be requested after a proposal exists and before approval" }, 400);
  }
  const now = new Date().toISOString();
  const decisionId = crypto.randomUUID();
  const correlation = crypto.randomUUID();
  const mutationId = crypto.randomUUID();
  const actor = `human:${access.user_id}`;
  try {
    const results = await env.DB.batch([
      env.DB.prepare(`INSERT INTO human_decisions(decision_id, task_id, state_at_request, question, requested_by, status, created_at)
        SELECT ?, t.task_id, t.status, ?, ?, 'pending', ? FROM tasks t
        WHERE t.task_id = ? AND t.workspace_id = ? AND t.version = ?
          AND EXISTS(SELECT 1 FROM workspace_memberships m JOIN auth_sessions s ON s.session_id = ?
            JOIN identity_users u ON u.user_id = m.user_id WHERE m.workspace_id = t.workspace_id
              AND m.user_id = ? AND m.revoked_at IS NULL AND m.role IN ('OWNER','EDITOR')
              AND s.user_id = m.user_id AND s.revoked_at IS NULL AND s.idle_expires_at > ?
              AND s.absolute_expires_at > ? AND u.disabled_at IS NULL)`)
        .bind(decisionId, input.question.trim(), actor, now, task.task_id, access.workspaceId, task.version,
          access.session_id, access.user_id, now, now),
      env.DB.prepare(`UPDATE tasks SET updated_at = ?, version = version + 1, last_mutation_id = ?
        WHERE task_id = ? AND workspace_id = ? AND version = ?
          AND EXISTS(SELECT 1 FROM human_decisions WHERE decision_id = ? AND status = 'pending')`)
        .bind(now, mutationId, task.task_id, access.workspaceId, task.version, decisionId),
      env.DB.prepare(`INSERT INTO task_events(task_id, event_type, actor, correlation_id, details_json, occurred_at)
        SELECT ?, 'human_decision_requested', ?, ?, ?, ? WHERE EXISTS(
          SELECT 1 FROM tasks WHERE task_id = ? AND workspace_id = ? AND last_mutation_id = ?
        )`)
        .bind(task.task_id, actor, correlation, JSON.stringify({ decision_id: decisionId, state_preserved: task.status,
          question: input.question.trim(), verified_human: { user_id: access.user_id, session_id: access.session_id } }), now,
          task.task_id, access.workspaceId, mutationId),
    ]);
    if (results[0].meta.changes !== 1 || results[1].meta.changes !== 1) return json({ error: "task state changed or a decision is already pending" }, 409);
  } catch {
    return json({ error: "task already has a pending decision" }, 409);
  }
  const decision = await env.DB.prepare(`SELECT decision_id, task_id, question, requested_by, status, created_at
    FROM human_decisions WHERE decision_id = ?`).bind(decisionId).first();
  return json({ data: decision }, 201);
}

async function answerDecision(request: Request, env: Env, access: WorkspaceAccess, task: TaskRecord, decisionId: string): Promise<Response> {
  const input = await jsonBody<{ answer?: unknown }>(request);
  if (input instanceof Response) return input;
  if (typeof input.answer !== "string" || !input.answer.trim() || [...input.answer].length > 4_000) {
    return json({ error: "invalid decision answer" }, 400);
  }
  const pending = await env.DB.prepare(`SELECT decision_id FROM human_decisions d JOIN tasks t ON t.task_id = d.task_id
    WHERE d.decision_id = ? AND d.task_id = ? AND t.workspace_id = ? AND d.status = 'pending'`)
    .bind(decisionId, task.task_id, access.workspaceId).first();
  if (!pending) return json({ error: "decision is not pending" }, 404);
  const now = new Date().toISOString();
  const actor = `human:${access.user_id}`;
  const correlation = crypto.randomUUID();
  const mutationId = crypto.randomUUID();
  const results = await env.DB.batch([
    env.DB.prepare(`UPDATE human_decisions SET status = 'answered', answer = ?, answered_by = ?, answered_at = ?
      WHERE decision_id = ? AND task_id = ? AND status = 'pending' AND EXISTS(
        SELECT 1 FROM workspace_memberships m JOIN auth_sessions s ON s.session_id = ?
        JOIN identity_users u ON u.user_id = m.user_id WHERE m.workspace_id = ? AND m.user_id = ?
          AND m.revoked_at IS NULL AND m.role IN ('OWNER','EDITOR') AND s.user_id = m.user_id
          AND s.revoked_at IS NULL AND s.idle_expires_at > ? AND s.absolute_expires_at > ? AND u.disabled_at IS NULL
      )`)
      .bind(input.answer.trim(), actor, now, decisionId, task.task_id, access.session_id, access.workspaceId, access.user_id, now, now),
    env.DB.prepare(`UPDATE tasks SET updated_at = ?, version = version + 1, last_mutation_id = ? WHERE task_id = ? AND workspace_id = ?
      AND EXISTS(SELECT 1 FROM human_decisions WHERE decision_id = ? AND status = 'answered' AND answered_at = ?)`)
      .bind(now, mutationId, task.task_id, access.workspaceId, decisionId, now),
    env.DB.prepare(`INSERT INTO task_events(task_id, event_type, actor, correlation_id, details_json, occurred_at)
      SELECT ?, 'human_decision_answered', ?, ?, ?, ? WHERE EXISTS(
        SELECT 1 FROM tasks WHERE task_id = ? AND workspace_id = ? AND last_mutation_id = ?
      )`)
      .bind(task.task_id, actor, correlation, JSON.stringify({ decision_id: decisionId, answer: input.answer.trim(),
        verified_human: { user_id: access.user_id, session_id: access.session_id } }), now, task.task_id, access.workspaceId, mutationId),
  ]);
  if (results[0].meta.changes !== 1) return json({ error: "decision is not pending or workspace access changed" }, 404);
  return json({ data: { decision_id: decisionId, status: "answered" } });
}

interface CheckpointContent {
  schema: string; version: string; checkpoint_id: string; task_id: string; source: string;
  source_completeness: string; goal: string; decisions: string; completed: string; blocked: string;
  next_actions: string; constraints: string; artifacts: Array<{ label: string; locator: string }>; context_capacity: string;
}

interface CheckpointRow {
  content_json: string; content_sha256: string; status: string; created_by: string; created_at: string;
  resumed_by: string | null; resumed_at: string | null;
}

async function listCheckpoints(env: Env, taskId: string, workspaceId: string): Promise<Response> {
  const rows = await env.DB.prepare(`SELECT content_json, content_sha256, status, created_by, created_at, resumed_by, resumed_at
    FROM context_checkpoints WHERE task_id = ? AND workspace_id = ? ORDER BY created_at DESC, checkpoint_id DESC`)
    .bind(taskId, workspaceId).all<CheckpointRow>();
  try {
    return json(await Promise.all(rows.results.map((row) => checkpointDto(row, taskId))));
  } catch {
    return json({ error: "checkpoint integrity check failed" }, 500);
  }
}

async function createCheckpoint(request: Request, env: Env, access: WorkspaceAccess, task: TaskRecord): Promise<Response> {
  const input = await jsonBody<Partial<CheckpointContent>>(request);
  if (input instanceof Response) return input;
  const checkpointId = input.checkpoint_id;
  const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
  if (typeof checkpointId !== "string" || !uuidPattern.test(checkpointId)
    || !["chatgpt_web", "codex", "unknown"].includes(String(input.source))
    || typeof input.goal !== "string" || !input.goal.trim() || [...input.goal].length > 2_000
    || !boundedString(input.decisions, 2_500) || !boundedString(input.completed, 1_500)
    || !boundedString(input.blocked, 1_500) || !boundedString(input.next_actions, 1_500)
    || !boundedString(input.constraints, 1_500) || !Array.isArray(input.artifacts) || input.artifacts.length > 12
    || !input.artifacts.every((artifact) => artifact && typeof artifact.label === "string" && artifact.label.trim()
      && [...artifact.label].length <= 120 && typeof artifact.locator === "string" && artifact.locator.trim()
      && [...artifact.locator].length <= 2_048)) return json({ error: "invalid context checkpoint" }, 400);
  const content: CheckpointContent = {
    schema: "codexpt.context-checkpoint", version: "1.0.0", checkpoint_id: checkpointId, task_id: task.task_id,
    source: input.source as string, source_completeness: "user_curated_partial", goal: input.goal.trim(),
    decisions: input.decisions as string, completed: input.completed as string, blocked: input.blocked as string,
    next_actions: input.next_actions as string, constraints: input.constraints as string,
    artifacts: input.artifacts as Array<{ label: string; locator: string }>,
    context_capacity: "unknown; no ChatGPT web context telemetry is available",
  };
  const contentJson = JSON.stringify(content);
  if (new TextEncoder().encode(contentJson).byteLength > 24 * 1024) return json({ error: "context checkpoint exceeds 24 KiB" }, 400);
  const contentHash = await sha256Hex(contentJson);
  const actor = `human:${access.user_id}`;
  const now = new Date().toISOString();
  const existing = await env.DB.prepare(`SELECT content_json, content_sha256, status, created_by, created_at, resumed_by, resumed_at
    FROM context_checkpoints WHERE checkpoint_id = ? AND workspace_id = ?`).bind(checkpointId, access.workspaceId).first<CheckpointRow>();
  if (existing) {
    if (existing.content_json !== contentJson || existing.created_by !== actor) return json({ error: "checkpoint ID conflict" }, 409);
    try { return json({ data: await checkpointDto(existing, task.task_id) }, 200); }
    catch { return json({ error: "checkpoint integrity check failed" }, 500); }
  }
  const ready = await env.DB.prepare(`SELECT checkpoint_id FROM context_checkpoints
    WHERE task_id = ? AND workspace_id = ? AND status = 'ready'`).bind(task.task_id, access.workspaceId).first();
  if (ready) return json({ error: "this task already has a ready checkpoint; resume it before creating another" }, 409);
  const eventId = crypto.randomUUID();
  try {
    const results = await env.DB.batch([
      env.DB.prepare(`INSERT INTO context_checkpoints(checkpoint_id, task_id, workspace_id, task_version, content_json,
        content_sha256, status, created_by, created_at)
        SELECT ?, ?, ?, ?, ?, ?, 'ready', ?, ? WHERE EXISTS (
          SELECT 1 FROM workspace_memberships m JOIN auth_sessions s ON s.session_id = ?
          JOIN identity_users u ON u.user_id = m.user_id WHERE m.workspace_id = ? AND m.user_id = ?
            AND m.revoked_at IS NULL AND m.role IN ('OWNER','EDITOR') AND s.user_id = m.user_id
            AND s.revoked_at IS NULL AND s.idle_expires_at > ? AND s.absolute_expires_at > ?
            AND u.disabled_at IS NULL
        ) AND EXISTS(SELECT 1 FROM tasks WHERE task_id = ? AND workspace_id = ? AND version = ?)`)
        .bind(checkpointId, task.task_id, access.workspaceId, task.version, contentJson, contentHash, actor, now,
          access.session_id, access.workspaceId, access.user_id, now, now, task.task_id, access.workspaceId, task.version),
      env.DB.prepare(`INSERT INTO task_events(task_id, event_type, actor, correlation_id, details_json, occurred_at)
        SELECT ?, 'context_checkpoint_ready', ?, ?, ?, ? WHERE EXISTS(
          SELECT 1 FROM context_checkpoints WHERE checkpoint_id = ? AND content_sha256 = ? AND status = 'ready'
        )`)
        .bind(task.task_id, actor, checkpointId, JSON.stringify({ checkpoint_id: checkpointId, sha256: contentHash,
          verified_human: { user_id: access.user_id, session_id: access.session_id } }), now, checkpointId, contentHash),
      env.DB.prepare(`INSERT INTO identity_audit_events(event_id, occurred_at, actor_user_id, actor_session_id, workspace_id,
        action, resource_type, resource_id, result, correlation_id, details_json)
        SELECT ?, ?, ?, ?, ?, 'checkpoint.create', 'context_checkpoint', ?, 'allowed', ?, '{}'
        WHERE EXISTS(SELECT 1 FROM context_checkpoints WHERE checkpoint_id = ? AND content_sha256 = ?
          AND created_by = ? AND created_at = ?)`)
        .bind(eventId, now, access.user_id, access.session_id, access.workspaceId, checkpointId, crypto.randomUUID(),
          checkpointId, contentHash, actor, now),
    ]);
    if (results[0].meta.changes !== 1) return json({ error: "workspace access or task state changed" }, 409);
  } catch {
    return json({ error: "checkpoint conflicts with current task state" }, 409);
  }
  return json({ data: { content, content_sha256: contentHash, status: "ready", created_by: actor,
    created_at: now, resumed_by: null, resumed_at: null } }, 201);
}

async function resumeCheckpoint(env: Env, access: WorkspaceAccess, taskId: string, checkpointId: string): Promise<Response> {
  const existing = await env.DB.prepare(`SELECT content_json, content_sha256, status, created_by, created_at, resumed_by, resumed_at
    FROM context_checkpoints WHERE checkpoint_id = ? AND task_id = ? AND workspace_id = ?`)
    .bind(checkpointId, taskId, access.workspaceId).first<CheckpointRow>();
  if (!existing) return json({ error: "checkpoint not found" }, 404);
  let current: Awaited<ReturnType<typeof checkpointDto>>;
  try { current = await checkpointDto(existing, taskId); }
  catch { return json({ error: "checkpoint integrity check failed" }, 500); }
  if (current.status === "resumed") {
    return current.resumed_by === `human:${access.user_id}`
      ? json({ data: current })
      : json({ error: "checkpoint was already resumed by another actor" }, 409);
  }
  const now = new Date().toISOString();
  const actor = `human:${access.user_id}`;
  const results = await env.DB.batch([
    env.DB.prepare(`UPDATE context_checkpoints SET status = 'resumed', resumed_by = ?, resumed_at = ?
      WHERE checkpoint_id = ? AND task_id = ? AND workspace_id = ? AND status = 'ready' AND EXISTS (
        SELECT 1 FROM workspace_memberships m JOIN auth_sessions s ON s.session_id = ?
        JOIN identity_users u ON u.user_id = m.user_id WHERE m.workspace_id = ? AND m.user_id = ?
          AND m.revoked_at IS NULL AND m.role IN ('OWNER','EDITOR') AND s.user_id = m.user_id
          AND s.revoked_at IS NULL AND s.idle_expires_at > ? AND s.absolute_expires_at > ? AND u.disabled_at IS NULL
      )`)
      .bind(actor, now, checkpointId, taskId, access.workspaceId, access.session_id, access.workspaceId, access.user_id, now, now),
    env.DB.prepare(`INSERT INTO task_events(task_id, event_type, actor, correlation_id, details_json, occurred_at)
      SELECT ?, 'context_checkpoint_resumed', ?, ?, ?, ? WHERE EXISTS (
        SELECT 1 FROM context_checkpoints WHERE checkpoint_id = ? AND status = 'resumed' AND resumed_at = ?
      )`)
      .bind(taskId, actor, checkpointId, JSON.stringify({ checkpoint_id: checkpointId,
        verified_human: { user_id: access.user_id, session_id: access.session_id } }), now, checkpointId, now),
  ]);
  if (results[0].meta.changes !== 1) return json({ error: "checkpoint state changed; reload and retry" }, 409);
  return json({ data: { ...current, status: "resumed", resumed_by: actor, resumed_at: now } });
}

async function checkpointDto(row: CheckpointRow, taskId: string) {
  const content = JSON.parse(row.content_json) as CheckpointContent;
  if (content.task_id !== taskId || content.schema !== "codexpt.context-checkpoint" || content.version !== "1.0.0"
    || !/^[a-f0-9]{64}$/.test(row.content_sha256)
    || await sha256Hex(row.content_json) !== row.content_sha256) throw new Error("checkpoint invalid");
  return { content, content_sha256: row.content_sha256, status: row.status, created_by: row.created_by,
    created_at: row.created_at, resumed_by: row.resumed_by, resumed_at: row.resumed_at };
}

function validEvidenceRef(item: unknown): item is { label: string; locator: string; sha256: string } {
  if (!item || typeof item !== "object") return false;
  const evidence = item as Record<string, unknown>;
  return typeof evidence.label === "string" && evidence.label.trim().length > 0 && [...evidence.label].length <= 120
    && typeof evidence.locator === "string" && evidence.locator.trim().length > 0 && [...evidence.locator].length <= 2_048
    && typeof evidence.sha256 === "string" && /^[a-f0-9]{64}$/i.test(evidence.sha256);
}

function boundedString(value: unknown, max: number): value is string {
  return typeof value === "string" && [...value].length <= max;
}

async function createInvite(request: Request, env: Env, access: WorkspaceAccess): Promise<Response> {
  const input = await jsonBody<{ email?: unknown; role?: unknown }>(request);
  if (input instanceof Response) return input;
  if (typeof input.email !== "string" || input.email.trim().length === 0 || input.email.trim().length > 320
    || !input.email.includes("@") || input.email.startsWith("@") || input.email.endsWith("@")
    || /[\u0000-\u001f\u007f]/.test(input.email)
    || (input.role !== "EDITOR" && input.role !== "VIEWER")) return json({ error: "invalid invitation" }, 400);
  const inviteId = crypto.randomUUID();
  const rawToken = randomToken();
  const now = new Date();
  const createdAt = now.toISOString();
  const expiresAt = new Date(now.getTime() + 7 * 24 * 60 * 60 * 1000).toISOString();
  try {
    const results = await env.DB.batch([
      env.DB.prepare(`INSERT INTO workspace_invites(invite_id, workspace_id, email_normalized, role, token_hash,
        invited_by, created_at, expires_at)
        SELECT ?, ?, ?, ?, ?, ?, ?, ? WHERE EXISTS(
          SELECT 1 FROM workspace_memberships actor
          JOIN auth_sessions session ON session.session_id = ?
          JOIN identity_users identity_user ON identity_user.user_id = actor.user_id
          WHERE actor.workspace_id = ? AND actor.user_id = ? AND actor.role = 'OWNER'
            AND actor.revoked_at IS NULL AND session.user_id = actor.user_id AND session.revoked_at IS NULL
            AND session.idle_expires_at > ? AND session.absolute_expires_at > ? AND identity_user.disabled_at IS NULL)`)
        .bind(inviteId, access.workspaceId, input.email.trim().toLowerCase(), input.role,
          await sha256Hex(rawToken), access.user_id, createdAt, expiresAt, access.session_id,
          access.workspaceId, access.user_id, createdAt, createdAt),
      env.DB.prepare(`INSERT INTO identity_audit_events(event_id, occurred_at, actor_user_id, actor_session_id, workspace_id,
        action, resource_type, resource_id, result, correlation_id, details_json)
        SELECT ?, ?, ?, ?, ?, 'member.invite', 'invite', ?, 'allowed', ?, '{}'
        WHERE EXISTS(SELECT 1 FROM workspace_invites WHERE invite_id = ? AND invited_by = ?)`)
        .bind(crypto.randomUUID(), createdAt, access.user_id, access.session_id, access.workspaceId,
          inviteId, crypto.randomUUID(), inviteId, access.user_id),
    ]);
    if (results[0].meta.changes !== 1) return identityError(409);
  } catch {
    return json({ error: "invitation could not be created" }, 409);
  }
  return json({ invite_id: inviteId, raw_token: rawToken, expires_at: expiresAt }, 201);
}

async function acceptInvite(request: Request, env: Env): Promise<Response> {
  const principal = await requirePrincipal(request, env);
  if (principal instanceof Response) return principal;
  const input = await jsonBody<{ token?: unknown }>(request);
  if (input instanceof Response) return input;
  if (typeof input.token !== "string" || input.token.length < 32 || input.token.length > 128) {
    return identityError(400);
  }
  const now = new Date().toISOString();
  const tokenHash = await sha256Hex(input.token);
  const invite = await env.DB.prepare(`SELECT invite_id, workspace_id, email_normalized, role FROM workspace_invites
    WHERE token_hash = ? AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > ?`)
    .bind(tokenHash, now).first<{ invite_id: string; workspace_id: string; email_normalized: string; role: "EDITOR" | "VIEWER" }>();
  const user = await env.DB.prepare(`SELECT email, email_verified, disabled_at FROM identity_users WHERE user_id = ?`)
    .bind(principal.user_id).first<{ email: string | null; email_verified: number; disabled_at: string | null }>();
  if (!invite || !user) return identityError(404);
  if (!user.email || user.email_verified !== 1 || user.disabled_at || user.email.trim().toLowerCase() !== invite.email_normalized) {
    return identityError(403);
  }
  const acceptedAt = new Date().toISOString();
  const results = await env.DB.batch([
    env.DB.prepare(`UPDATE workspace_invites SET accepted_at = ?, accepted_by = ?
      WHERE invite_id = ? AND token_hash = ? AND accepted_at IS NULL AND revoked_at IS NULL
        AND expires_at > ? AND email_normalized = ?
        AND EXISTS(SELECT 1 FROM identity_users user
          JOIN auth_sessions session ON session.user_id = user.user_id
          WHERE user.user_id = ? AND user.email_verified = 1 AND user.disabled_at IS NULL
            AND lower(trim(user.email)) = workspace_invites.email_normalized
            AND session.session_id = ? AND session.revoked_at IS NULL
            AND session.idle_expires_at > ? AND session.absolute_expires_at > ?)`)
      .bind(acceptedAt, principal.user_id, invite.invite_id, tokenHash, acceptedAt, invite.email_normalized,
        principal.user_id, principal.session_id, acceptedAt, acceptedAt),
    env.DB.prepare(`INSERT INTO identity_audit_events(event_id, occurred_at, actor_user_id, actor_session_id, workspace_id,
      action, resource_type, resource_id, result, correlation_id, details_json)
      SELECT ?, ?, ?, ?, ?, 'member.invite.accept', 'workspace_membership', ?, 'allowed', ?, '{}'
      WHERE EXISTS(SELECT 1 FROM workspace_invites WHERE invite_id = ? AND accepted_by = ? AND accepted_at = ?)`)
      .bind(crypto.randomUUID(), acceptedAt, principal.user_id, principal.session_id, invite.workspace_id,
        principal.user_id, crypto.randomUUID(), invite.invite_id, principal.user_id, acceptedAt),
  ]);
  if (results[0].meta.changes < 1) return identityError(409);
  return json({ workspace_id: invite.workspace_id, user_id: principal.user_id, role: invite.role }, 201);
}

async function changeMemberRole(request: Request, env: Env, access: WorkspaceAccess, targetUserId: string): Promise<Response> {
  const input = await jsonBody<{ role?: unknown }>(request);
  if (input instanceof Response) return input;
  if (input.role !== "OWNER" && input.role !== "EDITOR" && input.role !== "VIEWER") return identityError(400);
  const target = await env.DB.prepare(`SELECT role FROM workspace_memberships
    WHERE workspace_id = ? AND user_id = ? AND revoked_at IS NULL`).bind(access.workspaceId, targetUserId).first<{ role: string }>();
  if (!target) return identityError(404);
  const now = new Date().toISOString();
  const protectLastOwner = target.role === "OWNER" && input.role !== "OWNER";
  const results = await env.DB.batch([
    env.DB.prepare(`UPDATE workspace_memberships SET role = ?, role_updated_at = ?
      WHERE workspace_id = ? AND user_id = ? AND role = ? AND revoked_at IS NULL
        AND (? = 0 OR EXISTS(SELECT 1 FROM workspace_memberships other
          WHERE other.workspace_id = ? AND other.role = 'OWNER' AND other.revoked_at IS NULL AND other.user_id <> ?))
        AND EXISTS(SELECT 1 FROM workspace_memberships actor
          JOIN auth_sessions session ON session.session_id = ?
          JOIN identity_users user ON user.user_id = actor.user_id
          WHERE actor.workspace_id = ? AND actor.user_id = ? AND actor.role = 'OWNER'
            AND actor.revoked_at IS NULL AND session.user_id = actor.user_id AND session.revoked_at IS NULL
            AND session.idle_expires_at > ? AND session.absolute_expires_at > ? AND user.disabled_at IS NULL)`)
      .bind(input.role, now, access.workspaceId, targetUserId, target.role,
        protectLastOwner ? 1 : 0, access.workspaceId, targetUserId, access.session_id,
        access.workspaceId, access.user_id, now, now),
    env.DB.prepare(`UPDATE auth_sessions SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL AND ? <> ?
      AND EXISTS(SELECT 1 FROM workspace_memberships WHERE workspace_id = ? AND user_id = ? AND role_updated_at = ? AND role = ?)`)
      .bind(now, targetUserId, input.role, target.role, access.workspaceId, targetUserId, now, input.role),
    env.DB.prepare(`INSERT INTO identity_audit_events(event_id, occurred_at, actor_user_id, actor_session_id,
      workspace_id, action, resource_type, resource_id, result, correlation_id, details_json)
      SELECT ?, ?, ?, ?, ?, 'member.role.change', 'workspace_membership', ?, 'allowed', ?, ?
      WHERE EXISTS(SELECT 1 FROM workspace_memberships WHERE workspace_id = ? AND user_id = ? AND role = ? AND role_updated_at = ? AND revoked_at IS NULL)`)
      .bind(crypto.randomUUID(), now, access.user_id, access.session_id, access.workspaceId, targetUserId,
        crypto.randomUUID(), JSON.stringify({ from: target.role, to: input.role }), access.workspaceId, targetUserId, input.role, now),
  ]);
  if (results[0].meta.changes !== 1) return identityError(409);
  return new Response(null, { status: 204, headers: securityHeaders() });
}

async function revokeMember(env: Env, access: WorkspaceAccess, targetUserId: string): Promise<Response> {
  const target = await env.DB.prepare(`SELECT role FROM workspace_memberships
    WHERE workspace_id = ? AND user_id = ? AND revoked_at IS NULL`).bind(access.workspaceId, targetUserId).first<{ role: string }>();
  if (!target) return identityError(404);
  const now = new Date().toISOString();
  const results = await env.DB.batch([
    env.DB.prepare(`UPDATE workspace_memberships SET revoked_at = ?
      WHERE workspace_id = ? AND user_id = ? AND role = ? AND revoked_at IS NULL
        AND (role <> 'OWNER' OR EXISTS(SELECT 1 FROM workspace_memberships other
          WHERE other.workspace_id = ? AND other.role = 'OWNER' AND other.revoked_at IS NULL AND other.user_id <> ?))
        AND EXISTS(SELECT 1 FROM workspace_memberships actor
          JOIN auth_sessions session ON session.session_id = ?
          JOIN identity_users identity_user ON identity_user.user_id = actor.user_id
          WHERE actor.workspace_id = ? AND actor.user_id = ? AND actor.role = 'OWNER'
            AND actor.revoked_at IS NULL AND session.user_id = actor.user_id AND session.revoked_at IS NULL
            AND session.idle_expires_at > ? AND session.absolute_expires_at > ? AND identity_user.disabled_at IS NULL)`)
      .bind(now, access.workspaceId, targetUserId, target.role, access.workspaceId, targetUserId,
        access.session_id, access.workspaceId, access.user_id, now, now),
    env.DB.prepare("UPDATE auth_sessions SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL AND EXISTS (SELECT 1 FROM workspace_memberships WHERE workspace_id = ? AND user_id = ? AND revoked_at = ?)")
      .bind(now, targetUserId, access.workspaceId, targetUserId, now),
    env.DB.prepare(`INSERT INTO identity_audit_events(event_id, occurred_at, actor_user_id, actor_session_id, workspace_id,
      action, resource_type, resource_id, result, correlation_id, details_json)
      SELECT ?, ?, ?, ?, ?, 'member.revoke', 'workspace_membership', ?, 'allowed', ?, '{}'
      WHERE EXISTS(SELECT 1 FROM workspace_memberships WHERE workspace_id = ? AND user_id = ? AND revoked_at = ? AND role = ?)`)
      .bind(crypto.randomUUID(), now, access.user_id, access.session_id, access.workspaceId, targetUserId,
        crypto.randomUUID(), access.workspaceId, targetUserId, now, target.role),
  ]);
  if (results[0].meta.changes !== 1) return identityError(409);
  return new Response(null, { status: 204, headers: securityHeaders() });
}

async function transferOwnership(request: Request, env: Env, access: WorkspaceAccess): Promise<Response> {
  const input = await jsonBody<{ new_owner_user_id?: unknown }>(request);
  if (input instanceof Response) return input;
  if (typeof input.new_owner_user_id !== "string" || input.new_owner_user_id === access.user_id) return identityError(400);
  const targetId = input.new_owner_user_id;
  const target = await env.DB.prepare(`SELECT user_id, role FROM workspace_memberships
    WHERE workspace_id = ? AND user_id = ? AND revoked_at IS NULL`).bind(access.workspaceId, targetId).first<{ user_id: string; role: string }>();
  if (!target) return identityError(404);
  const now = new Date().toISOString();
  const auditId = crypto.randomUUID();
  const results = await env.DB.batch([
    env.DB.prepare(`UPDATE workspace_memberships SET role = 'OWNER', role_updated_at = ?
      WHERE workspace_id = ? AND user_id = ? AND role = ? AND revoked_at IS NULL
        AND EXISTS(SELECT 1 FROM workspace_memberships actor
          JOIN auth_sessions session ON session.session_id = ?
          JOIN identity_users identity_user ON identity_user.user_id = actor.user_id
          WHERE actor.workspace_id = ? AND actor.user_id = ? AND actor.role = 'OWNER' AND actor.revoked_at IS NULL
            AND session.user_id = actor.user_id AND session.revoked_at IS NULL
            AND session.idle_expires_at > ? AND session.absolute_expires_at > ? AND identity_user.disabled_at IS NULL)`)
      .bind(now, access.workspaceId, targetId, target.role, access.session_id, access.workspaceId, access.user_id, now, now),
    env.DB.prepare(`UPDATE workspace_memberships SET role = 'EDITOR', role_updated_at = ? WHERE workspace_id = ? AND user_id = ?
      AND revoked_at IS NULL AND role = 'OWNER' AND role_updated_at <> ?
      AND EXISTS(SELECT 1 FROM workspace_memberships WHERE workspace_id = ? AND user_id = ? AND role = 'OWNER' AND revoked_at IS NULL AND role_updated_at = ?)`)
      .bind(now, access.workspaceId, access.user_id, now, access.workspaceId, targetId, now),
    env.DB.prepare(`UPDATE auth_sessions SET revoked_at = ? WHERE user_id IN (?, ?) AND revoked_at IS NULL`)
      .bind(now, access.user_id, targetId),
    env.DB.prepare(`INSERT INTO identity_audit_events(event_id, occurred_at, actor_user_id, actor_session_id, workspace_id,
      action, resource_type, resource_id, result, correlation_id, details_json)
      SELECT ?, ?, ?, ?, ?, 'ownership.transfer', 'workspace_membership', ?, 'allowed', ?, ?
      WHERE EXISTS(SELECT 1 FROM workspace_memberships WHERE workspace_id = ? AND user_id = ? AND role = 'EDITOR' AND role_updated_at = ? AND revoked_at IS NULL)`)
      .bind(auditId, now, access.user_id, access.session_id, access.workspaceId, targetId, crypto.randomUUID(),
        JSON.stringify({ previous_owner: access.user_id, new_owner: targetId }), access.workspaceId, access.user_id, now),
  ]);
  if (results[0].meta.changes !== 1 || results[1].meta.changes !== 1) return identityError(409);
  return new Response(null, { status: 204, headers: securityHeaders() });
}

function identityError(status: number): Response {
  const errors: Record<number, string> = {
    400: "invalid identity request",
    403: "identity operation is not authorized",
    404: "identity or resource was not found",
    409: "identity state conflicts with this operation",
  };
  return json({ error: errors[status] ?? "identity operation could not be completed" }, status);
}

function securityHeaders(): Headers {
  return new Headers({
    "cache-control": "no-store",
    "x-content-type-options": "nosniff",
    "x-frame-options": "DENY",
    "referrer-policy": "no-referrer",
    "permissions-policy": "camera=(), microphone=(), geolocation=()",
    "content-security-policy": "default-src 'none'; frame-ancestors 'none'; base-uri 'none'",
  });
}
