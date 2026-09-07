use serde::{Deserialize, Serialize};

/// AppConfig — global application configuration loaded from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub providers: Vec<ProviderEntry>,
    pub embedding: EmbeddingConfig,
    pub privacy: PrivacyConfig,
    pub update: UpdateConfig,
    #[serde(default)]
    pub compaction: CompactionConfig,
    #[serde(default)]
    pub performance_profile: PerformanceProfile,
    #[serde(default)]
    pub cost_guard: CostGuardConfig,
    #[serde(default)]
    pub diff_review: DiffReviewConfig,
    #[serde(default)]
    pub features: FeatureToggles,
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub pricing: Vec<ModelPricing>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub model_pattern: String,
    pub prompt_cost_per_token: f32,
    pub completion_cost_per_token: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub theme: String,     // "dark" or "light"
    pub log_level: String, // "info", "debug", "trace"
    pub max_global_concurrent_agents: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEntry {
    pub name: String,     // "ollama", "openai", "anthropic", "google"
    pub endpoint: String, // "http://127.0.0.1:11434", "https://api.openai.com/v1"
    pub is_local: bool,
    pub is_free: bool,
    pub requires_api_key: bool,
    pub enabled: bool,
    pub models: Vec<String>, // Available model names
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String, // "ollama", "openai", etc.
    pub model: String,    // "llama3.1:8b", "gpt-4o", etc.
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub context_limit: Option<u32>, // Model's context window size
    pub timeout_secs: Option<u64>,
    pub fallback_provider: Option<String>,
    pub fallback_model: Option<String>,
    pub provider_endpoint: Option<String>, // Overrides ProviderEntry.endpoint
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            model: "qwen2.5-coder:3b".to_string(),
            temperature: Some(0.2),
            max_output_tokens: Some(4096),
            context_limit: Some(8192),
            timeout_secs: Some(60),
            fallback_provider: None,
            fallback_model: None,
            provider_endpoint: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub provider: String, // "ollama", "openai"
    pub model: String,    // "nomic-embed-text", "text-embedding-3-small"
    pub dimensions: u32,  // 768, 1536, 3072, etc.
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrivacyConfig {
    pub telemetry_enabled: bool,     // Default: false
    pub crash_reports_enabled: bool, // Default: false
    pub update_checks_enabled: bool, // Default: false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    pub check_on_launch: bool, // Default: false
    pub github_repo: String,   // "abhayzangir1/trans4mer"
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            log_level: "info".to_string(),
            max_global_concurrent_agents: 8,
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            dimensions: 768,
        }
    }
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            check_on_launch: false,
            github_repo: "abhayzangir1/trans4mer".to_string(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            providers: vec![ProviderEntry {
                name: "ollama".to_string(),
                endpoint: "http://127.0.0.1:11434".to_string(),
                is_local: true,
                is_free: true,
                requires_api_key: false,
                enabled: true,
                models: vec![
                    "qwen2.5-coder:3b".to_string(),
                    "llama3".to_string(),
                    "nomic-embed-text".to_string(),
                ],
            }],
            embedding: EmbeddingConfig::default(),
            privacy: PrivacyConfig::default(),
            update: UpdateConfig::default(),
            compaction: CompactionConfig::default(),
            performance_profile: PerformanceProfile::default(),
            cost_guard: CostGuardConfig::default(),
            diff_review: DiffReviewConfig::default(),
            features: FeatureToggles::default(),
            telegram: TelegramConfig::default(),
            pricing: vec![
                ModelPricing {
                    model_pattern: "mini".to_string(),
                    prompt_cost_per_token: 0.00000015,
                    completion_cost_per_token: 0.0000006,
                },
                ModelPricing {
                    model_pattern: "gpt-4o".to_string(),
                    prompt_cost_per_token: 0.0000025,
                    completion_cost_per_token: 0.000010,
                },
                ModelPricing {
                    model_pattern: "sonnet".to_string(),
                    prompt_cost_per_token: 0.000003,
                    completion_cost_per_token: 0.000015,
                },
                ModelPricing {
                    model_pattern: "haiku".to_string(),
                    prompt_cost_per_token: 0.00000025,
                    completion_cost_per_token: 0.00000125,
                },
                ModelPricing {
                    model_pattern: "default".to_string(),
                    prompt_cost_per_token: 0.000001,
                    completion_cost_per_token: 0.000003,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureToggles {
    pub nightly_dreaming_enabled: bool,
    pub dreaming_model: Option<String>,
    pub dynamic_model_scan_enabled: bool,
    pub human_in_the_loop_sync: bool,
}

impl Default for FeatureToggles {
    fn default() -> Self {
        Self {
            nightly_dreaming_enabled: false,
            dreaming_model: None,
            dynamic_model_scan_enabled: true,
            human_in_the_loop_sync: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub bot_token: Option<String>,
    pub allowed_chat_ids: Vec<i64>,
    pub default_project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    pub enabled: bool,
    pub trigger_threshold_pct: f32,
    pub preserve_recent_messages: usize,
    pub use_llm_summary: bool,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            trigger_threshold_pct: 0.8,
            preserve_recent_messages: 5,
            use_llm_summary: true,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum PerformanceMode {
    LowSpecLocal,
    HighSpecCloud,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceProfile {
    pub mode: PerformanceMode,
    pub max_active_llm_slots: usize,
    pub symbolic_short_term_enabled: bool,
    pub idle_only_background_extraction: bool,
}

impl Default for PerformanceProfile {
    fn default() -> Self {
        Self {
            mode: PerformanceMode::LowSpecLocal,
            max_active_llm_slots: 1,
            symbolic_short_term_enabled: true,
            idle_only_background_extraction: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostGuardConfig {
    pub enabled: bool,
    pub global_monthly_ceiling_usd: Option<f32>,
    pub global_daily_ceiling_usd: Option<f32>,
    pub hard_block: bool,
    pub alert_thresholds: Vec<f32>,
}

impl Default for CostGuardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            global_monthly_ceiling_usd: Some(50.0),
            global_daily_ceiling_usd: Some(10.0),
            hard_block: true,
            alert_thresholds: vec![0.5, 0.8, 1.0],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffReviewConfig {
    pub auto_approve_small_files: bool,
    pub auto_approve_clean_commands: bool,
    pub force_review_secret_patterns: Vec<String>,
}

impl Default for DiffReviewConfig {
    fn default() -> Self {
        Self {
            auto_approve_small_files: true,
            auto_approve_clean_commands: true,
            force_review_secret_patterns: vec![
                "password=".to_string(),
                "api_key=".to_string(),
                "sk_live_".to_string(),
                "BEGIN PRIVATE KEY".to_string(),
                "BEGIN RSA PRIVATE KEY".to_string(),
                "aws_secret=".to_string(),
                "stripe_".to_string(),
                "ghp_".to_string(),
            ],
        }
    }
}

impl AppConfig {
    pub fn load_from_toml(path: &std::path::Path) -> Result<Self, crate::error::Trans4mersError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path).map_err(|e| {
            crate::error::Trans4mersError::Config(format!("Failed to read config.toml: {}", e))
        })?;

        toml::from_str(&content).map_err(|e| {
            crate::error::Trans4mersError::Config(format!("Failed to parse config.toml: {}", e))
        })
    }
}
