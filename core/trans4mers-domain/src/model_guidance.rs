use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelFamily {
    AgenticCoding,
    GeneralReasoning,
    FastChat,
    Embedding,
    VisionMultimodal,
}

impl std::fmt::Display for ModelFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AgenticCoding => write!(f, "Agentic Coding"),
            Self::GeneralReasoning => write!(f, "General Reasoning"),
            Self::FastChat => write!(f, "Fast Chat"),
            Self::Embedding => write!(f, "Embedding"),
            Self::VisionMultimodal => write!(f, "Vision & Multimodal"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelSizeClass {
    Small,  // < 7B: runs comfortably on 8GB RAM CPU/iGPU
    Medium, // 7B - 14B: requires 16GB RAM / 8GB VRAM
    Large,  // 14B - 70B+: requires 32GB+ RAM / 24GB VRAM
    Cloud,  // Frontier API models hosted externally
}

impl std::fmt::Display for ModelSizeClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Small => write!(f, "Small (<7B)"),
            Self::Medium => write!(f, "Medium (7B-14B)"),
            Self::Large => write!(f, "Large (14B-70B+)"),
            Self::Cloud => write!(f, "Cloud Frontier"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelGuidance {
    pub pattern: String,
    pub display_name: String,
    pub family: ModelFamily,
    pub size_class: ModelSizeClass,
    pub recommended_for: Vec<String>,
    pub description: String,
    pub context_window: u32,
    pub supports_tools: bool,
}

pub struct StaticModelEntry {
    pub pattern: &'static str,
    pub display_name: &'static str,
    pub family: ModelFamily,
    pub size_class: ModelSizeClass,
    pub recommended_for: &'static [&'static str],
    pub description: &'static str,
    pub context_window: u32,
    pub supports_tools: bool,
}

impl StaticModelEntry {
    pub fn to_guidance(&self) -> ModelGuidance {
        ModelGuidance {
            pattern: self.pattern.to_string(),
            display_name: self.display_name.to_string(),
            family: self.family,
            size_class: self.size_class,
            recommended_for: self.recommended_for.iter().map(|s| s.to_string()).collect(),
            description: self.description.to_string(),
            context_window: self.context_window,
            supports_tools: self.supports_tools,
        }
    }
}

pub static MODEL_CATALOG: &[StaticModelEntry] = &[
    StaticModelEntry {
        pattern: "qwen2.5-coder",
        display_name: "Qwen 2.5 Coder",
        family: ModelFamily::AgenticCoding,
        size_class: ModelSizeClass::Medium,
        recommended_for: &[
            "Autonomous Coding",
            "ReAct Tool Use",
            "Refactoring",
            "Bug Fixing",
        ],
        description: "Specialized open weights coding model with strong repository reasoning and robust JSON tool-calling.",
        context_window: 32768,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "qwen2.5",
        display_name: "Qwen 2.5",
        family: ModelFamily::GeneralReasoning,
        size_class: ModelSizeClass::Medium,
        recommended_for: &["General Reasoning", "Multilingual", "Task Planning"],
        description: "Alibaba's general foundation model with balanced logic, mathematics, and multilingual instruction following.",
        context_window: 32768,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "deepseek-coder",
        display_name: "DeepSeek Coder",
        family: ModelFamily::AgenticCoding,
        size_class: ModelSizeClass::Medium,
        recommended_for: &["Code Generation", "Syntax Verification", "Unit Testing"],
        description: "Trained on 2T code tokens with fill-in-the-middle capability and strong Python/Rust syntax understanding.",
        context_window: 16384,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "deepseek-r1",
        display_name: "DeepSeek R1",
        family: ModelFamily::GeneralReasoning,
        size_class: ModelSizeClass::Large,
        recommended_for: &["Deep Reasoning", "Chain of Thought", "Complex Architecture"],
        description: "Reinforcement learning reasoning model demonstrating frontier logic and multi-step derivation.",
        context_window: 65536,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "llama3.1",
        display_name: "Llama 3.1",
        family: ModelFamily::GeneralReasoning,
        size_class: ModelSizeClass::Medium,
        recommended_for: &["General Tasks", "Agentic Tool Use", "Summarization"],
        description: "Meta's flagship open-weights foundation model with official zero-shot tool-calling support.",
        context_window: 131072,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "llama3.2",
        display_name: "Llama 3.2",
        family: ModelFamily::FastChat,
        size_class: ModelSizeClass::Small,
        recommended_for: &["Fast Chat", "On-Device Triage", "Lightweight Subagents"],
        description: "Compact 1B/3B parameters designed for edge and low-latency execution with minimal RAM footprint.",
        context_window: 131072,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "llama3.3",
        display_name: "Llama 3.3",
        family: ModelFamily::GeneralReasoning,
        size_class: ModelSizeClass::Large,
        recommended_for: &["Enterprise Planning", "Complex Synthesis", "Full Codebases"],
        description: "High-capacity 70B open weights model matching prior closed frontier models on standard benchmarks.",
        context_window: 131072,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "codestral",
        display_name: "Codestral",
        family: ModelFamily::AgenticCoding,
        size_class: ModelSizeClass::Large,
        recommended_for: &["Agentic Coding", "Fill-in-the-Middle", "Multi-Language"],
        description: "Mistral AI's dedicated 22B coding model fluent in 80+ programming languages.",
        context_window: 32768,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "mistral",
        display_name: "Mistral",
        family: ModelFamily::GeneralReasoning,
        size_class: ModelSizeClass::Small,
        recommended_for: &["Quick Tasks", "Transformation", "Fast Inference"],
        description: "Efficient 7B general reasoning model with fast sliding-window attention.",
        context_window: 32768,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "phi3",
        display_name: "Phi-3",
        family: ModelFamily::GeneralReasoning,
        size_class: ModelSizeClass::Small,
        recommended_for: &["CPU Inference", "Single-Task Execution", "Compact Devices"],
        description: "Microsoft's high-efficiency small language model trained on highly curated textbook datasets.",
        context_window: 128000,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "phi4",
        display_name: "Phi-4",
        family: ModelFamily::GeneralReasoning,
        size_class: ModelSizeClass::Medium,
        recommended_for: &["Advanced Math", "Reasoning", "STEM Problem Solving"],
        description: "Microsoft's 14B reasoning powerhouse focused on high-accuracy logical deduction.",
        context_window: 16384,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "claude-3-5-sonnet",
        display_name: "Claude 3.5 Sonnet",
        family: ModelFamily::AgenticCoding,
        size_class: ModelSizeClass::Cloud,
        recommended_for: &[
            "Autonomous Agents",
            "Code Architecture",
            "Zero-Shot Tool Calling",
        ],
        description: "Anthropic's leading frontier model for coding and agentic computer use benchmarks.",
        context_window: 200000,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "claude-3-7-sonnet",
        display_name: "Claude 3.7 Sonnet",
        family: ModelFamily::AgenticCoding,
        size_class: ModelSizeClass::Cloud,
        recommended_for: &["Hybrid Reasoning", "Self-Reflection", "Complex Refactors"],
        description: "Anthropic's hybrid reasoning model supporting instant responses and extended deliberate thinking.",
        context_window: 200000,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "claude-3-5-haiku",
        display_name: "Claude 3.5 Haiku",
        family: ModelFamily::FastChat,
        size_class: ModelSizeClass::Cloud,
        recommended_for: &["Rapid Triage", "Low-Latency Generation", "Parsing"],
        description: "Fast, cost-effective cloud model offering near-Sonnet coding speed for interactive tasks.",
        context_window: 200000,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "gpt-4o-mini",
        display_name: "GPT-4o Mini",
        family: ModelFamily::FastChat,
        size_class: ModelSizeClass::Cloud,
        recommended_for: &[
            "High Throughput",
            "Lightweight Tool Calling",
            "Cost Optimization",
        ],
        description: "OpenAI's low-cost, high-speed model suitable for background agent subtasks.",
        context_window: 128000,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "gpt-4o",
        display_name: "GPT-4o",
        family: ModelFamily::GeneralReasoning,
        size_class: ModelSizeClass::Cloud,
        recommended_for: &["Multimodal", "General Intelligence", "Structured Outputs"],
        description: "OpenAI's flagship omni model with fast inference and strong native function calling.",
        context_window: 128000,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "gemini-1.5-pro",
        display_name: "Gemini 1.5 Pro",
        family: ModelFamily::GeneralReasoning,
        size_class: ModelSizeClass::Cloud,
        recommended_for: &[
            "Ultra-Long Context",
            "Multi-Document Analysis",
            "Audio/Video",
        ],
        description: "Google's 2-million token context model capable of ingesting entire codebases and libraries.",
        context_window: 2000000,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "gemini-2.0-flash",
        display_name: "Gemini 2.0 Flash",
        family: ModelFamily::FastChat,
        size_class: ModelSizeClass::Cloud,
        recommended_for: &["Real-Time Agents", "Sub-Second Streaming", "Tool Calls"],
        description: "Google's next-gen real-time multimodal generation engine with low latency.",
        context_window: 1000000,
        supports_tools: true,
    },
    StaticModelEntry {
        pattern: "nomic-embed-text",
        display_name: "Nomic Embed Text",
        family: ModelFamily::Embedding,
        size_class: ModelSizeClass::Small,
        recommended_for: &["Semantic Document RAG", "Memory Indexing", "KNN Search"],
        description: "Open-source 768-dimensional text embedding model with an 8,192 token context window.",
        context_window: 8192,
        supports_tools: false,
    },
    StaticModelEntry {
        pattern: "mxbai-embed-large",
        display_name: "mxbai Embed Large",
        family: ModelFamily::Embedding,
        size_class: ModelSizeClass::Small,
        recommended_for: &["Dense Retrieval", "High-Precision Search"],
        description: "MixedBread AI's 1024-dimensional embedding model optimized for enterprise search.",
        context_window: 512,
        supports_tools: false,
    },
    StaticModelEntry {
        pattern: "bge-m3",
        display_name: "BGE-M3",
        family: ModelFamily::Embedding,
        size_class: ModelSizeClass::Small,
        recommended_for: &["Multilingual RAG", "Hybrid Dense/Sparse Search"],
        description: "Multi-functionality embedding model supporting 100+ languages and dense/sparse representations.",
        context_window: 8192,
        supports_tools: false,
    },
    StaticModelEntry {
        pattern: "all-minilm",
        display_name: "All-MiniLM-L6-v2",
        family: ModelFamily::Embedding,
        size_class: ModelSizeClass::Small,
        recommended_for: &["Ultra-Fast Semantic Search", "CPU-Only Retrieval"],
        description: "Standard 384-dimensional sentence transformer model offering high speed with low compute requirements.",
        context_window: 512,
        supports_tools: false,
    },
];

/// Returns offline guidance for a model name.
/// Performs exact or substring match against known catalog entries,
/// or generates a sound heuristic evaluation if the model is unlisted.
/// Operates strictly offline with ZERO network calls.
pub fn get_model_guidance(model_name: &str) -> ModelGuidance {
    let lower = model_name.to_lowercase();

    // 1. Direct or substring match in catalog
    for entry in MODEL_CATALOG {
        if lower.contains(entry.pattern) {
            return entry.to_guidance();
        }
    }

    // 2. Offline heuristic fallback for uncataloged models
    let is_embedding = lower.contains("embed") || lower.contains("bge") || lower.contains("minilm");
    let is_coding = lower.contains("coder")
        || lower.contains("code")
        || lower.contains("dev")
        || lower.contains("starcoder");
    let is_fast = lower.contains("mini")
        || lower.contains("haiku")
        || lower.contains("flash")
        || lower.contains("nano")
        || lower.contains(":1b")
        || lower.contains(":3b");
    let is_large = lower.contains("70b") || lower.contains("r1") || lower.contains("opus");
    let is_small = lower.contains(":1b")
        || lower.contains(":3b")
        || lower.contains("small")
        || lower.contains("nano");
    let is_cloud = lower.contains("gpt-") || lower.contains("claude-") || lower.contains("gemini-");

    let family = if is_embedding {
        ModelFamily::Embedding
    } else if is_coding {
        ModelFamily::AgenticCoding
    } else if is_fast {
        ModelFamily::FastChat
    } else {
        ModelFamily::GeneralReasoning
    };

    let size_class = if is_cloud {
        ModelSizeClass::Cloud
    } else if is_large {
        ModelSizeClass::Large
    } else if is_small {
        ModelSizeClass::Small
    } else {
        ModelSizeClass::Medium
    };

    let supports_tools = !is_embedding;

    let recommended_for = match family {
        ModelFamily::AgenticCoding => {
            vec!["Code Generation".to_string(), "Tool Calling".to_string()]
        }
        ModelFamily::Embedding => vec![
            "Vector Retrieval".to_string(),
            "Semantic Search".to_string(),
        ],
        ModelFamily::FastChat => vec!["Fast Chat".to_string(), "Subagent Tasks".to_string()],
        ModelFamily::GeneralReasoning => {
            vec!["General Reasoning".to_string(), "Task Planning".to_string()]
        }
        ModelFamily::VisionMultimodal => vec!["Multimodal Analysis".to_string()],
    };

    ModelGuidance {
        pattern: model_name.to_string(),
        display_name: model_name.to_string(),
        family,
        size_class,
        recommended_for,
        description: format!(
            "Detected {} model (offline heuristic categorization).",
            family
        ),
        context_window: if is_embedding { 4096 } else { 32768 },
        supports_tools,
    }
}

/// Returns the entire static catalog of pre-profiled model families.
pub fn list_model_guidance_catalog() -> Vec<ModelGuidance> {
    MODEL_CATALOG.iter().map(|e| e.to_guidance()).collect()
}
