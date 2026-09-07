use trans4mers_domain::error::Trans4mersError;
use trans4mers_providers::OllamaModelDetector;

#[tokio::test]
async fn test_ollama_detector_offline() {
    // 1. Scan disk for installed models (pure filesystem, zero network egress)
    let models = OllamaModelDetector::scan_local_disk_models();
    println!("Detected local models on disk: {:?}", models);

    let has_embedding = OllamaModelDetector::has_local_embedding_model();
    println!("Has local embedding model on disk: {}", has_embedding);

    // 2. Non-loopback endpoint must be rejected with PolicyDenied
    let external_err =
        OllamaModelDetector::check_loopback_endpoint("https://api.openai.com/v1").await;
    assert!(external_err.is_err());
    match external_err {
        Err(Trans4mersError::PolicyDenied { capability }) => {
            assert_eq!(capability, "loopback_only");
        }
        _ => panic!("Expected PolicyDenied error for non-loopback endpoint"),
    }
}
