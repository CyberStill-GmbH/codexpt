import { env as cloudflareEnv } from "cloudflare:workers";
import { beforeEach, describe, expect, it } from "vitest";
import worker, { type Env } from "../src/worker";

const ownerId = "10000000-0000-4000-8000-000000000001";
const workspaceId = "20000000-0000-4000-8000-000000000002";
const ownerSessionId = "30000000-0000-4000-8000-000000000003";
const rawSession = "test-session-token-with-enough-entropy-for-hash";
const proxySecret = "test-proxy-secret-with-more-than-32-bytes";
const db = (cloudflareEnv as unknown as { DB: D1Database }).DB;

describe("Cloudflare D1 API integration", () => {
  beforeEach(async () => {
    await db.batch([
      db.prepare("DELETE FROM identity_audit_events"),
      db.prepare("DELETE FROM context_checkpoints"),
      db.prepare("DELETE FROM human_decisions"),
      db.prepare("DELETE FROM messages"),
      db.prepare("DELETE FROM task_events"),
      db.prepare("DELETE FROM tasks"),
      db.prepare("DELETE FROM workspace_invites"),
      db.prepare("DELETE FROM auth_sessions"),
      db.prepare("DELETE FROM auth_transactions"),
      db.prepare("DELETE FROM workspace_memberships"),
      db.prepare("DELETE FROM workspaces"),
      db.prepare("DELETE FROM identity_users"),
      db.prepare("DELETE FROM identity_bootstrap"),
    ]);
    const now = new Date().toISOString();
    const hash = await sha256Hex(rawSession);
    await db.batch([
      db.prepare(`INSERT INTO identity_users(user_id, issuer, subject, email, email_verified, created_at)
        VALUES (?, 'https://identity.example.test', 'owner-subject', 'owner@example.test', 1, ?)`)
        .bind(ownerId, now),
      db.prepare("INSERT INTO workspaces(workspace_id, name, created_at) VALUES (?, 'Workspace', ?)").bind(workspaceId, now),
      db.prepare(`INSERT INTO workspace_memberships(workspace_id, user_id, role, created_at, role_updated_at)
        VALUES (?, ?, 'OWNER', ?, ?)`)
        .bind(workspaceId, ownerId, now, now),
      db.prepare(`INSERT INTO auth_sessions(session_id, user_id, token_hash, created_at, last_seen_at, idle_expires_at, absolute_expires_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)`)
        .bind(ownerSessionId, ownerId, hash, now, now, new Date(Date.now() + 60_000).toISOString(), new Date(Date.now() + 3_600_000).toISOString()),
    ]);
  });

  it("serves authenticated tenant workspaces and task creation from D1", async () => {
    const configuration = apiEnv();
    const workspaces = await worker.fetch(await apiRequest("/api/workspaces", "GET", configuration), configuration);
    expect(workspaces.status).toBe(200);
    expect(await workspaces.json()).toEqual([{ workspace_id: workspaceId, name: "Workspace", role: "OWNER" }]);

    const created = await worker.fetch(await apiRequest("/api/tasks", "POST", configuration, {
      title: "Review release",
      description: "Verify Cloudflare worker API",
    } ), configuration);
    expect(created.status).toBe(201);
    const payload = await created.json() as { data: { task_id: string; status: string; version: number; pending_decision: boolean } };
    expect(payload.data).toMatchObject({ status: "open", version: 1, pending_decision: false });

    const detail = await worker.fetch(await apiRequest(`/api/tasks/${payload.data.task_id}`, "GET", configuration), configuration);
    expect(detail.status).toBe(200);
    expect(await detail.json()).toMatchObject({ task: { task_id: payload.data.task_id }, messages: [], events: [{ event_type: "task_created" }], pending_decision: null });
  });

  it("rejects task reads across workspace boundaries even with a valid session", async () => {
    const now = new Date().toISOString();
    const otherWorkspace = "40000000-0000-4000-8000-000000000004";
    const taskId = "50000000-0000-4000-8000-000000000005";
    await db.batch([
      db.prepare("INSERT INTO workspaces(workspace_id, name, created_at) VALUES (?, 'Other', ?)").bind(otherWorkspace, now),
      db.prepare(`INSERT INTO tasks(task_id, title, description, status, created_at, updated_at, workspace_id, version, last_mutation_id)
        VALUES (?, 'Private task', '', 'open', ?, ?, ?, 1, 'fixture')`).bind(taskId, now, now, otherWorkspace),
    ]);
    const configuration = apiEnv();
    const response = await worker.fetch(await apiRequest(`/api/tasks/${taskId}`, "GET", configuration), configuration);
    expect(response.status).toBe(404);
  });

  it("keeps role changes and ownership transfer consistent in D1", async () => {
    const now = new Date().toISOString();
    const memberId = "70000000-0000-4000-8000-000000000007";
    await db.batch([
      db.prepare(`INSERT INTO identity_users(user_id, issuer, subject, email, email_verified, created_at)
        VALUES (?, 'https://identity.example.test', 'member-subject', 'member@example.test', 1, ?)`)
        .bind(memberId, now),
      db.prepare(`INSERT INTO workspace_memberships(workspace_id, user_id, role, created_at, role_updated_at)
        VALUES (?, ?, 'EDITOR', ?, ?)`)
        .bind(workspaceId, memberId, now, now),
    ]);
    const configuration = apiEnv();
    const demoteLastOwner = await worker.fetch(await apiRequest(`/api/workspaces/${workspaceId}/members/${ownerId}`, "PATCH", configuration, {
      role: "EDITOR",
    }), configuration);
    expect(demoteLastOwner.status).toBe(409);

    const changeRole = await worker.fetch(await apiRequest(`/api/workspaces/${workspaceId}/members/${memberId}`, "PATCH", configuration, {
      role: "VIEWER",
    }), configuration);
    expect(changeRole.status).toBe(204);
    const role = await db.prepare(`SELECT role FROM workspace_memberships WHERE workspace_id = ? AND user_id = ?`)
      .bind(workspaceId, memberId).first<{ role: string }>();
    expect(role?.role).toBe("VIEWER");

    const transfer = await worker.fetch(await apiRequest(`/api/workspaces/${workspaceId}/transfer-ownership`, "POST", configuration, {
      new_owner_user_id: memberId,
    }), configuration);
    expect(transfer.status).toBe(204);
    const memberships = await db.prepare(`SELECT user_id, role FROM workspace_memberships WHERE workspace_id = ? ORDER BY user_id`)
      .bind(workspaceId).all<{ user_id: string; role: string }>();
    expect(memberships.results).toEqual([
      { user_id: ownerId, role: "EDITOR" },
      { user_id: memberId, role: "OWNER" },
    ]);
  });

  it("creates a single-use invite and activates membership only for its verified email", async () => {
    const configuration = apiEnv();
    const created = await worker.fetch(await apiRequest(`/api/workspaces/${workspaceId}/invites`, "POST", configuration, {
      email: "member@example.test", role: "EDITOR",
    }), configuration);
    expect(created.status).toBe(201);
    const invite = await created.json() as { raw_token: string };

    const now = new Date().toISOString();
    const memberId = "80000000-0000-4000-8000-000000000008";
    const memberSessionId = "90000000-0000-4000-8000-000000000009";
    const memberRawSession = "member-session-token-with-enough-entropy-for-test";
    await db.prepare(`INSERT INTO identity_users(user_id, issuer, subject, email, email_verified, created_at)
      VALUES (?, 'https://identity.example.test', 'invitee-subject', 'member@example.test', 1, ?)`)
      .bind(memberId, now).run();
    await db.prepare(`INSERT INTO auth_sessions(session_id, user_id, token_hash, created_at, last_seen_at, idle_expires_at, absolute_expires_at)
      VALUES (?, ?, ?, ?, ?, ?, ?)`)
      .bind(memberSessionId, memberId, await sha256Hex(memberRawSession), now, now,
        new Date(Date.now() + 60_000).toISOString(), new Date(Date.now() + 3_600_000).toISOString()).run();
    const accepted = await worker.fetch(await apiRequest("/api/invites/accept", "POST", configuration, {
      token: invite.raw_token,
    }, memberRawSession), configuration);
    expect(accepted.status, await accepted.clone().text()).toBe(201);
    expect(await accepted.json()).toMatchObject({ workspace_id: workspaceId, user_id: memberId, role: "EDITOR" });
    const membership = await db.prepare(`SELECT role, revoked_at FROM workspace_memberships WHERE workspace_id = ? AND user_id = ?`)
      .bind(workspaceId, memberId).first<{ role: string; revoked_at: string | null }>();
    expect(membership).toEqual({ role: "EDITOR", revoked_at: null });

    const replay = await worker.fetch(await apiRequest("/api/invites/accept", "POST", configuration, {
      token: invite.raw_token,
    }, memberRawSession), configuration);
    expect(replay.status).toBe(404);
  });

  it("records messages, enforces optimistic transitions, and preserves human decision state", async () => {
    const configuration = apiEnv();
    const create = await worker.fetch(await apiRequest("/api/tasks", "POST", configuration, {
      title: "Review workflow", description: "",
    }), configuration);
    const { data: task } = await create.json() as { data: { task_id: string; version: number } };

    const message = await worker.fetch(await apiRequest(`/api/tasks/${task.task_id}/messages`, "POST", configuration, {
      sender: "human:forged", recipient: "agent:chatgpt", body: "Start the review", evidence_refs: [],
    }), configuration);
    expect(message.status).toBe(201);
    expect(await message.json()).toMatchObject({ data: { sender: `human:${ownerId}`, status: "recorded" } });

    const proposed = await worker.fetch(await apiRequest(`/api/tasks/${task.task_id}/transition`, "POST", configuration, {
      expected: "open", next: "proposed", actor: "human:forged", expected_version: 1,
    }), configuration);
    expect(proposed.status).toBe(409);

    const firstTransition = await worker.fetch(await apiRequest(`/api/tasks/${task.task_id}/transition`, "POST", configuration, {
      expected: "open", next: "proposed", expected_version: 2,
    }), configuration);
    expect(firstTransition.status).toBe(200);
    const detail = await worker.fetch(await apiRequest(`/api/tasks/${task.task_id}`, "GET", configuration), configuration);
    expect((await detail.json() as { task: { status: string; version: number } }).task).toMatchObject({ status: "proposed", version: 3 });
  });

  it("creates checkpoints with verified content digests and supports an explicit idempotent handoff", async () => {
    const configuration = apiEnv();
    const create = await worker.fetch(await apiRequest("/api/tasks", "POST", configuration, {
      title: "Context handoff", description: "",
    }), configuration);
    const { data: task } = await create.json() as { data: { task_id: string } };
    const checkpointId = "60000000-0000-4000-8000-000000000006";
    const checkpoint = {
      checkpoint_id: checkpointId,
      source: "codex",
      goal: "Continue the release review",
      decisions: "Use Cloudflare Workers + D1",
      completed: "Ported identity and session endpoints",
      blocked: "OIDC issuer registration",
      next_actions: "Complete the task API and verify the provider callback",
      constraints: "Do not change shared DNS",
      artifacts: [{ label: "ADR", locator: "docs/adr/0015-cloudflare-workers-durable-objects-hosting-spike.md" }],
    };
    const created = await worker.fetch(await apiRequest(`/api/tasks/${task.task_id}/checkpoints`, "POST", configuration, checkpoint), configuration);
    expect(created.status).toBe(201);
    const output = await created.json() as { data: { content_sha256: string; status: string; content: { context_capacity: string } } };
    expect(output.data.content_sha256).toMatch(/^[a-f0-9]{64}$/);
    expect(output.data.status).toBe("ready");
    expect(output.data.content.context_capacity).toContain("unknown");

    const resumed = await worker.fetch(await apiRequest(`/api/tasks/${task.task_id}/checkpoints/${checkpointId}/resume`, "POST", configuration, {}), configuration);
    expect(resumed.status).toBe(200);
    expect(await resumed.json()).toMatchObject({ data: { status: "resumed", resumed_by: `human:${ownerId}` } });
  });
});

function apiEnv(): Env {
  return {
    DB: db,
    PROXY_SECRET: proxySecret,
    API_HOST: "codexpt-api.account.workers.dev",
    WEB_ORIGIN: "https://codexpt.vercel.app",
    OIDC_ISSUER: "https://identity.example.test",
    OIDC_CLIENT_ID: "client-id",
    OIDC_REDIRECT_URI: "https://codexpt.vercel.app/api/auth/callback",
    LOGIN_SUCCESS_URL: "https://codexpt.vercel.app/",
  };
}

async function apiRequest(path: string, method: string, configuration: Env, body?: unknown, sessionToken = rawSession): Promise<Request> {
  const csrf = await csrfFor(sessionToken);
  const headers = new Headers({
    ["x-codexpt-proxy-key"]: configuration.PROXY_SECRET,
    cookie: `__Host-codexpt_session=${sessionToken}; __Host-codexpt_csrf=${csrf}`,
    ["x-codexpt-workspace-id"]: workspaceId,
  });
  if (method !== "GET") {
    headers.set("origin", configuration.WEB_ORIGIN);
    headers.set("content-type", "application/json");
    headers.set("x-csrf-token", csrf);
  }
  return new Request(`https://${configuration.API_HOST}${path}`, {
    method,
    headers,
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
}

async function csrfFor(value: string): Promise<string> {
  const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`codexpt-csrf-v1:${value}`));
  let binary = "";
  for (const byte of new Uint8Array(bytes)) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

async function sha256Hex(value: string): Promise<string> {
  const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return [...new Uint8Array(bytes)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}
