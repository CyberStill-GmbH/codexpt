import { applyD1Migrations, env as cloudflareEnv } from "cloudflare:test";
import type { D1Migration } from "@cloudflare/vitest-plugin";

const testEnv = cloudflareEnv as unknown as { DB: D1Database; TEST_MIGRATIONS: D1Migration[] };
await applyD1Migrations(testEnv.DB, testEnv.TEST_MIGRATIONS);
