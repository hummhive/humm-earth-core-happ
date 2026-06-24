#!/usr/bin/env node
/**
 * Emits humm-earth-core-happ session context to stdout. Wired as a Claude Code
 * SessionStart hook (.claude/settings.json) and/or an oh-my-pi session_start hook
 * (.omp/hooks/wsl-session-context.ts). Always prints the read-order + change-gravity
 * + hard-rules line; adds the WSL two-clone workflow + a live hApp sha256 check
 * only on a WSL host. Zero dependencies, cross-platform.
 */
import { execSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const out = [];
out.push("# humm-earth-core-happ — session context");
out.push(
	"Read order: `POSTCOMPACTION.md` → `README.md` → `CLAUDE.md` → `AGENTS.md`. Skills: `coding-standards`, `rust-patterns`, `rust-testing`. Map: `docs/CODEMAPS/*`. Terms: `../humm-tauri/GLOSSARY.md`.",
);
out.push(
	"Change gravity: editing the INTEGRITY zome (`zomes/integrity/`) changes the DNA hash and FORKS the chain — only for a sanctioned new pass + migration, never a drive-by. Coordinator (`zomes/coordinator/`) is hot-swappable. Wire shapes: add with `#[serde(default)]`, remove via migration.",
);
out.push(
	"Hard rules: `?`/ExternResult over `.unwrap()` (a panic traps the WASM guest); never silent-swallow (`let _ =`, `if let Err(_)`, `.ok();`, masking `unwrap_or_default`); exhaustive matching (no `_ =>` for business enums); iterators over loops; HDK `debug!`/`warn!` for logs (no LoggingService here); never NIST curves; no `any` in the TS tests; fns ≤~50; commit-local, never push.",
);

const isWsl =
	Boolean(process.env.WSL_DISTRO_NAME) ||
	Boolean(process.env.WSL_INTEROP) ||
	(existsSync("/proc/version") && /microsoft/i.test(readFileSync("/proc/version", "utf8")));

if (isWsl) {
	out.push("");
	out.push("## WSL two-clone workflow (this host)");
	out.push(
		"- Do all dev/build/test in `~/humm-earth-core-happ` (native FS, ~30× faster) — NEVER `/mnt/c/...` (corrupts `target/`). The Windows mount is only the bridge to `origin`.",
	);
	out.push(
		"- Sync with `scripts/wsl-pull.sh` (start) / `scripts/wsl-push.sh` (end) / `scripts/wsl-check.sh`. Never manual cp / format-patch across clones, never `git commit` on the Windows mount with WSL-clone content.",
	);
	out.push(
		"- Allowed scopes are the TWO clones ONLY (`~/humm-earth-core-happ` + `/mnt/c/proj/github/hummhive/humm-earth-core-happ`) — never read/write/list outside them. Subagents default to `~/humm-earth-core-happ`; ast/lsp take absolute `~/humm-earth-core-happ/...` paths.",
	);
	out.push(
		"- Build inside nix: `nix develop --command bash -c 'npm run build:zomes && hc app pack workdir --recursive'`. Reproducible build → deterministic DNA hash.",
	);

	const happ = join(homedir(), "humm-earth-core-happ", "workdir", "humm-earth-core-happ.happ");
	if (!existsSync(happ)) {
		out.push(
			"- ⚠️ `workdir/humm-earth-core-happ.happ` not built yet — run the nix build above before packing / deploying or comparing the hApp sha.",
		);
	} else {
		try {
			const sha = execSync(`sha256sum "${happ}"`, { encoding: "utf8" }).trim().split(/\s+/)[0];
			out.push(
				`- Built hApp sha256 \`${sha.slice(0, 12)}…\` — confirm it matches the current-generation row in \`~/hummhive-official-happ-versions/MANIFEST.tsv\` (current line: pass-4, DNA \`uhC0k26b…\`) before deploying to \`../humm-tauri/src-tauri/bin/\`.`,
			);
		} catch {
			// best-effort: a failed sha probe must never break session startup.
			out.push("- (hApp sha check skipped — couldn't read the binary; verify manually before deploy.)");
		}
	}
}

process.stdout.write(`${out.join("\n")}\n`);
