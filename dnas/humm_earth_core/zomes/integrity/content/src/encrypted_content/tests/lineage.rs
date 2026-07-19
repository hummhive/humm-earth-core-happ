use super::super::*;
use hdi::prelude::*;

fn valid_dna_b64() -> String {
    "uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz".to_string()
}

fn valid_action_b64() -> String {
    let dna = DnaHash::try_from(valid_dna_b64().as_str()).unwrap();
    ActionHash::from_raw_36(dna.get_raw_36().to_vec()).to_string()
}

fn lineage(prior_dna_hash_b64: &str, prior_action_hash_b64: &str) -> ContentLineage {
    ContentLineage {
        prior_dna_hash_b64: prior_dna_hash_b64.to_string(),
        prior_action_hash_b64: prior_action_hash_b64.to_string(),
    }
}

fn assert_shape_reject(lineage: &ContentLineage, expected_fragment: &str) {
    match validate_lineage_shape(lineage) {
        ValidateCallbackResult::Invalid(msg) => assert!(
            msg.contains(expected_fragment),
            "expected {expected_fragment:?} in {msg:?}",
        ),
        other => panic!("expected Invalid containing {expected_fragment:?}, got {other:?}"),
    }
}

#[test]
fn shape_accepts_a_well_formed_pair() {
    let claim = lineage(&valid_dna_b64(), &valid_action_b64());
    assert!(matches!(
        validate_lineage_shape(&claim),
        ValidateCallbackResult::Valid,
    ));
}

#[test]
fn shape_rejects_action_typed_or_truncated_dna_hash() {
    let dna = valid_dna_b64();
    let truncated_dna = dna[..dna.len() - 1].to_string();
    for bad_dna in [valid_action_b64(), truncated_dna] {
        assert_shape_reject(
            &lineage(&bad_dna, &valid_action_b64()),
            "lineage prior dna hash is not a valid DNA hash",
        );
    }
}

#[test]
fn shape_rejects_dna_typed_or_truncated_action_hash() {
    let action = valid_action_b64();
    let truncated_action = action[..action.len() - 1].to_string();
    for bad_action in [valid_dna_b64(), truncated_action] {
        assert_shape_reject(
            &lineage(&valid_dna_b64(), &bad_action),
            "lineage prior action hash is not a valid action hash",
        );
    }
}

#[test]
fn link_base_is_deterministic_and_component_ordered() {
    let base_ab = recompute_base(&[&valid_dna_b64(), &valid_action_b64()]).unwrap();
    let base_ab_again = recompute_base(&[&valid_dna_b64(), &valid_action_b64()]).unwrap();
    let base_ba = recompute_base(&[&valid_action_b64(), &valid_dna_b64()]).unwrap();
    assert_eq!(base_ab, base_ab_again);
    assert_ne!(base_ab, base_ba);
}
