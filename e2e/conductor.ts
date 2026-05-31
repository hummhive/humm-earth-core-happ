/**
 * Manual single-conductor e2e harness for the pass-4 humm_earth_core DNA.
 *
 * Why this exists: the published `@holochain/tryorama` (0.19.2, latest)
 * spawns conductors via `hc sandbox create network quic …`, but the
 * installed holochain 0.6.0 CLI removed the `quic` transport subcommand
 * (now `mem` / `webrtc`). Tryorama therefore cannot launch a conductor
 * in this toolchain, and no newer tryorama is published. This harness
 * sidesteps tryorama entirely: it boots a real holochain 0.6.0 conductor
 * with a fresh in-process lair keystore + a throwaway `--data-dir`,
 * installs the packed pass-4 .happ for N agents on ONE conductor (so
 * every agent shares a single DHT and cross-agent validation works
 * offline, with no bootstrap/signal networking), and drives it over the
 * normal AppWebsocket — exactly the surface humm-tauri uses.
 *
 * The conductor + keystore are created under a unique temp dir and
 * destroyed on teardown; nothing touches the repo's `.hc` sandboxes or
 * the developer's real conductor state.
 */
import { spawn, type ChildProcess } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import net from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  AdminWebsocket,
  AppWebsocket,
  encodeHashToBase64,
  type AgentPubKey,
  type CellId,
} from "@holochain/client";

export const ROLE_NAME = "humm_earth_core";
export const ZOME = "content";

/** Shared network seed so every installed agent's cell lands on the
 * SAME DHT within the single conductor — the whole point of the
 * harness (offline cross-agent `must_get_valid_record`). */
const NETWORK_SEED = "humm-pass4-e2e";

const HAPP_PATH =
  process.env.HUMM_HAPP_PATH ??
  join(process.cwd(), "workdir", "humm-earth-core-happ.happ");

const HOLOCHAIN_BIN = process.env.HOLOCHAIN_BIN ?? "holochain";

/** A connected agent on the shared conductor. */
export type Agent = {
  name: string;
  appWs: AppWebsocket;
  cellId: CellId;
  agentPubKey: AgentPubKey;
  /** Alias of `agentPubKey` (raw holohash bytes). */
  key: AgentPubKey;
  /** Multibase string form of `agentPubKey` (for header pubkey fields). */
  b64: string;
  /** Call a `content` zome function as this agent. */
  call: <T = unknown>(fnName: string, payload: unknown) => Promise<T>;
};

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.once("error", reject);
    srv.listen(0, "127.0.0.1", () => {
      const addr = srv.address();
      if (addr && typeof addr === "object") {
        const { port } = addr;
        srv.close(() => resolve(port));
      } else {
        srv.close(() => reject(new Error("could not resolve a free port")));
      }
    });
  });
}

/** Reject if `p` does not settle within `ms` — used only for the
 * admin-connect retry loop so a hung handshake cannot wedge boot. */
function withTimeout<T>(p: Promise<T>, ms: number): Promise<T> {
  let id: ReturnType<typeof setTimeout>;
  return Promise.race([
    p.finally(() => clearTimeout(id)),
    new Promise<T>((_, rej) => {
      id = setTimeout(() => rej(new Error(`timeout after ${ms}ms`)), ms);
    }),
  ]);
}

function conductorConfig(rootDir: string, adminPort: number): string {
  // Mirrors the layout `hc sandbox generate` produces on holochain
  // 0.6.0 (verified), with a FIXED admin port so the harness connects
  // deterministically. Network points at the public dev signal/bootstrap
  // (unused for a single conductor's intra-process peers, but kept so the
  // NetworkConfig parses identically to the tool-generated one).
  return [
    "tracing_override: null",
    `data_root_path: ${rootDir}/data`,
    "keystore:",
    "  type: lair_server_in_proc",
    `  lair_root: ${rootDir}/ks`,
    "admin_interfaces:",
    "- driver:",
    "    type: websocket",
    `    port: ${adminPort}`,
    "    danger_bind_addr: null",
    "    allowed_origins: '*'",
    "network:",
    "  base64_auth_material: null",
    "  bootstrap_url: https://dev-test-bootstrap2.holochain.org/",
    "  signal_url: wss://dev-test-bootstrap2.holochain.org/",
    "  webrtc_config: null",
    "  target_arc_factor: 1",
    "  report: none",
    "  advanced:",
    "    tx5Transport:",
    "      signalAllowPlainText: true",
    "request_timeout_s: 60",
    "db_sync_strategy: Resilient",
    "tracing_scope: null",
    "",
  ].join("\n");
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export class E2EConductor {
  private proc: ChildProcess | null = null;
  private admin: AdminWebsocket | null = null;
  private rootDir = "";
  private adminPort = 0;
  private appPort = 0;
  private readonly appWsockets: AppWebsocket[] = [];
  private readonly origin = "humm-e2e";

  /** Boot the conductor + bind the admin interface. */
  async start(): Promise<void> {
    this.rootDir = await mkdtemp(join(tmpdir(), "humm-e2e-"));
    // holochain does not create these for us; lair init + the conductor
    // DB open both fail silently (process exits early) if absent.
    await mkdir(join(this.rootDir, "data"), { recursive: true });
    await mkdir(join(this.rootDir, "ks"), { recursive: true });
    this.adminPort = await freePort();
    const configPath = join(this.rootDir, "conductor-config.yaml");
    await writeFile(configPath, conductorConfig(this.rootDir, this.adminPort), "utf8");

    this.proc = spawn(HOLOCHAIN_BIN, ["--piped", "-c", configPath], {
      detached: true,
      stdio: ["pipe", "pipe", "pipe"],
    });
    // Empty passphrase for the throwaway in-proc lair.
    this.proc.stdin?.end("\n");

    let booted = false;
    let exitedEarly: number | null = null;
    let log = "";
    let reportedPort: number | null = null;
    const onData = (buf: Buffer) => {
      const s = buf.toString("utf8");
      log += s;
      const m = s.match(/###ADMIN_PORT:(\d+)###/);
      if (m) reportedPort = Number(m[1]);
      if (s.includes("Conductor ready")) booted = true;
    };
    this.proc.stdout?.on("data", onData);
    this.proc.stderr?.on("data", onData);
    this.proc.on("exit", (code) => {
      if (!booted) exitedEarly = code ?? -1;
    });

    // Wait for "Conductor ready" AND a successful admin connection.
    // The conductor echoes the bound admin port as `###ADMIN_PORT:N###`;
    // trust that over the configured value in case holochain ever
    // re-binds. Each connect attempt is time-boxed so a hung handshake
    // cannot wedge the boot loop.
    const deadline = Date.now() + 90_000;
    while (Date.now() < deadline && exitedEarly === null) {
      if (booted) {
        const port = reportedPort ?? this.adminPort;
        try {
          this.admin = await withTimeout(
            AdminWebsocket.connect({
              url: new URL(`ws://127.0.0.1:${port}`),
              // Admin iface rejects the WS upgrade with 400 unless an
              // Origin header is present; any value is accepted because
              // the config sets allowed_origins: '*'.
              wsClientOptions: { origin: this.origin },
            }),
            5_000,
          );
          this.adminPort = port;
          break;
        } catch {
          // admin interface not accepting yet — retry
        }
      }
      await sleep(500);
    }
    if (!this.admin) {
      await this.stop();
      const why =
        exitedEarly !== null
          ? `conductor exited early (code ${exitedEarly})`
          : "conductor did not become reachable within 90s";
      throw new Error(`${why}\n--- conductor log ---\n${log.slice(-2000)}`);
    }
    this.appPort = (
      await this.admin.attachAppInterface({ allowed_origins: this.origin })
    ).port;
  }

  /** Generate a fresh agent, install + enable the pass-4 happ for it on
   * the shared DHT, and return a connected `Agent` handle. */
  async addAgent(name: string): Promise<Agent> {
    if (!this.admin) throw new Error("conductor not started");
    const agentPubKey = await this.admin.generateAgentPubKey();
    const installedAppId = `humm-${name}`;
    await this.admin.installApp({
      source: { type: "path", value: HAPP_PATH },
      agent_key: agentPubKey,
      installed_app_id: installedAppId,
      network_seed: NETWORK_SEED,
    });
    await this.admin.enableApp({ installed_app_id: installedAppId });

    const issued = await this.admin.issueAppAuthenticationToken({
      installed_app_id: installedAppId,
    });
    const appWs = await AppWebsocket.connect({
      token: issued.token,
      url: new URL(`ws://127.0.0.1:${this.appPort}`),
      wsClientOptions: { origin: this.origin },
    });
    this.appWsockets.push(appWs);

    const info = await appWs.appInfo();
    if (!info) throw new Error(`appInfo() null for ${installedAppId}`);
    const cell = info.cell_info[ROLE_NAME]?.find(
      (c): c is { type: "provisioned"; value: { cell_id: CellId } } =>
        (c as { type: string }).type === "provisioned",
    );
    if (!cell) {
      throw new Error(
        `no provisioned cell for role "${ROLE_NAME}" in ${installedAppId}`,
      );
    }
    const cellId = cell.value.cell_id;
    // Grant + store full-access signing credentials for this cell so
    // the AppWebsocket can sign zome calls (otherwise callZome throws
    // NoSigningCredentialsForCell). Credentials are held in the client's
    // process-global store keyed by cellId.
    await this.admin.authorizeSigningCredentials(cellId);
    return {
      name,
      appWs,
      cellId,
      agentPubKey,
      key: agentPubKey,
      b64: encodeHashToBase64(agentPubKey),
      call: <T,>(fnName: string, payload: unknown) =>
        appWs.callZome({
          cell_id: cellId,
          zome_name: ZOME,
          fn_name: fnName,
          payload,
        }) as Promise<T>,
    };
  }

  /** Tear down: close every socket, kill the conductor process group,
   * and delete the throwaway data dir + keystore. */
  async stop(): Promise<void> {
    for (const ws of this.appWsockets) {
      try {
        await ws.client.close();
      } catch {
        /* already closed */
      }
    }
    try {
      await this.admin?.client.close();
    } catch {
      /* already closed */
    }
    if (this.proc?.pid) {
      try {
        // Negative pid → kill the whole process group (holochain + lair).
        process.kill(-this.proc.pid, "SIGKILL");
      } catch {
        try {
          this.proc.kill("SIGKILL");
        } catch {
          /* already gone */
        }
      }
    }
    this.proc = null;
    this.admin = null;
    if (this.rootDir) {
      await rm(this.rootDir, { recursive: true, force: true }).catch(() => {});
    }
  }
}

/** Poll until `getter()` returns a non-null value or the timeout elapses.
 * Used to wait for a write authored by one agent to integrate into the
 * shared DHT so another agent's validator can fetch it. */
export async function waitFor<T>(
  getter: () => Promise<T | null | undefined>,
  { timeoutMs = 15_000, intervalMs = 500 }: { timeoutMs?: number; intervalMs?: number } = {},
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  let last: T | null | undefined;
  while (Date.now() < deadline) {
    last = await getter();
    if (last !== null && last !== undefined) return last;
    await sleep(intervalMs);
  }
  throw new Error("waitFor: condition not met within timeout");
}

export { sleep };
