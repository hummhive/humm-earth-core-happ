# RC hApp futureproofing — Unyt / Holo-Hosting readiness + platform posture (rev 1)

- **Status:** OPEN — research captured, plan pending. This file is the living
  plan: update it at each phase of learning / decision / execution, like its
  sibling `pass-7-integrity-candidates.md`.
- **Sibling:** `pass-7-integrity-candidates.md` (the integrity-fork batch).
  Nothing in THIS file forks the DNA — every candidate here is coordinator-only
  or lives outside the DNA entirely. If that ever changes for an item, it moves
  to the pass-7 catalogue, not here.
- **Origin:** owner directive 2026-07-17 ("futureproof our RC happ") + full
  read of the two repos in `../unyt-holohost/` (just cloned:
  `smart_agreement_library`, `unyt-sandbox`) by two source-reading agents.
  Citations refer to those repos at their 2026-07-17 checkout.

## 0. Ecosystem orientation (read this first — the names are confusing)

This ecosystem is fragmented in ways that trip up even people deep in it.
Plain-language map of every player this file mentions:

- **Holochain** — the framework we build on. Each app ("hApp") is its own
  peer-to-peer network. There is no global chain; each user signs their own
  history ("source chain") and peers validate each other's entries in a shared
  content-addressed store (the DHT).
- **DNA / integrity zome / coordinator zome** — a hApp's network identity is
  the hash of its validation rules (the DNA, produced by the integrity zome).
  Change the rules → new hash → a DIFFERENT network (that's why integrity
  changes are "forks" and need migrations). The coordinator zome is just the
  callable API on top and can be swapped without forking. This split is OUR
  load-bearing rule and, notably, Unyt converged on the same posture
  (they moved their execution engine "to coordinator from the integrity zome"
  after a security review — unyt-infra-marketplace CHANGELOG).
- **kitsune / tx5 / iroh** — transport layers, easily confused:
  - *kitsune* — Holochain's peer-gossip layer (how DHT data spreads). We ride
    whatever our pinned holochain (0.6.x) ships.
  - *tx5* — Holochain's WebRTC-based networking stack. Unyt tested BOTH tx5
    and iroh and currently ships tx5 (`unyt-sandbox/testing_docs/5_0…md:73`).
  - *iroh* — an independent blob-transfer/networking stack. OUR direction for
    big binary content (humm-tauri's persistent-blob keystone). Unyt's
    marketplace changelog also shows an iroh experiment. These choices are
    per-app plumbing, not a shared contract — nothing forces alignment.
- **Holo Hosting** — a hosting business model from the Holochain founders'
  circle: always-on machines ("HoloPorts" / edge nodes) host Holochain apps
  for web users who don't run their own node. Distinct from Holochain itself.
- **Unyt** — a peer-to-peer accounting/billing platform (its own hApp, built
  by ex-Holochain/Holo-Hosting founders) meant to meter and settle payments
  for services — INCLUDING Holo-style hosting. `../unyt-holohost/` holds its
  release wrapper (`unyt-sandbox`) and its agreement-template library
  (`smart_agreement_library`).
- **Smart Agreement / RAVE** — Unyt's billing logic unit. An agreement is
  DATA (a Rhai script + JSON schemas) published into a running Unyt network at
  runtime — NOT compiled into any DNA. Executing one produces a RAVE
  ("Recorded Agreement, Verifiably Executed") — the receipt.
- **Bridge Agent** — Unyt's integration pattern: an authorized agent process
  that WATCHES an external system (another DHT, an EVM chain) and mirrors
  events into the Unyt network. Integration happens BESIDE apps, not inside
  them.

## 1. Mission frame (what "futureproof" means for this repo)

Owner's posture (2026-07-17): the mid-2000s Mozilla extension mentality. The
user installs it, the user owns it, no DRM, removable at will. We are the
layer that lets a person interface with the Holochain network efficiently,
robustly, and safely — kitsune-gossiped structured content today, iroh-carried
blob data next. humm-tauri's `PHILOSOPHY.md` formalizes this (Sovereign
Accountable Commons, Walkaway, Ship Blank — the host ships empty, discovery is
out-of-band, so the platform-operator liability class never attaches to us).

The RC bar, in that frame:
- **Nobody can break it** for other users (validator soundness, bounded
  reads, no unvalidated growth surfaces — the pass-7 §A hardening batch).
- **Nobody can beat it** by forking value out of it (the data is the user's;
  walkaway is a feature, not a threat).
- **Nobody can "make it more correct"** — the validation floor protects every
  user; only the user's own choices can degrade their experience.

## 2. What Unyt/Holo-Hosting actually needs from us (finding: almost nothing, by design)

The single most important finding, double-confirmed from both repos:
**Unyt never hosts, embeds, or calls a third-party hApp.** It ships one
self-contained DNA (`alliance.dna`) per app build; billing logic is runtime
data; cross-system integration is always an external Bridge Agent watching two
networks (`unyt-sandbox/README.md:30-37`, `scripts/inherit-ui-release.sh:41-47`,
`release_docs/blockchain_bridging_v0.54.0.md`). Consequences:

- **Zero integrity-fork requirements found.** Nothing in either repo needs
  our DNA hash to move. (Full grep sweeps for `countersign`, `cap_grant`,
  `membrane_proof`, `PreflightRequest` across both repos: zero hits.)
- If HummHive hosting/serving is ever billed through Unyt's
  `holo_hosting_proof_of_service` agreement, the shape is: some process (their
  "Log Harvester" role) reads serving metrics and submits
  `invoice_payloads: [{host_pub_key, logs:[{s,g,gt,wt1,wt2}]}]` into the Unyt
  app, where the funder must be the sole `AuthorizedExecutor`
  (`smart_agreement_library/library/holo_hosting_proof_of_service/`). That
  harvester is an app-side adapter (humm-tauri / sidecar territory), NOT a
  zome obligation.
- Their Send→Accept settlement is two independently-authored entries, not a
  Holochain countersigning session — so no countersigning surface is being
  asked of us either. (Our own countersigned-receipts interest stays parked in
  pass-7 scoping notes, on its own merits.)

## 3. Candidate work items (all coordinator-only or app-side; none scheduled)

### F1. Service-proof read surface (COORDINATOR-ONLY, candidate for a future coordinator generation)
One sentence: make a host's serving activity externally readable in a stable,
bounded way, so any Bridge-Agent/Log-Harvester-style reader can meter it
without us trusting them.
- Most of this EXISTS: pin/provider records are EncryptedContent with
  discovery links, and v3.1.0's bounded page externs
  (`list_by_dynamic_link_page` etc.) are exactly the "stable query surface"
  such a reader needs. `BlobPinSignal` (tag `"pin"`) already broadcasts pin
  events to up to 16 readers.
- The GAP, if billing ever gets real: an append-only, author-signed serving
  COUNTER record (what was served, to whom, how many bytes, when) with a
  deterministic content-id path — i.e. an auditable meter, not just current
  state. Design constraint learned from Unyt: conserved quantities travel as
  exact-arithmetic STRINGS, never floats (their `add_fuel`/`sub_fuel`
  convention) — if we emit billable numbers, emit strings.
- Do NOT build until a counterparty exists; capture only. (YAGNI — but the
  design constraint is cheap to honor from day one.)

### F2. Unyt-alongside provisioning (CLIENT/CONDUCTOR — humm-tauri's, tracked here for the contract only)
If HummHive users ever hold Unyt accounts, Unyt runs as a SECOND installed
app (its own DNA + agent keys), not inside our conductor cell. Our only
contract exposure is F1's read surface. humm-tauri's `09_PROJECT_Payments`
epic is the natural home; nothing for this repo.

### F3. RC hygiene debt (this repo, found during the 2026-07-17 cleanliness sweep)
- **CI tests the wrong suite:** `.github/workflows/test.yaml` runs `npm t` →
  the tryorama harness, which CANNOT boot a conductor on holochain 0.6.x —
  while `crates/sweettest` (28/28 active, the real conductor gate) never runs
  in CI. Fix shape: CI runs host tests + sweettest; tryorama stays dormant
  until the hc-0.7 hop revives it. (Nix pin staleness in that workflow is
  already tracked in `github-release-automation-happ-registry.md` — don't
  double-ticket it.)
- **LICENSE text still missing** (DecraLicense — owner hasn't supplied text;
  RC blocker class, zero wasm impact).
- **D1 GitHub release automation** deferred by owner — batch near RC.

### F4. Toolchain watch items (no action; re-check at pass-7 scoping)
- Unyt's public sibling repo tracks holochain 0.4→0.6.0 — same major line as
  our hc 0.6.1 pin; no compatibility cliff visible today.
- The hc-0.7/S2 stack hop (already the pass-7 fork-ride event, catalogue
  principle 2) is where tryorama revival, kitsune/tx5 changes, and any Unyt
  version alignment all get re-evaluated TOGETHER — one wipe, not two.

## 4. Open questions (blocked on people, not code)

1. **Private source:** the real Unyt zome code lives in the private
   `unytco/unyt` repo (`unyt-sandbox/.gitmodules`; submodule uninitialized,
   `git ls-remote` → not found). Ask the Unyt/Holo-Host contact for: entry-type
   list, cap-grant/membrane-proof usage, countersigning reality of
   Send→Accept, and the concrete Proof-of-Service input schema for
   DNA-hosted services.
2. **Premise check with owner:** is the goal for HummHive nodes to BE
   Holo-style hosts (earn credit for serving), to PAY for hosting, both, or
   neither-yet? F1's priority hangs entirely on this.
3. **Who builds a Log-Harvester adapter** if billing gets real — humm-tauri
   sidecar, standalone process, or Unyt-side tooling?

## 5. Decision log

- 2026-07-17 — File created. Research phase complete (both unyt-holohost
  repos read end-to-end; no integrity-fork requirement found; F1–F4 captured;
  premise question routed to owner). Next: owner /plan session to rank F1/F3
  and answer §4.2.
