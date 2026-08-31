//! The visual tokens both clients render from.
//!
//! `docs/contracts/design-tokens-v1.json` is generated from the native
//! `Theme.swift` and checked against it in that repository. This lane checks
//! the other half: that the contract covers every provider this client can
//! show, and that the generated TypeScript still matches the contract.
//!
//! Without the second check the front end would drift silently — the
//! generator is only run by hand, and a contract update with no regeneration
//! leaves the UI drawing yesterday's colours while every test passes.

/// The contract, embedded so a build cannot disagree with the file it shipped.
pub const DESIGN_TOKENS: &str = include_str!("../../../docs/contracts/design-tokens-v1.json");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ToolType;
    use serde_json::Value;

    fn contract() -> Value {
        serde_json::from_str(DESIGN_TOKENS).expect("design tokens parse")
    }

    /// Every provider this client can render has an accent. A missing one is
    /// a provider drawn in the default colour while the native app gives it a
    /// brand — the same account looking like two different things.
    #[test]
    fn every_tool_has_an_accent() {
        let doc = contract();
        let accents = doc["providerAccent"]
            .as_object()
            .expect("providerAccent is an object");
        let missing: Vec<&str> = ToolType::ALL
            .iter()
            .map(|tool| tool.raw_value())
            .filter(|raw| !accents.contains_key(*raw))
            .collect();
        assert!(missing.is_empty(), "no accent for {missing:?}");
    }

    /// The generated `tokens.ts` still matches the contract it came from.
    /// The generator runs by hand, so this is what stops a contract update
    /// from leaving the UI on the previous colours.
    #[test]
    fn the_generated_typescript_is_current() {
        let generated = include_str!("../../../apps/desktop/src/tokens.ts");
        let doc = contract();
        let accents = doc["providerAccent"].as_object().expect("accents");

        for (tool, value) in accents {
            match value {
                Value::String(hex) => assert!(
                    generated.contains(&format!("{tool}: \"{hex}\"")),
                    "tokens.ts is stale for {tool}: expected {hex}. Run `pnpm run tokens`."
                ),
                Value::Object(pair) => {
                    let light = pair["light"].as_str().expect("light");
                    let dark = pair["dark"].as_str().expect("dark");
                    assert!(
                        generated.contains(&format!(
                            "{tool}: {{ light: \"{light}\", dark: \"{dark}\" }}"
                        )),
                        "tokens.ts is stale for {tool}. Run `pnpm run tokens`."
                    );
                }
                other => panic!("{tool} has an unexpected accent shape: {other}"),
            }
        }

        // The chart palette is separate from providerAccent — Claude is coral
        // in one and orange in the other — so it needs its own check.
        let reset_history = doc["resetHistoryAccent"]
            .as_object()
            .expect("resetHistoryAccent is an object");
        for (tool, value) in reset_history {
            let hex = value.as_str().expect("reset-history accent");
            assert!(
                generated.contains(&format!("{tool}: \"{hex}\"")),
                "tokens.ts is stale for resetHistoryAccent.{tool}: expected {hex}. \
                 Run `pnpm run tokens`."
            );
        }
        assert!(
            reset_history.contains_key("default"),
            "an unlisted provider must have a defined colour in both clients"
        );

        let bar = &doc["quotaBar"];
        for mode in ["remaining", "used"] {
            for level in ["critical", "warning", "ok"] {
                let hex = bar[mode][level].as_str().expect("bar colour");
                assert!(
                    generated.contains(&format!("{level}: \"{hex}\"")),
                    "tokens.ts is stale for quotaBar.{mode}.{level}. Run `pnpm run tokens`."
                );
            }
        }
        // Thresholds too: the same hue at a different cut-off is still two
        // clients disagreeing about the same number.
        for (mode, key) in [
            ("remaining", "criticalBelow"),
            ("remaining", "warningBelow"),
            ("used", "criticalAtOrAbove"),
            ("used", "warningAtOrAbove"),
        ] {
            let value = bar[mode][key].as_f64().expect("threshold") as i64;
            assert!(
                generated.contains(&format!("{key}: {value}")),
                "tokens.ts is stale for quotaBar.{mode}.{key}. Run `pnpm run tokens`."
            );
        }
        let opacity = bar["trackOpacity"].as_f64().expect("trackOpacity");
        assert!(
            generated.contains(&format!("trackOpacity: {opacity}")),
            "tokens.ts is stale for quotaBar.trackOpacity. Run `pnpm run tokens`."
        );
    }

    /// Every bundled mark must be able to take the accent. The marks disagree
    /// on how they spell their fill — nine say `white`, two say `#FFFFFF` in
    /// different cases, three carry dark brand hexes — and matching only the
    /// literal `white` left Grok and Kiro invisible on a light background.
    #[test]
    fn every_brand_mark_can_take_the_accent() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/assets/providers");
        let mut seen = 0usize;
        for entry in std::fs::read_dir(&dir).expect("provider icons") {
            let path = entry.expect("entry").path();
            if path.extension().is_none_or(|extension| extension != "svg") {
                continue;
            }
            seen += 1;
            let markup = std::fs::read_to_string(&path).expect("read mark");
            // Mirrors ProviderIcon.tsx, which rewrites every fill that is not
            // `none` to currentColor.
            let empty_fill = markup
                .split("fill=\"")
                .skip(1)
                .filter_map(|rest| rest.split('"').next())
                .any(str::is_empty);
            assert!(!empty_fill, "{} has an empty fill", path.display());
        }
        assert!(seen >= 23, "expected the full mark set, saw {seen}");
    }
}
