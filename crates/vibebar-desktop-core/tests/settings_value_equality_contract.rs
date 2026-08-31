//! When two JSON values mean the same setting.
//!
//! Both clients decide this, and a disagreement is not academic: whichever
//! side thinks a value changed writes a file the other did not expect, and
//! tells its user their setting was replaced when nothing happened to it.

use serde_json::Value;
use vibebar_desktop_core::shared::settings_document::values_equal;

const CONTRACT: &str = include_str!("../../../docs/contracts/settings-value-equality-v1.json");

fn contract() -> Value {
    serde_json::from_str(CONTRACT).expect("the contract file parses")
}

#[test]
fn every_case_agrees_with_the_contract() {
    let document = contract();
    let cases = document["cases"].as_array().expect("cases");
    assert!(cases.len() > 10, "the contract file looks truncated");
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let expected = case["equal"].as_bool().expect("equal");
        assert_eq!(
            values_equal(&case["left"], &case["right"]),
            expected,
            "{name}: left={} right={}",
            case["left"],
            case["right"]
        );
    }
}

/// A divergence is only worth recording while it is still true. If this
/// client's answer changes, the record is stale and the note explaining why it
/// was left alone no longer applies.
#[test]
fn the_recorded_divergences_are_still_what_this_client_does() {
    let document = contract();
    let divergences = document["knownDivergences"].as_array().expect("knownDivergences");
    assert!(!divergences.is_empty());
    for case in divergences {
        let name = case["case"].as_str().expect("case");
        assert_eq!(
            values_equal(&case["left"], &case["right"]),
            case["desktop"].as_bool().expect("desktop"),
            "{name}: this client no longer behaves as the contract records"
        );
        assert_ne!(
            case["native"], case["desktop"],
            "{name}: recorded as a divergence but both clients agree"
        );
    }
}
