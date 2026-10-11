import { env } from "cloudflare:workers";
import { runInDurableObject } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import {
  CheckpointConflict,
  CheckpointForbidden,
  CheckpointNotFound,
  type CheckpointInput,
  type ResumeInput,
  type WorkspaceStore,
} from "../src/worker";
import worker from "../src/worker";

const workspaceId = "10000000-0000-4000-8000-000000000001";
const taskId = "20000000-0000-4000-8000-000000000002";
const editorId = "30000000-0000-4000-8000-000000000003";
const viewerId = "40000000-0000-4000-8000-000000000004";

describe("Cloudflare SQLite Durable Object checkpoint spike", () => {
  let store: DurableObjectStub<WorkspaceStore>;

  beforeEach(() => {
    const objectId = env.WORKSPACES.idFromName(crypto.randomUUID());
    store = env.WORKSPACES.get(objectId);
  });

  it("commits checkpoint, task version, and audit event as one operation", async () => {
    const input = makeInput(crypto.randomUUID());
    const result = await runInDurableObject(store, async (instance) => {
      instance.seedFixture(workspaceId, taskId, editorId, "editor");
      return instance.createCheckpoint(input);
    });
    const state = await inspect(store);

    expect(result.status).toBe("ready");
    expect(result.idempotent).toBe(false);
    expect(result.digest).toMatch(/^[a-f0-9]{64}$/);
    expect(state).toEqual({ version: 1, readyCount: 1, auditCount: 1, digest: result.digest });
  });

  it("returns the existing result on an identical retry without a second event", async () => {
    const input = makeInput(crypto.randomUUID());
    const results = await runInDurableObject(store, async (instance) => {
      instance.seedFixture(workspaceId, taskId, editorId, "editor");
      const first = await instance.createCheckpoint(input);
      const retry = await instance.createCheckpoint(input);
      return [first, retry] as const;
    });
    const state = await inspect(store);

    expect(results[0].idempotent).toBe(false);
    expect(results[1]).toEqual({ ...results[0], idempotent: true });
    expect(state).toEqual({ version: 1, readyCount: 1, auditCount: 1, digest: results[0].digest });
  });

  it("resumes atomically, makes the same actor's retry idempotent, then allows a new checkpoint", async () => {
    const original = makeInput(crypto.randomUUID());
    const resume: ResumeInput = {
      workspaceId,
      taskId,
      checkpointId: original.checkpointId,
      actorId: editorId,
    };
    const results = await runInDurableObject(store, async (instance) => {
      instance.seedFixture(workspaceId, taskId, editorId, "editor");
      const created = await instance.createCheckpoint(original);
      const resumed = await instance.resumeCheckpoint(resume);
      const retry = await instance.resumeCheckpoint(resume);
      const createRetryAfterResume = await instance.createCheckpoint(original);
      const next = await instance.createCheckpoint(makeInput(crypto.randomUUID()));
      return { created, resumed, retry, createRetryAfterResume, next };
    });

    expect(results.resumed).toMatchObject({
      checkpointId: original.checkpointId,
      status: "resumed",
      idempotent: false,
      digest: results.created.digest,
    });
    expect(results.retry).toEqual({ ...results.resumed, idempotent: true });
    expect(results.createRetryAfterResume).toEqual({ ...results.created, status: "resumed", idempotent: true });
    expect(results.next.status).toBe("ready");
    expect(await inspect(store)).toEqual({ version: 3, readyCount: 1, auditCount: 3, digest: results.next.digest });
  });

  it("rolls back all writes if a transaction fails before commit", async () => {
    const checkpointId = crypto.randomUUID();
    await runInDurableObject(store, (instance) => {
      instance.seedFixture(workspaceId, taskId, editorId, "editor");
    });

    await expect(
      runInDurableObject(store, (_instance, state) =>
        state.storage.transactionSync(() => {
          state.storage.sql.exec(
            `INSERT INTO context_checkpoints(
              checkpoint_id, workspace_id, task_id, created_by, body, digest, status, created_at
            ) VALUES (?, ?, ?, ?, 'synthetic', 'digest', 'ready', '2026-10-11T00:00:00Z')`,
            checkpointId,
            workspaceId,
            taskId,
            editorId,
          );
          state.storage.sql.exec(`UPDATE tasks SET version = version + 1 WHERE task_id = ?`, taskId);
          state.storage.sql.exec(
            `INSERT INTO audit_events(event_id, workspace_id, task_id, actor_id, event_type, checkpoint_id, created_at)
             VALUES (?, ?, ?, ?, 'checkpoint.created', ?, '2026-10-11T00:00:00Z')`,
            crypto.randomUUID(),
            workspaceId,
            taskId,
            editorId,
            checkpointId,
          );
          throw new Error("injected pre-commit failure");
        }),
      ),
    ).rejects.toThrow("injected pre-commit failure");

    expect(await inspect(store)).toEqual({ version: 0, readyCount: 0, auditCount: 0, digest: null });
  });

  it("serializes concurrent distinct IDs and leaves exactly one ready checkpoint", async () => {
    const inputs = Array.from({ length: 20 }, () => makeInput(crypto.randomUUID()));
    const results = await runInDurableObject(store, async (instance) => {
      instance.seedFixture(workspaceId, taskId, editorId, "editor");
      return Promise.allSettled(inputs.map((input) => instance.createCheckpoint(input)));
    });
    const state = await inspect(store);

    expect(results.filter((item) => item.status === "fulfilled")).toHaveLength(1);
    expect(results.filter((item) => item.status === "rejected")).toHaveLength(19);
    expect(results.filter((item) => item.status === "rejected").every((item) => item.status === "rejected" && item.reason instanceof CheckpointConflict)).toBe(true);
    expect(state.version).toBe(1);
    expect(state.readyCount).toBe(1);
    expect(state.auditCount).toBe(1);
  });

  it("checks current membership inside the same store before writing", async () => {
    const input = makeInput(crypto.randomUUID(), viewerId);
    const outcome = await runInDurableObject(store, async (instance) => {
      instance.seedFixture(workspaceId, taskId, viewerId, "viewer");
      try {
        await instance.createCheckpoint(input);
        return "unexpected-success";
      } catch (error) {
        return error;
      }
    });

    expect(outcome).toBeInstanceOf(CheckpointForbidden);
    expect(await inspect(store)).toEqual({ version: 0, readyCount: 0, auditCount: 0, digest: null });
  });

  it("rejects a revoked editor and a task from another workspace", async () => {
    const revoked = await runInDurableObject(store, async (instance) => {
      instance.seedFixture(workspaceId, taskId, editorId, "editor");
      instance.revokeFixtureMember(workspaceId, editorId);
      try {
        await instance.createCheckpoint(makeInput(crypto.randomUUID()));
        return null;
      } catch (error) {
        return error;
      }
    });
    expect(revoked).toBeInstanceOf(CheckpointForbidden);

    const otherWorkspace = "50000000-0000-4000-8000-000000000005";
    const wrongScope = await runInDurableObject(store, async (instance) => {
      instance.seedFixture(workspaceId, taskId, editorId, "editor");
      instance.seedFixture(otherWorkspace, taskId, editorId, "editor");
      try {
        await instance.createCheckpoint({ ...makeInput(crypto.randomUUID()), workspaceId: otherWorkspace });
        return null;
      } catch (error) {
        return error;
      }
    });
    expect(wrongScope).toBeInstanceOf(CheckpointNotFound);
    expect(await inspect(store)).toEqual({ version: 0, readyCount: 0, auditCount: 0, digest: null });
  });

  it("keeps the spike Worker HTTP surface closed", async () => {
    const response = await worker.fetch(new Request("https://spike.invalid/"), env);
    expect(response.status).toBe(404);
    expect(await response.text()).toBe("Not found");
  });
});

function makeInput(checkpointId: string, actorId = editorId): CheckpointInput {
  return {
    workspaceId,
    taskId,
    checkpointId,
    actorId,
    body: "synthetic checkpoint for transactional storage tests",
  };
}

function inspect(store: DurableObjectStub<WorkspaceStore>) {
  return runInDurableObject(store, (instance) => instance.inspectFixture(taskId));
}
