//! Shared quota vocabulary.
//!
//! **The two naming axes.** Vibe Bar names things along two axes that must
//! never be mixed inside one list:
//!
//! - *Quota axis* — L1 company → L2 SubProvider → L3 tool. This is the
//!   billing view, and it is what [`ToolType`] and [`ProviderHierarchy`]
//!   describe.
//! - *Usage axis* — the harness (the CLI or app that produced a session).
//!   That lives in `agent-session-core`.
//!
//! Raw values here are storage keys shared with the native app: they appear
//! in `settings.json`, in `quotas/*.json`, and in menu-bar field ids. They
//! must match the Swift `ToolType.rawValue` byte for byte.

use serde::{Deserialize, Serialize};

/// Three-level vendor / SubProvider / tool identity for one tracked surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProviderHierarchy {
    /// L1 — the enterprise / brand owner (OpenAI, Anthropic, Google AI…).
    pub vendor: &'static str,
    /// L2 — the SubProvider consumed inside that owner.
    pub product: &'static str,
    /// L3 — the concrete local or web surface tracked.
    pub tool: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolType {
    #[serde(rename = "codex")]
    Codex,
    #[serde(rename = "claude")]
    Claude,
    #[serde(rename = "gemini")]
    Gemini,
    #[serde(rename = "antigravity")]
    Antigravity,
    #[serde(rename = "grok")]
    Grok,
    #[serde(rename = "cursor")]
    Cursor,
    #[serde(rename = "copilot")]
    Copilot,
    #[serde(rename = "alibaba")]
    Alibaba,
    #[serde(rename = "alibabaTokenPlan")]
    AlibabaTokenPlan,
    #[serde(rename = "zai")]
    Zai,
    #[serde(rename = "minimax")]
    Minimax,
    #[serde(rename = "kimi")]
    Kimi,
    #[serde(rename = "mimo")]
    Mimo,
    #[serde(rename = "iflytek")]
    IFlytek,
    #[serde(rename = "tencentHunyuan")]
    TencentHunyuan,
    #[serde(rename = "tencentTokenPlan")]
    TencentTokenPlan,
    #[serde(rename = "volcengine")]
    Volcengine,
    #[serde(rename = "volcengineAgentPlan")]
    VolcengineAgentPlan,
    #[serde(rename = "baiduQianfan")]
    BaiduQianfan,
    #[serde(rename = "openCodeGo")]
    OpenCodeGo,
    #[serde(rename = "kilo")]
    Kilo,
    #[serde(rename = "kiro")]
    Kiro,
    #[serde(rename = "ollama")]
    Ollama,
    #[serde(rename = "openRouter")]
    OpenRouter,
    #[serde(rename = "warp")]
    Warp,
}

impl ToolType {
    pub const ALL: [ToolType; 25] = [
        ToolType::Codex,
        ToolType::Claude,
        ToolType::Gemini,
        ToolType::Antigravity,
        ToolType::Grok,
        ToolType::Cursor,
        ToolType::Copilot,
        ToolType::Alibaba,
        ToolType::AlibabaTokenPlan,
        ToolType::Zai,
        ToolType::Minimax,
        ToolType::Kimi,
        ToolType::Mimo,
        ToolType::IFlytek,
        ToolType::TencentHunyuan,
        ToolType::TencentTokenPlan,
        ToolType::Volcengine,
        ToolType::VolcengineAgentPlan,
        ToolType::BaiduQianfan,
        ToolType::OpenCodeGo,
        ToolType::Kilo,
        ToolType::Kiro,
        ToolType::Ollama,
        ToolType::OpenRouter,
        ToolType::Warp,
    ];

    pub fn raw_value(self) -> &'static str {
        match self {
            ToolType::Codex => "codex",
            ToolType::Claude => "claude",
            ToolType::Gemini => "gemini",
            ToolType::Antigravity => "antigravity",
            ToolType::Grok => "grok",
            ToolType::Cursor => "cursor",
            ToolType::Copilot => "copilot",
            ToolType::Alibaba => "alibaba",
            ToolType::AlibabaTokenPlan => "alibabaTokenPlan",
            ToolType::Zai => "zai",
            ToolType::Minimax => "minimax",
            ToolType::Kimi => "kimi",
            ToolType::Mimo => "mimo",
            ToolType::IFlytek => "iflytek",
            ToolType::TencentHunyuan => "tencentHunyuan",
            ToolType::TencentTokenPlan => "tencentTokenPlan",
            ToolType::Volcengine => "volcengine",
            ToolType::VolcengineAgentPlan => "volcengineAgentPlan",
            ToolType::BaiduQianfan => "baiduQianfan",
            ToolType::OpenCodeGo => "openCodeGo",
            ToolType::Kilo => "kilo",
            ToolType::Kiro => "kiro",
            ToolType::Ollama => "ollama",
            ToolType::OpenRouter => "openRouter",
            ToolType::Warp => "warp",
        }
    }

    pub fn from_raw(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.raw_value() == raw)
    }

    /// Canonical L1/L2/L3 identity. Single source of truth for every surface
    /// that groups providers — mirrors `ProviderHierarchyCatalog` in the
    /// native app.
    pub fn hierarchy(self) -> ProviderHierarchy {
        let (vendor, product, tool) = match self {
            ToolType::Codex => ("OpenAI", "ChatGPT Agentic", "Codex"),
            ToolType::Claude => ("Anthropic", "Claude", "Claude Code"),
            ToolType::Gemini => ("Google AI", "Gemini Web", "Gemini Web"),
            ToolType::Antigravity => ("Google AI", "AntiGravity", "AntiGravity"),
            ToolType::Grok => ("SpaceXAI", "Grok", "Grok"),
            ToolType::Cursor => ("SpaceXAI", "Cursor", "Cursor"),
            ToolType::Copilot => ("GitHub", "Copilot", "GitHub Copilot"),
            ToolType::Alibaba => ("Alibaba", "Bailian", "Coding Plan"),
            ToolType::AlibabaTokenPlan => ("Alibaba", "Bailian", "Token Plan"),
            ToolType::Zai => ("Zhipu", "GLM", "GLM Coding Plan"),
            ToolType::Minimax => ("MiniMax", "MiniMax", "MiniMax Token Plan"),
            ToolType::Kimi => ("Moonshot", "Kimi", "Kimi Coding Plan"),
            ToolType::Mimo => ("Xiaomi", "MiMo", "MiMo Token Plan"),
            ToolType::IFlytek => ("iFlytek", "Spark", "Spark Coding Plan"),
            ToolType::TencentHunyuan => ("Tencent", "Hunyuan", "Hunyuan Coding Plan"),
            ToolType::TencentTokenPlan => ("Tencent", "Hunyuan", "Hunyuan Token Plan"),
            ToolType::Volcengine => ("ByteDance", "Doubao", "Doubao Coding Plan"),
            ToolType::VolcengineAgentPlan => ("ByteDance", "Doubao", "Doubao Agent Plan"),
            ToolType::BaiduQianfan => ("Baidu", "Qianfan", "Qianfan Coding Plan"),
            ToolType::OpenCodeGo => ("OpenCode", "OpenCode Go", "OpenCode Go"),
            ToolType::Kilo => ("Kilo", "Kilo", "Kilo"),
            ToolType::Kiro => ("Kiro", "Kiro", "Kiro"),
            ToolType::Ollama => ("Ollama", "Ollama", "Ollama"),
            ToolType::OpenRouter => ("OpenRouter", "OpenRouter", "OpenRouter"),
            ToolType::Warp => ("Warp", "Warp", "Warp"),
        };
        ProviderHierarchy {
            vendor,
            product,
            tool,
        }
    }

    /// Providers this build can fetch live. Everything else is rendered from
    /// the shared cache the native app wrote, clearly attributed as such.
    pub fn has_live_adapter(self) -> bool {
        matches!(
            self,
            ToolType::Codex
                | ToolType::Claude
                | ToolType::Alibaba
                | ToolType::Zai
                | ToolType::Minimax
                | ToolType::Kilo
                | ToolType::OpenRouter
                | ToolType::Warp
        )
    }
}

/// One quota window (a "bucket") of one account.
///
/// `used_percent` is the stored observation and is always clamped to
/// `0..=100`; a non-finite input becomes 0 rather than silently reading as a
/// full bar — the same choice the native app makes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaBucket {
    pub id: String,
    pub title: String,
    pub short_label: String,
    pub used_percent: f64,
    /// Unix epoch seconds. The shared cache stores Apple reference-date
    /// seconds; conversion happens at the cache boundary, never here.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reset_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub raw_window_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub group_title: Option<String>,
}

impl QuotaBucket {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        short_label: impl Into<String>,
        used_percent: f64,
        reset_at: Option<f64>,
        raw_window_seconds: Option<i64>,
        group_title: Option<String>,
    ) -> Self {
        let id = id.into();
        let short_label = expanded_window_label(&short_label.into(), &id);
        Self {
            id,
            title: title.into(),
            short_label,
            used_percent: if used_percent.is_finite() {
                used_percent.clamp(0.0, 100.0)
            } else {
                0.0
            },
            reset_at,
            raw_window_seconds,
            group_title,
        }
    }

    pub fn remaining_percent(&self) -> f64 {
        (100.0 - self.used_percent).clamp(0.0, 100.0)
    }
}

/// Quota-window names are UI copy, not telemetry codes — the abbreviations
/// providers return are expanded once, here, exactly as the native app does.
fn expanded_window_label(label: &str, bucket_id: &str) -> String {
    match bucket_id {
        "five_hour" => return "5 Hours".to_string(),
        "weekly" => return "Weekly".to_string(),
        _ => {}
    }
    label
        .split(' ')
        .map(|part| match part.to_ascii_lowercase().as_str() {
            "5h" => "5 Hours",
            "wk" => "Weekly",
            "mo" | "month" => "Monthly",
            _ => part,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where a rendered quota came from. The UI always says this out loud: a
/// number this client fetched and a number the native app left in the shared
/// cache are different claims about freshness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QuotaOrigin {
    /// Fetched by this client in this session.
    Live,
    /// Read from the shared cache under `~/.vibebar/quotas/`.
    SharedCache,
}

/// Tolerance for clock skew between whatever wrote an observation and this
/// machine reading it.
pub const CLOCK_SKEW_TOLERANCE_SECONDS: f64 = 300.0;

/// One account's quota state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountQuota {
    pub account_id: String,
    pub tool: ToolType,
    pub buckets: Vec<QuotaBucket>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// Unix epoch seconds of the observation.
    pub queried_at: f64,
    pub origin: QuotaOrigin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<crate::error::QuotaError>,
}

impl AccountQuota {
    /// Whether this observation's timestamp can be believed.
    ///
    /// A shared cache accumulates entries from every client that ever ran,
    /// and nothing prunes an account the user signed out of. A real data root
    /// held an entry stamped five months in the *future*, which any
    /// "show the newest reading" rule will pick forever. An observation
    /// cannot come from the future, so one that claims to is not newer —
    /// it is broken, and must never win a recency comparison.
    pub fn has_plausible_timestamp(&self, now_unix: f64) -> bool {
        self.queried_at <= now_unix + CLOCK_SKEW_TOLERANCE_SECONDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_values_round_trip_for_every_tool() {
        for tool in ToolType::ALL {
            assert_eq!(ToolType::from_raw(tool.raw_value()), Some(tool));
            // The storage key must also be what serde writes.
            let json = serde_json::to_string(&tool).unwrap();
            assert_eq!(json, format!("\"{}\"", tool.raw_value()));
        }
    }

    #[test]
    fn hierarchy_is_populated_for_every_tool() {
        for tool in ToolType::ALL {
            let h = tool.hierarchy();
            assert!(!h.vendor.is_empty() && !h.product.is_empty() && !h.tool.is_empty());
        }
        assert_eq!(ToolType::Codex.hierarchy().vendor, "OpenAI");
        assert_eq!(ToolType::Claude.hierarchy().product, "Claude");
        // Google AI and SpaceXAI each own two SubProviders.
        assert_eq!(ToolType::Antigravity.hierarchy().vendor, "Google AI");
        assert_eq!(ToolType::Cursor.hierarchy().vendor, "SpaceXAI");
    }

    #[test]
    fn bucket_clamps_and_expands_labels() {
        let b = QuotaBucket::new("five_hour", "5 Hours", "5h", 142.0, None, None, None);
        assert_eq!(b.used_percent, 100.0);
        assert_eq!(b.short_label, "5 Hours");
        assert_eq!(b.remaining_percent(), 0.0);

        let nan = QuotaBucket::new("weekly", "Weekly", "wk", f64::NAN, None, None, None);
        assert_eq!(nan.used_percent, 0.0);

        let scoped = QuotaBucket::new(
            "weekly_fable",
            "Weekly",
            "Fable wk",
            12.5,
            None,
            Some(604_800),
            Some("Fable".into()),
        );
        assert_eq!(scoped.short_label, "Fable Weekly");
    }
}
