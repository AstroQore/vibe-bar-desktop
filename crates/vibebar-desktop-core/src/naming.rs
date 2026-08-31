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

    /// The generated `naming.ts` is exactly what the generator produces from
    /// the current contract.
    ///
    /// This checked selected substrings first, which was not a freshness
    /// check at all: a contract change to the group-key rules, the stem
    /// suffixes, the SubProvider overrides or the ungrouped list leaves the
    /// behaviour different and every substring still present. Re-running the
    /// generator and diffing covers whatever the contract grows next, without
    /// this test having to learn about it.
    #[test]
    fn the_generated_typescript_is_current() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let generated = root.join("apps/desktop/src/naming.ts");
        let committed = std::fs::read_to_string(&generated).expect("naming.ts");

        let Ok(output) = std::process::Command::new("node")
            .arg(root.join("scripts/generate-naming.mjs"))
            .output()
        else {
            // No node on this machine: the substring fallback below is weaker
            // but better than no check at all.
            assert_fallback(&committed, &contract());
            return;
        };
        // Restore whatever was committed before asserting, so a failing test
        // never leaves the working tree modified.
        let regenerated = std::fs::read_to_string(&generated).expect("naming.ts");
        std::fs::write(&generated, &committed).expect("restore naming.ts");
        assert!(
            output.status.success(),
            "the generator refused to run: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            regenerated, committed,
            "naming.ts is stale — the contract changed without it being \
             regenerated, which leaves this client grouping providers the way \
             the native app used to. Run `pnpm run naming`."
        );
    }

    /// Every tool and group label present, for the case where the generator
    /// cannot be run.
    fn assert_fallback(generated: &str, doc: &Value) {
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
    }

    /// The adapters shorten their own bucket names, which the UI prefers over
    /// the contract's label. Two shortenings of one thing is one too many, so
    /// where both have an opinion they have to agree — otherwise a bucket
    /// reads as "Spark" in the native app and something else here.
    #[test]
    fn adapter_short_names_agree_with_the_contract() {
        let doc = contract();
        let labels = doc["groupLabels"].as_object().expect("groupLabels");
        let source = include_str!("providers/codex.rs");
        let block = source
            .split("fn short_limit_name")
            .nth(1)
            .expect("short_limit_name");
        let block = &block[..block.find("\n}").expect("end of fn")];

        // `spark` -> "Spark", which has to be what codex.spark is called.
        for (needle, shortened) in
            regex_like_pairs(block)
        {
            let key = format!("codex.{needle}");
            if let Some(expected) = labels.get(&key).and_then(Value::as_str) {
                assert_eq!(
                    shortened, expected,
                    "codex.rs shortens {needle} to {shortened:?} where the \
                     contract calls it {expected:?}"
                );
            }
        }
    }

    /// `contains("spark")` / `"Spark".to_string()` pairs, without a regex
    /// dependency for one call site.
    fn regex_like_pairs(block: &str) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        let mut needle: Option<String> = None;
        for line in block.lines() {
            if let Some(rest) = line.split("contains(\"").nth(1) {
                needle = rest.split('"').next().map(str::to_string);
            } else if let Some(rest) = line.split('"').nth(1) {
                if let Some(found) = needle.take() {
                    pairs.push((found, rest.to_string()));
                }
            }
        }
        pairs
    }
}
