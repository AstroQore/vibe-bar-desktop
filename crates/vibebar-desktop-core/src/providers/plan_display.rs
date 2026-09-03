//! Turning a provider's raw plan string into the label the cards show —
//! the native `ProviderPlanDisplay`, so both clients name a plan the same.

/// The generic formatter every provider's plan goes through: a small exact
/// map first, then a trailing "plan"/"account" trimmed off, then each word
/// title-cased unless it is already an acronym.
pub fn generic(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(exact) = exact(raw) {
        return Some(exact);
    }
    let cleaned = clean(raw);
    let words: Vec<&str> = cleaned
        .split(|c: char| c == '_' || c == '-' || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() {
        return Some(if cleaned.is_empty() {
            raw.to_string()
        } else {
            cleaned
        });
    }
    let formatted = words.into_iter().map(word).collect::<Vec<_>>().join(" ");
    Some(if formatted.is_empty() {
        raw.to_string()
    } else {
        formatted
    })
}

/// Grok's tiers, whose product spelling has no space inside "SuperGrok".
pub fn grok(raw: &str) -> Option<String> {
    let display = generic(raw)?;
    Some(
        match display.replace(' ', "").to_ascii_lowercase().as_str() {
            "supergrokheavy" => "SuperGrok Heavy".to_string(),
            "supergrok" => "SuperGrok".to_string(),
            "supergroklite" => "SuperGrok Lite".to_string(),
            _ => display,
        },
    )
}

fn exact(raw: &str) -> Option<String> {
    match raw.to_ascii_lowercase().as_str() {
        "prolite" | "pro_lite" | "pro-lite" | "pro lite" => Some("Pro Lite".into()),
        _ => None,
    }
}

fn clean(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    let trimmed = if lower.ends_with(" plan") {
        &raw[..raw.len() - 5]
    } else if lower.ends_with(" account") {
        &raw[..raw.len() - 8]
    } else {
        raw
    };
    trimmed.trim().to_string()
}

/// Acronyms that stay upper-case rather than becoming "Cbp".
const UPPERCASE_WORDS: [&str; 2] = ["cbp", "k12"];

fn word(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if let Some(exact) = exact(&lower) {
        return exact;
    }
    if UPPERCASE_WORDS.contains(&lower.as_str()) {
        return lower.to_uppercase();
    }
    // Something already shouting (an acronym, a wire constant) is left as it
    // is; only a lower-case first letter is raised.
    if raw == raw.to_uppercase() && raw.chars().any(char::is_alphabetic) {
        return raw.to_string();
    }
    let mut chars = raw.chars();
    match chars.next() {
        Some(first) if first.is_lowercase() => {
            first.to_uppercase().collect::<String>() + chars.as_str()
        }
        _ => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generic_formatter_title_cases_words() {
        assert_eq!(generic("pro").as_deref(), Some("Pro"));
        assert_eq!(generic("  free_trial ").as_deref(), Some("Free Trial"));
        assert_eq!(generic("Business plan").as_deref(), Some("Business"));
        assert_eq!(generic("team account").as_deref(), Some("Team"));
        assert_eq!(generic("pro-lite").as_deref(), Some("Pro Lite"));
        assert_eq!(generic("").as_deref(), None);
        assert_eq!(generic("   ").as_deref(), None);
        // An acronym keeps its shape; a known one is raised.
        assert_eq!(generic("MAX").as_deref(), Some("MAX"));
        assert_eq!(generic("cbp").as_deref(), Some("CBP"));
    }

    #[test]
    fn grok_tiers_keep_their_product_spelling() {
        assert_eq!(grok("SUPER_GROK_HEAVY").as_deref(), Some("SuperGrok Heavy"));
        assert_eq!(grok("super grok").as_deref(), Some("SuperGrok"));
        assert_eq!(grok("supergrok_lite").as_deref(), Some("SuperGrok Lite"));
        assert_eq!(grok("something else").as_deref(), Some("Something Else"));
        assert_eq!(grok(" ").as_deref(), None);
    }
}
