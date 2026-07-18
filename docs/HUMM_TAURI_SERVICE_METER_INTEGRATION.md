# HummTauri Integration — pass-6-service-meter coordinator generation (v3.3.0)

> **Audience:** humm-tauri developers wiring host metering and node-capacity
> declarations, plus Log Harvester and billing-bridge developers reading them.
> **Status:** v3.3.0 wire contract — coordinator-only hot-swap, DNA HELD.
> **Sanity checks:** good and close-but-wrong statements appear in § 10.

---

## 1. Ecosystem orientation

HummHive nodes already host Holochain data and serve other hive members. A
service-meter record is a host node's own cumulative usage tally for one hive
on one UTC day. A node-spec record is a node's opt-in declaration of its
hardware and system capacity. The node's own agent writes both records when
humm-tauri calls the matching extern. Other hive members and an external Log
Harvester or billing bridge may read them, including Unyt (the Holochain
hosting-payments network).

An attestation is an Ed25519 app signature attached to a node-spec publication.
It ties the claim to a recognized humm-tauri app key. A coordinator hot-swap
replaces the cell's callable API while the integrity zome, DNA hash, and
Holochain network stay unchanged. v3.3.0 takes that route; no migration or new
cell generation follows from these two externs.

A DHT link is an indexed pointer stored in Holochain's distributed hash table
(DHT). These records retain the existing EncryptedContent entry and link
model, so readers call the existing granted query externs.

## 2. Record identity, indexing, and readers

| Record | `content_type` | `content_id` | Payload `schema` | Cardinality |
|---|---|---|---|---|
| Service meter | `hummhive-core-service-meter-v1` | `service-meter-v1:<YYYY-MM-DD>` | `hummhive-service-meter/1` | One entry per author+hive+UTC day |
| Node spec | `hummhive-core-node-spec-v1` | `node-spec-v1` | `hummhive-node-spec/1` | One per author+hive, replaced in place |

Both records set `acl_spec = OpenWrite` targeting the hive. The meter sets
`dynamic_links = [period]`, which creates a day-scoped cross-host listing. The
node spec sets no dynamic links.

Reads ride existing granted externs:

| Read need | Existing extern |
|---|---|
| Resolve the author's fixed record id | `get_by_content_id_link` |
| List meter records for one UTC-day bucket | `list_by_dynamic_link_page` |
| List records by hive and content type | `list_by_hive_link` |

`upsert_service_meter` and `publish_node_spec` are mutators and are not
cap-granted. Call them as the cell owner only. For externally meterable
records, the client MUST set `public_key_acl.reader = ["*"]`.

## 3. Serialized payloads

Both payloads travel as msgpack via Holochain `SerializedBytes`.

```rust
pub struct ServiceMeterSnapshot {
    pub schema: String,
    pub period: String,
    pub counters: BTreeMap<String, String>,
}

pub struct NodeSpecSnapshot {
    pub schema: String,
    pub spec: BTreeMap<String, String>,
    pub declared_at_micros: i64,
    pub verified_by_app_key: Option<String>,
}
```

`ServiceMeterSnapshot.period` carries `"YYYY-MM-DD"`. Each
`ServiceMeterSnapshot.counters` item maps a dimension to a `u128` decimal
string. `NodeSpecSnapshot.declared_at_micros` comes from the client; readers
judge staleness. `NodeSpecSnapshot.verified_by_app_key = None` means the node
self-reported the spec without an app attestation.

### 3.1 Service-meter msgpack-level JSON example

The JSON below mirrors the object before `SerializedBytes` encodes it as
msgpack.

```json
{
  "schema": "hummhive-service-meter/1",
  "period": "2026-07-17",
  "counters": {
    "bytes_served": "10485760",
    "requests": "120"
  }
}
```

### 3.2 Node-spec msgpack-level JSON example

```json
{
  "schema": "hummhive-node-spec/1",
  "spec": {
    "architecture": "x86_64",
    "cpu_cores": "16",
    "memory_mib": "32768"
  },
  "declared_at_micros": 1784304000123456,
  "verified_by_app_key": null
}
```

## 4. Validation bounds

| Wire value | Bound |
|---|---|
| Meter dimensions | At most 16 dimensions |
| Node-spec entries | At most 32 entries |
| Meter and spec keys | 1–64 ASCII-printable characters; none of `\|`, `;`, `=` |
| Meter values | Canonical `u128` decimal strings |
| Spec values | 1–256 characters; no control characters; none of `\|`, `;` |
| `period` | Exactly `NNNN-NN-NN`; month `01`–`12`; day `01`–`31` |

A period is a bucket label and is not calendar-verified. Meter values are
absolute CUMULATIVE totals and integers only. Fractions enter as scaled
integers—milli-units are one convention; floats never enter the wire.

Control characters (newlines, ANSI escapes) are rejected in spec values, but
Unicode format characters such as bidi overrides and zero-width characters
pass. Anything that renders spec values—dashboards, logs, billing tooling—must
sanitize them for display.

Every bound violation fails the whole call with one of these exact errors:

| Condition | Exact error |
|---|---|
| Bad `period` shape or range | `service meter period must be YYYY-MM-DD` |
| More than 16 meter dimensions in the input | `service meter accepts at most 16 counter dimensions` |
| Merged union with the prior record past 16 dimensions | `service meter counter union with the prior record exceeds 16 dimensions` |
| Bad meter key | `service meter counter keys must be 1-64 printable ASCII chars without \| ; =` |
| Non-integer meter value | `service meter counters must be canonical u128 decimal strings` |
| More than 32 spec entries | `node spec accepts at most 32 entries` |
| Bad spec key | `node spec keys must be 1-64 printable ASCII chars without \| ; =` |
| Bad spec value | `node spec values must be 1-256 chars without control characters or \| ;` |

## 5. Extern input and response wire

```rust
pub struct UpsertServiceMeterInput {
    pub hive_genesis_hash: ActionHash,
    pub period: String,
    pub counters: BTreeMap<String, String>,
    pub display_hive_id: String,
    pub revision_author_signing_public_key: String,
    pub public_key_acl: Acl,
}

pub struct NodeSpecAttestation {
    pub app_signing_key_b64: String,
    pub signature_b64: String,
}

pub struct PublishNodeSpecInput {
    pub hive_genesis_hash: ActionHash,
    pub spec: BTreeMap<String, String>,
    pub declared_at_micros: i64,
    pub app_attestation: Option<NodeSpecAttestation>,
    pub display_hive_id: String,
    pub revision_author_signing_public_key: String,
    pub public_key_acl: Acl,
}

pub struct UpsertContentResponse {
    pub response: EncryptedContentResponse,
    pub was_created: bool,
    pub was_updated: bool,
}
```

Both externs return `UpsertContentResponse { response: EncryptedContentResponse, was_created: bool, was_updated: bool }`.
Both flags are false on an idempotent no-op. On a fresh create, `was_updated`
is false.

Every upsert also converges the stored header to the call's values for
`display_hive_id`, `revision_author_signing_public_key`, and `public_key_acl`.
Changing only those fields—for example widening `public_key_acl.reader` to
`["*"]` to opt into external metering—is a real update: `was_updated` is true
even though the counters or spec did not change.

### 5.1 Meter merge

`upsert_service_meter` takes the per-dimension `max(prior,new)` over the union
of keys. A missing key on either side does not erase the other side. Identical
merged counters produce a no-op with no write.

### 5.2 Node-spec replacement

`publish_node_spec` applies REPLACE semantics. The new `spec` map replaces the
prior map; fields omitted by the new map disappear. An identical snapshot
produces a no-op with no write.

## 6. App-attestation handshake

The signature covers the UTF-8 bytes of this canonical string with Ed25519:

```text
hummhive-node-spec/1|<author_agent_b64>|<declared_at_micros>|k1=v1;k2=v2;...
```

Keys appear in ascending order. `author_agent_b64` is the caller's
`agent_initial_pubkey.to_string()`. The app key wire form is an Ed25519 key
encoded as an `AgentPubKey` b64 string. `signature_b64` is base64 of the raw
64-byte Ed25519 signature.

A worked canonical string with a sorted map:

```text
hummhive-node-spec/1|uhCAkBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcH|1784304000123456|architecture=x86_64;cpu_cores=16;memory_mib=32768
```

> **Wire warning:** Sign exactly these bytes: `utf8(canonical_string)`. Do not
> serialize them first. No msgpack/SerializedBytes wrapping, no length prefix,
> no trailing newline. The zome passes the raw UTF-8 bytes to
> `verify_signature_raw`; a signature over a msgpack-encoded string NEVER
> verifies.

The humm-tauri build pipeline mints and guards the Ed25519 app key. The
coordinator verifies only keys baked into `ACCEPTED_APP_SIGNING_KEYS_B64`.
Adding or rotating a key is a coordinator hot-swap, with no DNA change.
v3.3.0 ships `ACCEPTED_APP_SIGNING_KEYS_B64` EMPTY. Any supplied attestation
currently rejects with `unrecognized app signing key`; attestation calls start
working after the humm-tauri app key lands in that list.

| Condition | Exact error |
|---|---|
| App key absent from the accepted list | `unrecognized app signing key` |
| Signature wire is malformed | `app attestation signature malformed` |
| Ed25519 verification fails | `app attestation signature invalid` |

**Honesty:** extracting the app key from a shipped binary defeats this tier. It
deters casual spoofing and attributes claims to an app build. It does not
prove hardware identity or runtime integrity. Hardware attestation remains
pass-7 research.

## 7. Publication is opt-in

Nothing publishes without an explicit client call. The zome has no ambient or
background publication path. humm-tauri decides whether and when to call
`upsert_service_meter` or `publish_node_spec`.

## 8. Recommended client cadence

Every meter upsert that changes counters appends an update action to the same day
record's update fan. An update fan is the set of update actions that point back
to one original day record. Counters are cumulative, so most reports do write.

Report hourly or after a material delta, for instance at least 10 MiB served or
at least 100 requests since the last write. Batch dimensions into one upsert.
Do not run per-request or per-minute write loops: a minute loop adds about 1440
actions per day of DHT bloat. One meter record per UTC day keeps the update fan
bounded, and client batching keeps each day's fan small.

## 9. Payer-side boundary

This zome adds zero payer or settlement surface. Unyt runs as a separate app
alongside HummHive, and settlement happens there. When Unyt's
`holo_hosting_proof_of_service` dimensions land, map the meter dimension names
onto their names in the client, not the zome.

## 10. Given/When/Then sanity pairs

| Good statement | Close, but wrong |
|---|---|
| Given a crashed re-upsert with the same absolute counters, when it retries, then counters do NOT double | Given a retry, when the same totals return, then the zome adds them again |
| Given a lower counter value, when upserted, then the stored max stays — meters never decrement | Given a corrected lower total, when upserted, then the stored counter decreases |
| Given an old node spec with three keys and a new spec with one key, when published, then only the new key remains | Given a new node spec, when published, then omitted old keys remain after a map merge |
| Given v3.3.0's empty accepted-keys list and a supplied attestation, when published, then it rejects with `unrecognized app signing key` | Given an unknown app key, when published, then the zome silently records a self-report |
| Given no explicit node-spec call, when the cell stays online, then no node-spec record appears | Given an online cell, when time passes, then the zome publishes capacity in the background |
