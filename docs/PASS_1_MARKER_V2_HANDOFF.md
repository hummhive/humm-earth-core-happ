# Pass-1 marker-v2 fixture handoff — humm-tauri E2E

Short-form handoff for the humm-tauri team. This is a **coordinator-only**
rebuild of the pass-1 fixture that adds the V2 migration-marker externs
(`mark_migrated_v2`, `get_migration_marker_v2`) while keeping the integrity
DNA byte-identical to pass-1. It exists to unblock the two skipped
`tests/e2e/specs/10-migration-runner.e2e.ts` cases (the `mark_hive_migrated`
marker step + the cross-peer poller) that resolve `MigrationMarkerV2` on the
pass-1 SOURCE cell.

For the marker security model (defenses A/B/C, forge resistance, the
`_migrated/` sentinel), the canonical reference is
[`DNA_MIGRATION_GUIDE.md`](./DNA_MIGRATION_GUIDE.md). This doc only covers
the V2 delta and the fixture.

## Source-of-truth state

**hApp branch:** `feat-pass1-coordinator-marker-v2` off `a10a4ba` (pass-1).

| Commit | Scope |
|---|---|
| `28d7012` | `feat(coordinator)`: V2 marker externs (the build commit — MANIFEST/README pin this exact ref) |
| _(tip)_ | `docs`: this handoff doc (inert to the build) |

**DNA hash:** `uhC0kb0T3Lrh0sILx9hx6oCtcSMdtGM_et0jIUoahIWCUoA1ZYHLE` —
**held byte-identical** to pass-1. Coordinator-only; no integrity zome
touched; no user wipe. `content_integrity.wasm` sha256 is unchanged
(`636b4ef5…`); only `content.wasm` changed (the two added externs).

## Artifact (`~/hummhive-official-happ-versions/` + `MANIFEST.tsv` row `pass-1-marker-v2`)

```text
filename:         humm-earth-core-happ_pass-1-marker-v2_dna-uhC0kb0T3Lrh_happ-0e6baaea.happ
dna_hash:         uhC0kb0T3Lrh0sILx9hx6oCtcSMdtGM_et0jIUoahIWCUoA1ZYHLE   (HELD, == pass-1)
integrity_sha256: 636b4ef50beb957ed260de39a8677230c5a2707c617f4f4e05c13111b1cf895f (HELD, == pass-1)
content_sha256:   e5e7c9b6d0bbe9428e6e239b030cba1b72371ba10645c4f3341680954ffc34f3 (new — V2 externs)
happ_sha256:      0e6baaead4b28bb59483dabc5d321e326c9b88475deb9145eab1e992ca24d624
size:             791087 bytes
```

## The two new externs — exact wire shapes

Both mirror the pinned surface humm-tauri already encodes/decodes in
`src-tauri/src/migration/wire.rs` (`MigrationMarkerV2`, `MarkMigratedV2Input`)
and `getMigrationMarkerV2*` / `decodeMigrationMarker` on the TS side.

### `mark_migrated_v2` (write, local-only)

```rust
// input (msgpack struct-map, snake_case)
MarkMigratedV2Input {
    original_action_hash: ActionHash,
    marker: MigrationMarkerV2,
}
// returns
Option<EncryptedContentResponse>   // Ok(None)+warn! if the original entry
                                   // is unresolvable (dormant/absent cell)
```

NOT in `set_cap_tokens` — local-only by design (a remote-callable writer
would let a peer pollute another agent's chain + fan out a spurious
migration signal in their name). Reachable from the local UI via the
conductor's AppWebsocket auth, exactly like `update_encrypted_content`.

### `get_migration_marker_v2` (read, cap-granted)

```rust
// input
ActionHash                         // the original entry's action hash
// returns
Option<MigrationMarker>            // externally-tagged: {"V1": {...}} | {"V2": {...}}
```

Granted in `set_cap_tokens` beside the V1 reader — it walks already-public
DHT data and applies the same author-binding filter (only updates authored
by the original entry's author count as valid markers), so it does not rely
on the cap surface for forge resistance.

### `MigrationMarkerV2`

```rust
MigrationMarkerV2 {
    schema_tag: String,                 // == "humm-earth-core-happ/migration-marker"
    schema_version: u32,                // == 2
    new_dna_hash_base64: String,
    new_action_hash_base64: String,
    new_app_id: String,
    migrated_at_microseconds: i64,
    #[serde(default)] new_hive_genesis_hash_base64: Option<String>,
    #[serde(default)] new_hive_genesis_display_id: Option<String>,
}
```

The two `#[serde(default)]` tail fields are the only additions over V1; they
carry the hive-identity continuity info (which new-DNA `HiveGenesis` members
should join, and its display alias). `None` for per-entry content markers.

### Cross-version decode contract (load-bearing)

- `MigrationMarker` is the reader enum. `decode_marker` tries **V2 first**,
  falls back to V1, returns `None` if neither is well-formed. V2 readers
  therefore see **every** V1 marker.
- A `schema_version == 1` payload decodes into the V2 struct (via
  `#[serde(default)]`) but fails `MigrationMarkerV2::is_well_formed`, so it
  is returned as the `V1` variant — never a malformed V2.
- A `schema_version == 2` payload decodes into the V1 struct (msgpack
  struct-map ignores unknown fields) but fails `MigrationMarkerV1::is_well_formed`,
  so pre-V2 readers see V2 markers as `Ok(None)`.
- Wire tagging is serde external tagging (`{"V1": …}` / `{"V2": …}`); a unit
  test pins the msgpack 1-fixmap first byte so a switch to internal tagging
  is a deliberate breaking change, not an accident.

## What changed structurally

The flat `encrypted_content/migration.rs` became a `migration/` module
(`markers` / `payload` / `writers` / `readers` / `tests`) matching the
canonical layout. Export-table diff vs the original published pass-1 `.happ`:
**added exactly** `["get_migration_marker_v2", "mark_migrated_v2"]`,
**removed nothing** (all 27 original externs intact; 29 total). V1
`mark_migrated` / `get_migration_marker` behavior is unchanged.

## Verification (evidence)

- Reproducible overlay (RUSTFLAGS remap + `codegen-units=1` + binaryen-123
  `--strip-debug --strip-producers`), holonix `main-0.6` / hc 0.6.0 / rustc 1.88.
- The UNMODIFIED `a10a4ba` tree was rebuilt first and reproduced all four
  pinned pass-1 hashes exactly (integrity `636b4ef5`, DNA `uhC0kb0T3Lrh`,
  content `780502de`, happ `63921f6b`) — the pipeline was proven before editing.
- After the V2 change: `content_integrity.wasm` sha256 is still `636b4ef5`
  (byte-identical, cross-checked against the original published `.happ`'s
  unpacked integrity wasm) → DNA held.

## Provisioning + wiring (humm-tauri side)

```bash
cp ~/hummhive-official-happ-versions/humm-earth-core-happ_pass-1-marker-v2_dna-uhC0kb0T3Lrh_happ-0e6baaea.happ .testdata/happs/
cp ~/hummhive-official-happ-versions/MANIFEST.tsv .testdata/happs/
```

Repoint `HAPP_PATH_PASS_1` in `tests/e2e/paths.ts` to the new filename.
The fixture is a strict superset of the old pass-1 (same DNA, all prior
externs, +2), so every existing pass-1 staged-flow test keeps passing on it
— no separate const required. If you prefer to keep the base pinned, add a
`HAPP_PATH_PASS_1_MARKER_V2` const mirroring the pass-4 base/rescue split.

## Caveats

1. **Marker write + read both happen on the pass-1 source cell**, so the two
   unblocked tests are INDEPENDENT of the `PASS_2_DNA_HASH_BASE64`-vs-rebuilt-
   pass-2 divergence caveat (`.testdata/happs/README.md`) — that only bites
   full receiver resolution ONTO pass-2. The marker just carries whatever
   `new_dna_hash_base64` the runner stamps.
2. **MANIFEST commit `28d7012` is a local branch, not yet pushed** (never-push
   policy). The `.happ` is provided + hash-pinned, so tests don't depend on the
   push; but independent rebuild-from-source verification needs the maintainer
   to push `feat-pass1-coordinator-marker-v2`.
