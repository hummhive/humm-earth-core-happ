use super::*;

fn map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(key, value)| ((*key).into(), (*value).into()))
        .collect()
}

fn indexed_map(count: usize) -> BTreeMap<String, String> {
    (0..count)
        .map(|index| (format!("k{index:02}"), "1".into()))
        .collect()
}

fn assert_guest_error<T>(result: ExternResult<T>, expected: &str) {
    let error = match result {
        Ok(_) => panic!("expected guest error: {expected}"),
        Err(error) => error,
    };
    match error.error {
        WasmErrorInner::Guest(message) => assert_eq!(message, expected),
        other => panic!("expected Guest error, got {other:?}"),
    }
}

#[test]
fn period_validation_accepts_day_bucket_and_rejects_invalid_shapes() {
    assert!(validate_period("2026-07-17").is_ok());
    for invalid in [
        "2026-13-01",
        "2026-00-10",
        "2026-1-17",
        "26-07-17",
        "2026-07-32",
        "2026/07/17",
        "2026-07-1",
        "2026-07-170",
    ] {
        assert_guest_error(validate_period(invalid), INVALID_METER_PERIOD);
    }
}

#[test]
fn key_validation_enforces_both_wire_contracts() {
    let valid = "k".repeat(64);
    assert!(canonicalize_counters(map(&[(&valid, "1"), ("a b", "2")])).is_ok());
    assert!(validate_spec_entries(&map(&[(&valid, "v"), ("a b", "v")])).is_ok());
    let too_long = "k".repeat(65);
    for invalid in ["", "bad|key", "bad;key", "bad=key", "é", &too_long] {
        assert_guest_error(
            canonicalize_counters(map(&[(invalid, "1")])),
            INVALID_METER_KEY,
        );
        assert_guest_error(
            validate_spec_entries(&map(&[(invalid, "v")])),
            INVALID_NODE_SPEC_KEY,
        );
    }
}

#[test]
fn counter_canonicalization_normalizes_u128_and_rejects_non_decimals() {
    for (input, expected) in [
        ("007", "7"),
        ("0", "0"),
        (
            "340282366920938463463374607431768211455",
            "340282366920938463463374607431768211455",
        ),
    ] {
        let actual =
            canonicalize_counters(map(&[("counter", input)])).expect("valid counter value");
        assert_eq!(actual, map(&[("counter", expected)]));
    }
    for invalid in [
        "-1",
        "+1",
        "",
        "1.5",
        "1e3",
        "9999999999999999999999999999999999999999",
    ] {
        assert_guest_error(
            canonicalize_counters(map(&[("counter", invalid)])),
            INVALID_METER_COUNTER,
        );
    }
}

#[test]
fn collection_bounds_accept_the_limit_and_reject_one_over() {
    assert!(canonicalize_counters(indexed_map(16)).is_ok());
    assert_guest_error(
        canonicalize_counters(indexed_map(17)),
        TOO_MANY_METER_DIMENSIONS,
    );
    assert!(validate_spec_entries(&indexed_map(32)).is_ok());
    assert_guest_error(
        validate_spec_entries(&indexed_map(33)),
        INVALID_NODE_SPEC_ENTRIES,
    );
}

#[test]
fn counter_merge_keeps_maximums_and_enforces_the_union_bound() {
    for (prior, new, expected) in [
        (
            map(&[("requests", "4")]),
            map(&[("requests", "9")]),
            map(&[("requests", "9")]),
        ),
        (
            map(&[("requests", "9")]),
            map(&[("requests", "4")]),
            map(&[("requests", "9")]),
        ),
        (
            map(&[("cpu", "2")]),
            map(&[("storage", "3")]),
            map(&[("cpu", "2"), ("storage", "3")]),
        ),
    ] {
        assert_eq!(merge_counters(&prior, &new).expect("valid merge"), expected);
    }
    let equal = map(&[("requests", "9")]);
    assert_eq!(merge_counters(&equal, &equal).expect("equal maps"), equal);
    assert_guest_error(
        merge_counters(&indexed_map(16), &map(&[("k16", "1")])),
        METER_UNION_EXCEEDS_DIMENSIONS,
    );
}

#[test]
fn attestation_canonical_string_matches_the_wire_golden_value() {
    let author = AgentPubKey::from_raw_36(vec![7u8; 36]).to_string();
    let spec = map(&[("ram_gb", "32"), ("cpu_cores", "8")]);
    assert_eq!(
        attestation_canonical_string(&author, 1_752_700_000_000_000, &spec),
        "hummhive-node-spec/1|uhCAkBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcH|1752700000000000|cpu_cores=8;ram_gb=32"
    );
}

#[test]
fn snapshots_round_trip_through_serialized_bytes() {
    let meter = ServiceMeterSnapshot {
        schema: SERVICE_METER_SCHEMA_TAG.into(),
        period: "2026-07-17".into(),
        counters: map(&[("requests", "7")]),
    };
    let bytes = SerializedBytes::try_from(meter.clone()).expect("serialize meter");
    assert_eq!(
        ServiceMeterSnapshot::try_from(bytes).expect("decode meter"),
        meter
    );
    let node = NodeSpecSnapshot {
        schema: NODE_SPEC_SCHEMA_TAG.into(),
        spec: map(&[("cpu_cores", "8")]),
        declared_at_micros: 1_752_700_000_000_000,
        verified_by_app_key: None,
    };
    let bytes = SerializedBytes::try_from(node.clone()).expect("serialize node spec");
    assert_eq!(
        NodeSpecSnapshot::try_from(bytes).expect("decode node spec"),
        node
    );
}

#[test]
fn node_spec_value_bounds_and_delimiters_are_enforced() {
    let maximum = "v".repeat(256);
    assert!(validate_spec_entries(&map(&[("note", &maximum)])).is_ok());
    let too_long = "v".repeat(257);
    for invalid in [
        "",
        "bad|value",
        "bad;value",
        "line\nbreak",
        "esc\u{1b}[31m",
        &too_long,
    ] {
        assert_guest_error(
            validate_spec_entries(&map(&[("note", invalid)])),
            INVALID_NODE_SPEC_VALUE,
        );
    }
}
