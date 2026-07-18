import type { HookAPI } from "@oh-my-pi/pi-coding-agent/extensibility/hooks";

/**
 * Injects humm-earth-core-happ session context at session start by running the
 * shared `hooks/session-context.mjs` generator, so the oh-my-pi and Claude Code
 * wirings stay DRY (one source for the dynamic/host-conditional context). The
 * script self-detects WSL and adds the two-clone workflow + hApp sha check there.
 *
 * The static hard-rules digest lives in the alwaysApply `repo-standards` rule;
 * this hook adds only the dynamic part.
 *
 * oh-my-pi caveat: the hook subsystem is migrating to the extension runner
 * (docs/hooks.md — `--hook` may alias to `--extension`). Verify the `pi.exec` /
 * `pi.sendMessage` signatures against `src/extensibility/hooks/types.ts` for your
 * build, and register via the extension path if hook discovery has moved.
 */
export default function sessionContext(pi: HookAPI): void {
	pi.on("session_start", async (_event, ctx) => {
		try {
			const result = await pi.exec("node hooks/session-context.mjs", { cwd: ctx.cwd });
			const text = String(result?.stdout ?? result ?? "").trim();
			if (text.length > 0) {
				pi.sendMessage({ customType: "humm-session-context", content: text, display: true });
			}
		} catch {
			// best-effort context injection; never block a session on it.
		}
	});
}
