use std::path::{Path, PathBuf};
use tracing::info;
use trans4mers_domain::error::Trans4mersError;

pub struct BrowserBinaryDetector;

impl BrowserBinaryDetector {
    /// Discovers an installed Chrome, Chromium, Edge, or Brave binary.
    /// Priority order:
    /// 1. Explicit override path (if specified in space configuration)
    /// 2. CHROME_BIN / BROWSER_BIN environment variables
    /// 3. Standard platform-specific installation paths
    /// 4. PATH lookup
    pub fn find_binary(space_override: Option<&str>) -> Result<PathBuf, Trans4mersError> {
        if let Some(custom) = space_override {
            let p = PathBuf::from(custom);
            if p.is_file() {
                info!(path = %p.display(), "Using space-configured browser binary");
                return Ok(p);
            }
        }

        // 2. Environment variables
        for env_var in &["CHROME_BIN", "BROWSER_BIN", "EDGE_BIN"] {
            if let Ok(val) = std::env::var(env_var) {
                let p = PathBuf::from(val);
                if p.is_file() {
                    info!(env = %env_var, path = %p.display(), "Using browser binary from environment variable");
                    return Ok(p);
                }
            }
        }

        // 3. Platform-specific candidate paths
        let candidates = Self::platform_candidates();
        for candidate in &candidates {
            if candidate.is_file() {
                info!(path = %candidate.display(), "Detected installed browser binary");
                return Ok(candidate.clone());
            }
        }

        // 4. Fallback: check PATH
        for bin_name in &[
            "chrome",
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "msedge",
            "brave",
        ] {
            if let Some(path) = which::which(bin_name).ok().filter(|p| p.is_file()) {
                info!(path = %path.display(), "Detected browser binary via PATH");
                return Ok(path);
            }
        }

        Err(Trans4mersError::BrowserNotInstalled(
            "No installed Chrome, Chromium, Edge, or Brave executable detected. \
             Please install Google Chrome or Microsoft Edge, or set the CHROME_BIN environment variable.".to_string()
        ))
    }

    #[cfg(target_os = "windows")]
    fn platform_candidates() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let program_files =
            std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".to_string());
        let program_files_x86 = std::env::var("ProgramFiles(x86)")
            .unwrap_or_else(|_| r"C:\Program Files (x86)".to_string());
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();

        let base_dirs = vec![program_files, program_files_x86, local_app_data];

        let sub_paths = &[
            r"Google\Chrome\Application\chrome.exe",
            r"Microsoft\Edge\Application\msedge.exe",
            r"BraveSoftware\Brave-Browser\Application\brave.exe",
            r"Chromium\Application\chrome.exe",
        ];

        for base in &base_dirs {
            if base.is_empty() {
                continue;
            }
            for sub in sub_paths {
                paths.push(Path::new(base).join(sub));
            }
        }

        paths
    }

    #[cfg(target_os = "linux")]
    fn platform_candidates() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/usr/bin/google-chrome-stable"),
            PathBuf::from("/usr/bin/google-chrome"),
            PathBuf::from("/usr/bin/chromium"),
            PathBuf::from("/usr/bin/chromium-browser"),
            PathBuf::from("/snap/bin/chromium"),
            PathBuf::from("/usr/bin/microsoft-edge-stable"),
            PathBuf::from("/usr/bin/brave-browser"),
        ]
    }

    #[cfg(target_os = "macos")]
    fn platform_candidates() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
            PathBuf::from("/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"),
            PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
        ]
    }
}
