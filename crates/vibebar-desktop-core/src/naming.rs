//! The quota naming axis, shared with the native client.
//!
//! `docs/contracts/quota-naming-v1.json` is generated from the native Swift
//! sources and checked against them there. This lane checks the other half:
//! that this crate's own hierarchy table says the same thing, and that the
//! generated TypeScript still matches the contract.
//!
//! Both matter for the same reason. The names decide how every provider list
//! in both clients is arranged, and `AGENTS.md` § 7.1 makes agreeing on them a
//! behavioural rule — a bucket filed under "Spark" in one client and
//! "GPT-5.3 Codex Spark" in the other is two things to the reader.

/// The contract, embedded so a build cannot disagree with the file it shipped.
pub const QUOTA_NAMING: &str = include_str!("../../../docs/contracts/quota-naming-v1.json");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ToolType;
    use serde_json::Value;

    fn contract() -> Value {
        serde_json::from_str(QUOTA_NAMING).expect("quota naming parses")
    }

    /// This crate keeps its own `ToolType::hierarchy` because it returns
    /// `&'static str` and is on hot paths. That makes it a second copy, so it
    /// is checked rather than trusted.
    #[test]
    fn the_rust_hierarchy_agrees_with_the_contract() {
        let doc = contract();
        let hierarchy = doc["hierarchy"].as_object().expect("hierarchy is an object");

        for tool in ToolType::ALL {
            let entry = hierarchy
                .get(tool.raw_value())
                .unwrap_or_else(|| panic!("no hierarchy for {}", tool.raw_value()));
            let ours = tool.hierarchy();
            assert_eq!(
                entry["company"].as_str(),
                Some(ours.vendor),
                "{} company",
                tool.raw_value()
            );
            assert_eq!(
                entry["subProvider"].as_str(),
                Some(ours.product),
                "{} subProvider",
                tool.raw_value()
            );
        }
        assert_eq!(
            hierarchy.len(),
            ToolType::ALL.len(),
            "the contract names a tool this client does not have, or the reverse"
        );
    }

    /// The generated `naming.ts` still matches the contract it came from. The
    /// generator runs by hand, so this is what stops a contract update from
    /// leaving the UI grouping providers the way the native app used to.
    #[test]
    fn the_generated_typescript_is_current() {
        let generated = include_str!("../../../apps/desktop/src/naming.ts");
        let doc = contract();

        for (tool, entry) in doc["hierarchy"].as_object().expect("hierarchy") {
            let company = entry["company"].as_str().expect("company");
            let sub = entry["subProvider"].as_str().expect("subProvider");
            assert!(
                generated.contains(&format!(
                    "{tool}: {{ company: \"{company}\", subProvider: \"{sub}\" }}"
                )),
                "naming.ts is stale for {tool}. Run `pnpm run naming`."
            );
        }
        for (key, label) in doc["groupLabels"].as_object().expect("groupLabels") {
            let label = label.as_str().expect("label");
            assert!(
                generated.contains(&format!("\"{key}\": \"{label}\"")),
                "naming.ts is stale for group {key}. Run `pnpm run naming`."
            );
        }
        // The rule that decides whether a bucket is its own group. Without it
        // the client has to guess, and the guess was wrong for every bucket
        // discovered at runtime.
        let branch = &doc["groupKey"]["branchStyle"];
        for bucket in branch["buckets"].as_array().expect("branch buckets") {
            let bucket = bucket.as_str().expect("bucket id");
            assert!(
                generated.contains(&format!("\"{bucket}\"")),
                "naming.ts is stale: {bucket} is missing from the branch-style rule."
            );
        }
        for tool in branch["alwaysTools"].as_array().expect("always tools") {
            assert!(
                generated.contains(tool.as_str().expect("tool")),
                "naming.ts is stale: a branch-style tool is missing."
            );
        }
    }
}
