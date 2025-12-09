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

    /// Install CA certificate to system trust store (platform-specific)
    #[cfg(target_os = "macos")]
    pub fn install_to_system(&self, pem: &str) -> Result<()> {
        use std::process::Command;

        // Save cert to temp file
        let temp_path = std::env::temp_dir().join("postgate-ca.crt");
        std::fs::write(&temp_path, pem)?;

        // Use security command to add to keychain
        let output = Command::new("security")
            .args([
                "add-trusted-cert",
                "-d",
                "-r",
                "trustRoot",
                "-k",
                "/Library/Keychains/System.keychain",
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
