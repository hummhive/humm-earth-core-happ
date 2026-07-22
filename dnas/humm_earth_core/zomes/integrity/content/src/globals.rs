use hdi::prelude::*;

pub const ENCRYPTED_CONTENT_TIME_INDEX: &'static str = "encrypted_content_time";

/// Verdict on whether a delegated `expiry` stays within the grantor's
/// `grantor_expiry`; callers strip permanent-authority cases before delegating.
pub(crate) fn validate_expiry_containment(
    new_expiry: Option<Timestamp>,
    grantor_expiry: Timestamp,
) -> ValidateCallbackResult {
    match new_expiry {
        Some(new_expiry) if new_expiry <= grantor_expiry => ValidateCallbackResult::Valid,
        Some(new_expiry) => ValidateCallbackResult::Invalid(format!(
            "granted expiry {new_expiry:?} exceeds the grantor membership's expiry \
             {grantor_expiry:?}; an expiring grantor may not extend the delegation window",
        )),
        None => ValidateCallbackResult::Invalid(
            "an expiring grantor may not mint a permanent (no-expiry) membership".into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contained_expiry_is_valid() {
        let verdict = validate_expiry_containment(Some(Timestamp(100)), Timestamp(200));
        assert!(matches!(verdict, ValidateCallbackResult::Valid));
    }

    #[test]
    fn expiry_equal_to_grantor_is_valid() {
        let verdict = validate_expiry_containment(Some(Timestamp(200)), Timestamp(200));
        assert!(matches!(verdict, ValidateCallbackResult::Valid));
    }

    #[test]
    fn expiry_beyond_grantor_is_invalid() {
        let verdict = validate_expiry_containment(Some(Timestamp(300)), Timestamp(200));
        let ValidateCallbackResult::Invalid(msg) = verdict else {
            panic!("extension past grantor expiry must reject");
        };
        assert!(msg.contains("may not extend the delegation window"));
    }

    #[test]
    fn permanent_grant_from_expiring_grantor_is_invalid() {
        let verdict = validate_expiry_containment(None, Timestamp(200));
        let ValidateCallbackResult::Invalid(msg) = verdict else {
            panic!("permanent grant from an expiring grantor must reject");
        };
        assert!(msg.contains("may not mint a permanent (no-expiry) membership"));
    }
}
