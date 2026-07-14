use crate::error::{PostGateError, Result};
use std::path::PathBuf;

/// Certificate storage helper
pub struct CertStore {
    data_dir: PathBuf,
}

impl CertStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Get the path to the CA certificate file
    pub fn ca_cert_path(&self) -> PathBuf {
        self.data_dir.join("ca.crt")
    }

    /// Get the path to the CA private key file
    pub fn ca_key_path(&self) -> PathBuf {
        self.data_dir.join("ca.key")
    }

    /// Save CA certificate to disk
    pub fn save_ca_cert(&self, pem: &str) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::write(self.ca_cert_path(), pem)?;
        Ok(())
    }

    /// Load CA certificate from disk
    pub fn load_ca_cert(&self) -> Result<String> {
        let path = self.ca_cert_path();
        if !path.exists() {
            return Err(PostGateError::NotFound("CA certificate not found".into()));
        }
        Ok(std::fs::read_to_string(path)?)
    }

    /// Check if CA certificate exists
    pub fn ca_exists(&self) -> bool {
        self.ca_cert_path().exists()
    }

    /// Check whether this exact CA certificate is present in the system trust
    /// location PostGate installs to.
    pub fn is_installed(&self, pem: &str) -> Result<bool> {
        is_installed_platform(pem, &self.data_dir)
    }

    /// Install CA certificate to system trust store (platform-specific)
    #[cfg(target_os = "macos")]
    pub fn install_to_system(&self, pem: &str) -> Result<()> {
        use std::process::Command;

        // Save cert to temp file
        let temp_path = std::env::temp_dir().join("postgate-ca.crt");
        std::fs::write(&temp_path, pem)?;

        let temp_path_str = temp_path.to_str().unwrap();

        // Use osascript to run the security command with administrator privileges
        // This will prompt the user for their password via the native macOS dialog
        // Note: This may fail in development mode (cargo tauri dev) due to missing
        // GUI session context. It works correctly when running the packaged .app
        let script = format!(
            r#"do shell script "security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain '{}'" with administrator privileges"#,
            temp_path_str
        );

        let output = Command::new("osascript").args(["-e", &script]).output()?;

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Check if user cancelled
            if stderr.contains("User canceled") || stderr.contains("-128") {
                return Err(PostGateError::Certificate(
                    "Installation cancelled by user".into(),
                ));
            }
            // Check for authorization denied (common in dev mode)
            if stderr.contains("authorization was denied") || stderr.contains("no user interaction")
            {
                return Err(PostGateError::Certificate(
                    "Cannot install certificate in development mode. Please either: \
                    1) Build and run the packaged .app, or \
                    2) Export the certificate and install manually via Keychain Access"
                        .into(),
                ));
            }
            return Err(PostGateError::Certificate(format!(
                "Failed to install certificate: {}",
                stderr
            )));
        }

        tracing::info!("CA certificate installed to system keychain successfully");
        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub fn install_to_system(&self, pem: &str) -> Result<()> {
        use std::process::Command;

        // Save cert to temp file
        let temp_path = std::env::temp_dir().join("postgate-ca.crt");
        std::fs::write(&temp_path, pem)?;

        // Use certutil to add to root store
        let output = Command::new("certutil")
            .args(["-addstore", "-user", "ROOT", temp_path.to_str().unwrap()])
            .output()?;

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PostGateError::Certificate(format!(
                "Failed to install certificate: {}",
                stderr
            )));
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub fn install_to_system(&self, pem: &str) -> Result<()> {
        use std::io::Write;
        use std::process::Command;

        let (cert_path, install_script) =
            if PathBuf::from("/usr/local/share/ca-certificates").exists() {
                (
                    PathBuf::from("/usr/local/share/ca-certificates/postgate-ca.crt"),
                    "install -m 0644 \"$1\" \"$2\" && update-ca-certificates",
                )
            } else if PathBuf::from("/etc/pki/ca-trust/source/anchors").exists() {
                (
                    PathBuf::from("/etc/pki/ca-trust/source/anchors/postgate-ca.crt"),
                    "install -m 0644 \"$1\" \"$2\" && update-ca-trust extract",
                )
            } else {
                return Err(PostGateError::Certificate(
                    "Could not find a supported system certificate directory".into(),
                ));
            };

        let mut temp_file = tempfile::NamedTempFile::new()?;
        temp_file.write_all(pem.as_bytes())?;
        temp_file.flush()?;

        let output = Command::new("pkexec")
            .args(["sh", "-c", install_script, "postgate-ca-install"])
            .arg(temp_file.path())
            .arg(&cert_path)
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    PostGateError::Certificate(
                        "pkexec is required to install the certificate. Export it and install it manually instead."
                            .into(),
                    )
                } else {
                    PostGateError::Io(error)
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.to_ascii_lowercase().contains("cancel")
                || stderr.to_ascii_lowercase().contains("dismiss")
            {
                return Err(PostGateError::Certificate(
                    "Installation cancelled by user".into(),
                ));
            }
            return Err(PostGateError::Certificate(format!(
                "Failed to install certificate: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }
}

fn normalized_pem_blocks(contents: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut in_block = false;

    for line in contents.lines().map(str::trim) {
        if line == "-----BEGIN CERTIFICATE-----" {
            in_block = true;
            current.clear();
        }

        if in_block {
            current.push(line.to_string());
        }

        if line == "-----END CERTIFICATE-----" && in_block {
            blocks.push(current.join("\n"));
            current.clear();
            in_block = false;
        }
    }

    blocks
}

fn pem_matches(contents: &str, expected_pem: &str) -> bool {
    let expected = normalized_pem_blocks(expected_pem)
        .into_iter()
        .next()
        .unwrap_or_else(|| expected_pem.trim().to_string());

    normalized_pem_blocks(contents)
        .into_iter()
        .any(|block| block == expected)
}

#[cfg(target_os = "macos")]
fn is_installed_platform(pem: &str, _data_dir: &std::path::Path) -> Result<bool> {
    use std::process::Command;

    let output = Command::new("security")
        .args(["find-certificate", "-a", "-p", "-c", "PostGate Root CA"])
        .output()?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(pem_matches(&stdout, pem))
}

#[cfg(target_os = "windows")]
fn is_installed_platform(_pem: &str, _data_dir: &std::path::Path) -> Result<bool> {
    use std::process::Command;

    let output = Command::new("certutil")
        .args(["-user", "-store", "ROOT", "PostGate Root CA"])
        .output()?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.contains("PostGate Root CA"))
}

#[cfg(target_os = "linux")]
fn is_installed_platform(pem: &str, _data_dir: &std::path::Path) -> Result<bool> {
    let candidates = [
        std::path::PathBuf::from("/usr/local/share/ca-certificates/postgate-ca.crt"),
        std::path::PathBuf::from("/etc/pki/ca-trust/source/anchors/postgate-ca.crt"),
    ];

    for path in candidates {
        if path.exists() {
            let contents = std::fs::read_to_string(path)?;
            if pem_matches(&contents, pem) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn is_installed_platform(_pem: &str, _data_dir: &std::path::Path) -> Result<bool> {
    Ok(false)
}
