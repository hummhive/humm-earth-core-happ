import type { HookAPI } from "@oh-my-pi/pi-coding-agent/extensibility/hooks";

/**
 * Injects humm-earth-core-happ session context at session start by running the
 * shared `hooks/session-context.mjs` generator, so the oh-my-pi, Claude Code,
 * and Codex wirings stay DRY (one source for the dynamic/host-conditional
 * context). The script self-detects WSL and adds the two-clone workflow + hApp
 * sha check there.
 *
 * The static hard-rules digest lives in the alwaysApply `repo-standards` rule;
 * this hook adds only the dynamic part.
 *
 * Placement: omp's native hook discovery scans ONLY `<cfg>/hooks/pre/` and
 * `<cfg>/hooks/post/` (discovery/builtin.ts: hookTypes = ["pre","post"]), never
 * flat `hooks/` — so this file MUST live in `.omp/hooks/pre/` to be auto-loaded.
 * The pre/post folder is just the discovery location; the loader registers
 * whatever `pi.on(...)` declares (here `session_start`).
 *
 * `pi.exec` signature is `(command, args[], options)` — verified against
 * src/extensibility/hooks/types.ts (HookAPI.exec).
 */
export default function wslSessionContext(pi: HookAPI): void {
	pi.on("session_start", async (_event, ctx) => {
		try {
			const result = await pi.exec("node", ["hooks/session-context.mjs"], { cwd: ctx.cwd });
			const text = String(result?.stdout ?? "").trim();
			if (text.length > 0) {
				pi.sendMessage({ customType: "humm-session-context", content: text, display: true });
			}
		} catch {
			// best-effort context injection; never block a session on it.
		}
	});
}
