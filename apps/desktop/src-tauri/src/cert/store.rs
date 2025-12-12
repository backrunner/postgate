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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn load_ca_cert(&self) -> Result<String> {
        let path = self.ca_cert_path();
        if !path.exists() {
            return Err(PostGateError::NotFound("CA certificate not found".into()));
        }
        Ok(std::fs::read_to_string(path)?)
    }

    /// Check if CA certificate exists
    #[allow(dead_code)]
    pub fn ca_exists(&self) -> bool {
        self.ca_cert_path().exists()
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

        let output = Command::new("osascript")
            .args(["-e", &script])
            .output()?;

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
            if stderr.contains("authorization was denied") || stderr.contains("no user interaction") {
                return Err(PostGateError::Certificate(
                    "Cannot install certificate in development mode. Please either: \
                    1) Build and run the packaged .app, or \
                    2) Export the certificate and install manually via Keychain Access".into(),
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
            .args([
                "-addstore",
                "-user",
                "ROOT",
                temp_path.to_str().unwrap(),
            ])
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
        use std::process::Command;

        // Try different methods based on distro

        // Method 1: Debian/Ubuntu
        let cert_dir = PathBuf::from("/usr/local/share/ca-certificates");
        if cert_dir.exists() {
            let cert_path = cert_dir.join("postgate-ca.crt");
            std::fs::write(&cert_path, pem)?;

            let output = Command::new("update-ca-certificates").output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(PostGateError::Certificate(format!(
                    "Failed to update certificates: {}",
                    stderr
                )));
            }

            return Ok(());
        }

        // Method 2: Fedora/RHEL
        let cert_dir = PathBuf::from("/etc/pki/ca-trust/source/anchors");
        if cert_dir.exists() {
            let cert_path = cert_dir.join("postgate-ca.crt");
            std::fs::write(&cert_path, pem)?;

            let output = Command::new("update-ca-trust").arg("extract").output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(PostGateError::Certificate(format!(
                    "Failed to update certificates: {}",
                    stderr
                )));
            }

            return Ok(());
        }

        Err(PostGateError::Certificate(
            "Could not find system certificate directory".into(),
        ))
    }
}
