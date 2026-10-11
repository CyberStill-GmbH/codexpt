import { DurableObject } from "cloudflare:workers";

/**
 * Compatibility spike only. No request routes are exposed; the exported
 * Durable Object is accessed directly by local tests. Production routing,
 * authentication, migration, backup and recovery are deliberately absent.
 */
export default {
  fetch(_request: Request, _env: Env): Response {
    return new Response("Not found", { status: 404 });
  },
};

export interface Env {
  WORKSPACES: DurableObjectNamespace<WorkspaceStore>;
}

export type Role = "owner" | "editor" | "viewer";

export interface CheckpointInput {
  workspaceId: string;
  taskId: string;
  checkpointId: string;
  actorId: string;
  body: string;
}

export interface CheckpointResult {
  checkpointId: string;
  status: "ready" | "resumed";
  digest: string;
  idempotent: boolean;
}

export interface ResumeInput {
  workspaceId: string;
  taskId: string;
  checkpointId: string;
  actorId: string;
}

export class CheckpointConflict extends Error {}
export class CheckpointForbidden extends Error {}
export class CheckpointNotFound extends Error {}
export class CheckpointInvalid extends Error {}

export class WorkspaceStore extends DurableObject<Env> {
  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    ctx.storage.sql.exec(`
      CREATE TABLE IF NOT EXISTS workspace_members (
        workspace_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        role TEXT NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
        revoked INTEGER NOT NULL DEFAULT 0 CHECK (revoked IN (0, 1)),
        PRIMARY KEY (workspace_id, user_id)
      );
      CREATE TABLE IF NOT EXISTS tasks (
        task_id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL,
        version INTEGER NOT NULL DEFAULT 0
      );
      CREATE TABLE IF NOT EXISTS context_checkpoints (
        checkpoint_id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL,
        task_id TEXT NOT NULL REFERENCES tasks(task_id),
        created_by TEXT NOT NULL,
        body TEXT NOT NULL,
        digest TEXT NOT NULL,
        status TEXT NOT NULL CHECK (status IN ('ready', 'resumed')),
        created_at TEXT NOT NULL,
        resumed_at TEXT
      );
      CREATE UNIQUE INDEX IF NOT EXISTS one_ready_checkpoint_per_task
        ON context_checkpoints(task_id) WHERE status = 'ready';
      CREATE TABLE IF NOT EXISTS audit_events (
        event_id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL,
        task_id TEXT NOT NULL,
        actor_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        checkpoint_id TEXT NOT NULL,
        created_at TEXT NOT NULL
      );
    `);
  }

  /** Durable Object HTTP dispatch remains closed in this storage-only spike. */
  fetch(_request: Request): Response {
    return new Response("Not found", { status: 404 });
  }

  /** Test fixture only; no Worker HTTP route invokes this method. */
  seedFixture(workspaceId: string, taskId: string, userId: string, role: Role): void {
    this.ctx.storage.transactionSync(() => {
      this.ctx.storage.sql.exec(
        `INSERT INTO workspace_members(workspace_id, user_id, role)
         VALUES (?, ?, ?) ON CONFLICT(workspace_id, user_id)
         DO UPDATE SET role = excluded.role, revoked = 0`,
        workspaceId,
        userId,
        role,
      );
      this.ctx.storage.sql.exec(
        `INSERT INTO tasks(task_id, workspace_id) VALUES (?, ?)
         ON CONFLICT(task_id) DO NOTHING`,
        taskId,
        workspaceId,
      );
    });
  }

  /** Test fixture only; mirrors a revocation committed in this workspace store. */
  revokeFixtureMember(workspaceId: string, userId: string): void {
    this.ctx.storage.sql.exec(
      `UPDATE workspace_members SET revoked = 1 WHERE workspace_id = ? AND user_id = ?`,
      workspaceId,
      userId,
    );
  }

  async createCheckpoint(input: CheckpointInput): Promise<CheckpointResult> {
    validate(input);
    const payload = JSON.stringify({ taskId: input.taskId, body: input.body });
    const digest = await sha256(payload);
    const now = new Date().toISOString();
    const candidateEventId = crypto.randomUUID();

    return this.ctx.storage.transactionSync(() => {
      const membership = this.ctx.storage.sql
        .exec<{ role: Role; revoked: number }>(
          `SELECT role, revoked FROM workspace_members
           WHERE workspace_id = ? AND user_id = ?`,
          input.workspaceId,
          input.actorId,
        )
        .toArray()[0];
      if (!membership || membership.revoked || membership.role === "viewer") {
        throw new CheckpointForbidden("Actor cannot create checkpoints in this workspace");
      }

      const task = this.ctx.storage.sql
        .exec<{ workspace_id: string }>(
          `SELECT workspace_id FROM tasks WHERE task_id = ?`,
          input.taskId,
        )
        .toArray()[0];
      if (!task || task.workspace_id !== input.workspaceId) {
        throw new CheckpointNotFound("Task does not exist in this workspace");
      }

      const previous = this.ctx.storage.sql
        .exec<{
          checkpoint_id: string;
          created_by: string;
          body: string;
          digest: string;
          status: "ready" | "resumed";
        }>(
          `SELECT checkpoint_id, created_by, body, digest, status
           FROM context_checkpoints WHERE checkpoint_id = ? AND task_id = ? AND workspace_id = ?`,
          input.checkpointId,
          input.taskId,
          input.workspaceId,
        )
        .toArray()[0];
      if (previous) {
        if (
          previous.created_by !== input.actorId ||
          previous.body !== input.body ||
          previous.digest !== digest
        ) {
          throw new CheckpointConflict("Checkpoint ID was reused with different content or actor");
        }
        return {
          checkpointId: previous.checkpoint_id,
          status: previous.status,
          digest: previous.digest,
          idempotent: true,
        };
      }

      const ready = this.ctx.storage.sql
        .exec<{ checkpoint_id: string }>(
          `SELECT checkpoint_id FROM context_checkpoints
           WHERE task_id = ? AND status = 'ready'`,
          input.taskId,
        )
        .toArray()[0];
      if (ready) throw new CheckpointConflict("Task already has a ready checkpoint");

      this.ctx.storage.sql.exec(
        `INSERT INTO context_checkpoints(
          checkpoint_id, workspace_id, task_id, created_by, body, digest, status, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, 'ready', ?)`,
        input.checkpointId,
        input.workspaceId,
        input.taskId,
        input.actorId,
        input.body,
        digest,
        now,
      );
      this.ctx.storage.sql.exec(
        `UPDATE tasks SET version = version + 1 WHERE task_id = ?`,
        input.taskId,
      );
      this.ctx.storage.sql.exec(
        `INSERT INTO audit_events(event_id, workspace_id, task_id, actor_id, event_type, checkpoint_id, created_at)
         VALUES (?, ?, ?, ?, 'checkpoint.created', ?, ?)`,
        candidateEventId,
        input.workspaceId,
        input.taskId,
        input.actorId,
        input.checkpointId,
        now,
      );

      return {
        checkpointId: input.checkpointId,
        status: "ready",
        digest,
        idempotent: false,
      };
    });
  }

  async resumeCheckpoint(input: ResumeInput): Promise<CheckpointResult> {
    validateIds(input);
    const now = new Date().toISOString();
    const eventId = crypto.randomUUID();

    return this.ctx.storage.transactionSync(() => {
      const membership = this.ctx.storage.sql
        .exec<{ role: Role; revoked: number }>(
          `SELECT role, revoked FROM workspace_members
           WHERE workspace_id = ? AND user_id = ?`,
          input.workspaceId,
          input.actorId,
        )
        .toArray()[0];
      if (!membership || membership.revoked || membership.role === "viewer") {
        throw new CheckpointForbidden("Actor cannot resume checkpoints in this workspace");
      }

      const checkpoint = this.ctx.storage.sql
        .exec<{
          checkpoint_id: string;
          created_by: string;
          digest: string;
          status: "ready" | "resumed";
        }>(
          `SELECT checkpoint_id, created_by, digest, status FROM context_checkpoints
           WHERE checkpoint_id = ? AND task_id = ? AND workspace_id = ?`,
          input.checkpointId,
          input.taskId,
          input.workspaceId,
        )
        .toArray()[0];
      if (!checkpoint) throw new CheckpointNotFound("Checkpoint does not exist in this workspace");
      if (checkpoint.status === "resumed") {
        if (checkpoint.created_by !== input.actorId) {
          throw new CheckpointConflict("Checkpoint was already resumed by another actor");
        }
        return {
          checkpointId: checkpoint.checkpoint_id,
          status: "resumed",
          digest: checkpoint.digest,
          idempotent: true,
        };
      }

      this.ctx.storage.sql.exec(
        `UPDATE context_checkpoints SET status = 'resumed', resumed_at = ?
         WHERE checkpoint_id = ? AND status = 'ready'`,
        now,
        input.checkpointId,
      );
      this.ctx.storage.sql.exec(`UPDATE tasks SET version = version + 1 WHERE task_id = ?`, input.taskId);
      this.ctx.storage.sql.exec(
        `INSERT INTO audit_events(event_id, workspace_id, task_id, actor_id, event_type, checkpoint_id, created_at)
         VALUES (?, ?, ?, ?, 'checkpoint.resumed', ?, ?)`,
        eventId,
        input.workspaceId,
        input.taskId,
        input.actorId,
        input.checkpointId,
        now,
      );

      return {
        checkpointId: checkpoint.checkpoint_id,
        status: "resumed",
        digest: checkpoint.digest,
        idempotent: false,
      };
    });
  }

  /** Test fixture read; this is not exposed by the Worker. */
  inspectFixture(taskId: string): {
    version: number;
    readyCount: number;
    auditCount: number;
    digest: string | null;
  } {
    const task = this.ctx.storage.sql
      .exec<{ version: number }>(`SELECT version FROM tasks WHERE task_id = ?`, taskId)
      .toArray()[0];
    const ready = this.ctx.storage.sql
      .exec<{ count: number }>(
        `SELECT COUNT(*) AS count FROM context_checkpoints WHERE task_id = ? AND status = 'ready'`,
        taskId,
      )
      .toArray()[0];
    const audit = this.ctx.storage.sql
      .exec<{ count: number }>(
        `SELECT COUNT(*) AS count FROM audit_events WHERE task_id = ?`,
        taskId,
      )
      .toArray()[0];
    const checkpoint = this.ctx.storage.sql
      .exec<{ digest: string }>(
        `SELECT digest FROM context_checkpoints WHERE task_id = ? AND status = 'ready'`,
        taskId,
      )
      .toArray()[0];
    return {
      version: task?.version ?? -1,
      readyCount: ready?.count ?? 0,
      auditCount: audit?.count ?? 0,
      digest: checkpoint?.digest ?? null,
    };
  }
}

function validate(input: CheckpointInput): void {
  validateIds(input);
  if (input.body.trim().length === 0 || new TextEncoder().encode(input.body).byteLength > 32 * 1024) {
    throw new CheckpointInvalid("Invalid checkpoint payload");
  }
}

function validateIds(input: Pick<CheckpointInput, "workspaceId" | "taskId" | "checkpointId" | "actorId">): void {
  if (
    !/^[0-9a-f-]{36}$/i.test(input.workspaceId) ||
    !/^[0-9a-f-]{36}$/i.test(input.taskId) ||
    !/^[0-9a-f-]{36}$/i.test(input.checkpointId) ||
    !/^[0-9a-f-]{36}$/i.test(input.actorId)
  ) {
    throw new CheckpointInvalid("Invalid checkpoint payload");
  }
}

async function sha256(value: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}
