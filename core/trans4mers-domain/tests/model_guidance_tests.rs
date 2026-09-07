use trans4mers_domain::model_guidance::{
    ModelFamily, ModelSizeClass, get_model_guidance, list_model_guidance_catalog,
};

#[test]
fn test_catalog_retrieval_known_models() {
    // 1. Qwen 2.5 Coder
    let qwen = get_model_guidance("qwen2.5-coder:7b");
    assert_eq!(qwen.family, ModelFamily::AgenticCoding);
    assert_eq!(qwen.size_class, ModelSizeClass::Medium);
    assert!(qwen.supports_tools);
    assert!(qwen.context_window >= 32768);
    assert!(
        qwen.recommended_for
            .contains(&"Autonomous Coding".to_string())
    );

    // 2. Claude 3.5 Sonnet
    let claude = get_model_guidance("claude-3-5-sonnet-20241022");
    assert_eq!(claude.family, ModelFamily::AgenticCoding);
    assert_eq!(claude.size_class, ModelSizeClass::Cloud);
    assert!(claude.supports_tools);
    assert_eq!(claude.context_window, 200000);

    // 3. Nomic Embed
    let nomic = get_model_guidance("nomic-embed-text:latest");
    assert_eq!(nomic.family, ModelFamily::Embedding);
    assert_eq!(nomic.size_class, ModelSizeClass::Small);
    assert!(!nomic.supports_tools);

    // 4. DeepSeek R1
    let r1 = get_model_guidance("deepseek-r1:70b");
    assert_eq!(r1.family, ModelFamily::GeneralReasoning);
    assert_eq!(r1.size_class, ModelSizeClass::Large);
    assert!(r1.supports_tools);
}

#[test]
fn test_heuristic_fallback_unknown_models() {
    // Uncataloged model with "coder" in name
    let custom_coder = get_model_guidance("my-custom-coder-v1");
    assert_eq!(custom_coder.family, ModelFamily::AgenticCoding);
    assert!(custom_coder.supports_tools);

    // Uncataloged model with "embed" in name
    let custom_embed = get_model_guidance("custom-embedder-small");
    assert_eq!(custom_embed.family, ModelFamily::Embedding);
    assert!(!custom_embed.supports_tools);

    // Uncataloged small fast model
    let mini_model = get_model_guidance("random-llm:1b");
    assert_eq!(mini_model.family, ModelFamily::FastChat);
    assert_eq!(mini_model.size_class, ModelSizeClass::Small);
    assert!(mini_model.supports_tools);

    // Uncataloged large model
    let big_model = get_model_guidance("custom-model-70b");
    assert_eq!(big_model.size_class, ModelSizeClass::Large);
}

#[test]
fn test_catalog_integrity() {
    let catalog = list_model_guidance_catalog();
    assert!(
        catalog.len() >= 20,
        "Catalog should have at least 20 curated models"
    );

    for model in catalog {
        assert!(!model.pattern.is_empty(), "Pattern cannot be empty");
        assert!(
            !model.display_name.is_empty(),
            "Display name cannot be empty"
        );
        assert!(!model.description.is_empty(), "Description cannot be empty");
        assert!(model.context_window > 0, "Context window must be > 0");
        assert!(
            !model.recommended_for.is_empty(),
            "Recommendations cannot be empty"
        );
    }
}
