use trans4mers_providers::browser::BrowserBinaryDetector;

#[test]
fn test_browser_binary_detector() {
    let result = BrowserBinaryDetector::find_binary(None);
    println!("Detected browser: {:?}", result);
    // On Windows, Edge or Chrome is almost always present
    if let Ok(path) = result {
        assert!(path.is_file());
    }
}
