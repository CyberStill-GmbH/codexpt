import { describe, expect, it, vi } from "vitest";
import worker, { type Env } from "../src/worker";

function makeEnv(overrides: Partial<Env> = {}): Env {
  return {
    DB: {
      prepare: vi.fn(() => ({ first: vi.fn(async () => ({ value: 1 })) })),
    } as unknown as D1Database,
    PROXY_SECRET: "test-proxy-secret-with-more-than-32-bytes",
    API_HOST: "codexpt-api.account.workers.dev",
    WEB_ORIGIN: "https://codexpt.vercel.app",
    OIDC_ISSUER: "https://identity.example.test",
    OIDC_CLIENT_ID: "client-id",
    OIDC_REDIRECT_URI: "https://codexpt.vercel.app/api/auth/callback",
    LOGIN_SUCCESS_URL: "https://codexpt.vercel.app/",
    ...overrides,
  };
}

describe("Cloudflare API boundary", () => {
  it("exposes only a generic public liveness response", async () => {
    const response = await worker.fetch(new Request("https://codexpt-api.account.workers.dev/api/health"), makeEnv());
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ status: "ok" });
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(response.headers.get("x-content-type-options")).toBe("nosniff");
  });

  it("does not expose readiness without the trusted Vercel proxy", async () => {
    const response = await worker.fetch(
      new Request("https://codexpt-api.account.workers.dev/api/health/ready"),
      makeEnv(),
    );
    expect(response.status).toBe(403);
    expect(await response.json()).toEqual({ error: "trusted application proxy required" });
  });

  it("reports OIDC configuration only when the issuer and exact Vercel callback are valid", async () => {
    const validEnv = makeEnv();
    const configured = await worker.fetch(
      new Request("https://codexpt-api.account.workers.dev/api/auth/status", {
        headers: { ["x-codexpt-proxy-key"]: validEnv.PROXY_SECRET },
      }),
      validEnv,
    );
    expect(await configured.json()).toEqual({ configured: true, provider: "proveedor de identidad" });

    const invalidEnv = makeEnv({ OIDC_REDIRECT_URI: "https://other.example/callback" });
    const unavailable = await worker.fetch(
      new Request("https://codexpt-api.account.workers.dev/api/auth/status", {
        headers: { ["x-codexpt-proxy-key"]: invalidEnv.PROXY_SECRET },
      }),
      invalidEnv,
    );
    expect(await unavailable.json()).toEqual({ configured: false, provider: "proveedor de identidad" });
  });

  it("rejects browser traffic that bypasses the server-side proxy", async () => {
    const response = await worker.fetch(
      new Request("https://codexpt-api.account.workers.dev/api/auth/status", {
        headers: { origin: "https://codexpt.vercel.app" },
      }),
      makeEnv(),
    );
    expect(response.status).toBe(403);
  });

  it("requires the exact API host and same-site JSON on mutations", async () => {
    const env = makeEnv();
    const wrongHost = await worker.fetch(
      new Request("https://attacker.workers.dev/api/tasks", { headers: { ["x-codexpt-proxy-key"]: env.PROXY_SECRET } }),
      env,
    );
    expect(wrongHost.status).toBe(403);

    const wrongOrigin = await worker.fetch(
      new Request("https://codexpt-api.account.workers.dev/api/tasks/unknown/route", {
        method: "POST",
        headers: {
          ["x-codexpt-proxy-key"]: env.PROXY_SECRET,
          origin: "https://attacker.example",
          "content-type": "application/json",
        },
        body: "{}",
      }),
      env,
    );
    expect(wrongOrigin.status).toBe(403);

    const wrongType = await worker.fetch(
      new Request("https://codexpt-api.account.workers.dev/api/tasks/unknown/route", {
        method: "POST",
        headers: {
          ["x-codexpt-proxy-key"]: env.PROXY_SECRET,
          origin: env.WEB_ORIGIN,
          "content-type": "text/plain",
        },
        body: "{}",
      }),
      env,
    );
    expect(wrongType.status).toBe(415);
  });

  it("requires a session-bound CSRF value for every browser mutation", async () => {
    const env = makeEnv();
    const response = await worker.fetch(
      new Request("https://codexpt-api.account.workers.dev/api/auth/logout", {
        method: "POST",
        headers: {
          ["x-codexpt-proxy-key"]: env.PROXY_SECRET,
          origin: env.WEB_ORIGIN,
          "content-type": "application/json",
        },
        body: "{}",
      }),
      env,
    );
    expect(response.status).toBe(403);
    expect(await response.json()).toEqual({ error: "CSRF validation failed" });
  });

  it("bounds streamed request bodies even without a declared content length", async () => {
    const env = makeEnv();
    const response = await worker.fetch(
      new Request("https://codexpt-api.account.workers.dev/api/tasks", {
        method: "POST",
        headers: {
          ["x-codexpt-proxy-key"]: env.PROXY_SECRET,
          origin: env.WEB_ORIGIN,
          "content-type": "application/json",
        },
        body: "x".repeat(32 * 1024 + 1),
      }),
      env,
    );
    expect(response.status).toBe(413);
  });

  it("fails readiness until OIDC and its exact callback are configured", async () => {
    const env = makeEnv({ OIDC_CLIENT_ID: "" });
    const response = await worker.fetch(
      new Request("https://codexpt-api.account.workers.dev/api/health/ready", {
        headers: { ["x-codexpt-proxy-key"]: env.PROXY_SECRET },
      }),
      env,
    );
    expect(response.status).toBe(503);
    expect(await response.json()).toEqual({ error: "service is not ready" });
  });

  it("does not pretend unimplemented application routes exist", async () => {
    const env = makeEnv();
    const response = await worker.fetch(
      new Request("https://codexpt-api.account.workers.dev/api/tasks/unknown/route", {
        headers: { ["x-codexpt-proxy-key"]: env.PROXY_SECRET },
      }),
      env,
    );
    expect(response.status).toBe(404);
    expect(await response.json()).toEqual({ error: "not found" });
  });
});
