import type { Env as SpikeEnv } from "./worker";

declare global {
  namespace Cloudflare {
    interface Env extends SpikeEnv {}
  }
}

export {};
