use crate::error::{PostGateError, Result};
use dashmap::DashMap;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose, SanType,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::path::Path;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};

/// Certificate Authority for generating TLS certificates
#[derive(Clone)]
pub struct CertificateAuthority {
    /// CA certificate in DER format
    ca_cert_der: Vec<u8>,
    /// CA certificate in PEM format
    ca_cert_pem: String,
    /// CA private key
    ca_key_pair: Arc<KeyPair>,
    /// CA certificate for signing
    ca_cert: Arc<Certificate>,
    /// Cache of generated host certificates
    cert_cache: Arc<DashMap<String, Arc<CertifiedKey>>>,
}

/// A certified key containing certificate chain and private key
pub struct CertifiedKey {
    pub cert_chain: Vec<CertificateDer<'static>>,
    pub key: PrivateKeyDer<'static>,
}

impl CertificateAuthority {
    /// Create a new Certificate Authority (generates fresh CA)
    pub fn new() -> Result<Self> {
        let (ca_cert, ca_key_pair, ca_cert_der, ca_cert_pem) = Self::generate_ca()?;

        Ok(Self {
            ca_cert_der,
            ca_cert_pem,
            ca_key_pair: Arc::new(ca_key_pair),
            ca_cert: Arc::new(ca_cert),
            cert_cache: Arc::new(DashMap::new()),
        })
    }

    /// Load CA from files, or create new one if files don't exist
    pub fn load_or_create(data_dir: &Path) -> Result<Self> {
        let cert_path = data_dir.join("ca.crt");
        let key_path = data_dir.join("ca.key");

        // Try to load existing CA
        if cert_path.exists() && key_path.exists() {
            match Self::load_from_files(&cert_path, &key_path) {
                Ok(ca) => {
                    tracing::info!("Loaded existing CA certificate from {:?}", cert_path);
                    return Ok(ca);
                }
                Err(e) => {
                    tracing::warn!("Failed to load existing CA, generating new one: {}", e);
                }
            }
        }

        // Generate new CA and save it
        let ca = Self::new()?;
        ca.save_to_files(data_dir)?;
        tracing::info!("Generated and saved new CA certificate to {:?}", cert_path);

        Ok(ca)
    }

    /// Load CA from PEM files
    fn load_from_files(cert_path: &Path, key_path: &Path) -> Result<Self> {
        let cert_pem = std::fs::read_to_string(cert_path)?;
        let key_pem = std::fs::read_to_string(key_path)?;

        // Parse the key pair from PEM
        let key_pair = KeyPair::from_pem(&key_pem)
            .map_err(|e| PostGateError::Certificate(format!("Failed to parse CA key: {}", e)))?;

        // Parse certificate to extract information
        let pem_parsed = pem::parse(&cert_pem)
            .map_err(|e| PostGateError::Certificate(format!("Failed to parse CA cert PEM: {}", e)))?;
        
        let cert = x509_parser::parse_x509_certificate(&pem_parsed.contents())
            .map_err(|e| PostGateError::Certificate(format!("Failed to parse X509 certificate: {}", e)))?
            .1;

        // Create minimal params for CA loading - we just need to recreate the cert
        let mut params = CertificateParams::default();
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::CrlSign,
        ];
        
        // Extract subject from original cert
        let subject_cn = cert.subject()
            .iter_common_name()
            .next()
            .and_then(|cn| cn.as_str().ok())
            .unwrap_or("PostGate CA");
        
        params.distinguished_name.push(
            rcgen::DnType::CommonName,
            subject_cn,
        );

        // Recreate the certificate with the loaded key
        let ca_cert = params
            .self_signed(&key_pair)
            .map_err(|e| PostGateError::Certificate(format!("Failed to recreate CA cert: {}", e)))?;

        let cert_der = ca_cert.der().to_vec();

        Ok(Self {
            ca_cert_der: cert_der,
            ca_cert_pem: cert_pem,
            ca_key_pair: Arc::new(key_pair),
            ca_cert: Arc::new(ca_cert),
            cert_cache: Arc::new(DashMap::new()),
        })
    }

    /// Save CA certificate and key to files
    pub fn save_to_files(&self, data_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(data_dir)?;

        let cert_path = data_dir.join("ca.crt");
        let key_path = data_dir.join("ca.key");

        // Save certificate PEM
        std::fs::write(&cert_path, &self.ca_cert_pem)?;

        // Save private key PEM
        let key_pem = self.ca_key_pair.serialize_pem();
        std::fs::write(&key_path, key_pem)?;

        // Set restrictive permissions on key file (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&key_path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&key_path, perms)?;
        }

        tracing::info!("Saved CA certificate and key to {:?}", data_dir);
        Ok(())
    }

    /// Generate a new CA certificate
    fn generate_ca() -> Result<(Certificate, KeyPair, Vec<u8>, String)> {
        let mut params = CertificateParams::default();

        // Set distinguished name
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "PostGate Root CA");
        dn.push(DnType::OrganizationName, "PostGate");
        dn.push(DnType::CountryName, "US");
        params.distinguished_name = dn;

        // Set validity period (10 years)
        let now = OffsetDateTime::now_utc();
        params.not_before = now;
        params.not_after = now + Duration::days(3650);

        // Set as CA
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);

        // Set key usage
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        // Generate key pair
        let key_pair = KeyPair::generate()
            .map_err(|e| PostGateError::Certificate(format!("Failed to generate key pair: {}", e)))?;

        // Generate certificate
        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| PostGateError::Certificate(format!("Failed to generate CA cert: {}", e)))?;

        let cert_der = cert.der().to_vec();
        let cert_pem = cert.pem();

        Ok((cert, key_pair, cert_der, cert_pem))
    }

    /// Get a certificate for a specific host, generating one if necessary
    pub fn get_cert_for_host(&self, host: &str) -> Result<Arc<CertifiedKey>> {
        // Check cache first
        if let Some(cached) = self.cert_cache.get(host) {
            return Ok(cached.clone());
        }

        // Generate new certificate
        let certified_key = self.generate_host_cert(host)?;
        let certified_key = Arc::new(certified_key);

        // Cache it
        self.cert_cache.insert(host.to_string(), certified_key.clone());

        Ok(certified_key)
    }

    /// Generate a certificate for a specific host
    fn generate_host_cert(&self, host: &str) -> Result<CertifiedKey> {
        let mut params = CertificateParams::default();

        // Set distinguished name
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, host);
        dn.push(DnType::OrganizationName, "PostGate Proxy");
        params.distinguished_name = dn;

        // Set validity period (1 year)
        let now = OffsetDateTime::now_utc();
        params.not_before = now;
        params.not_after = now + Duration::days(365);

        // Set Subject Alternative Names
        let san = if host.parse::<std::net::IpAddr>().is_ok() {
            SanType::IpAddress(host.parse().unwrap())
        } else {
            SanType::DnsName(host.try_into().map_err(|e| {
                PostGateError::Certificate(format!("Invalid DNS name '{}': {}", host, e))
            })?)
        };
        params.subject_alt_names = vec![san];

        // Set extended key usage for server auth
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

        // Set key usage
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];

        // Generate key pair for host
        let key_pair = KeyPair::generate()
            .map_err(|e| PostGateError::Certificate(format!("Failed to generate host key: {}", e)))?;

        // Sign with CA
        let cert = params
            .signed_by(&key_pair, &self.ca_cert, &self.ca_key_pair)
            .map_err(|e| PostGateError::Certificate(format!("Failed to sign host cert: {}", e)))?;

        // Convert to rustls types
        let cert_der = CertificateDer::from(cert.der().to_vec());
        let ca_cert_der = CertificateDer::from(self.ca_cert_der.clone());
        let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der()));

        Ok(CertifiedKey {
            cert_chain: vec![cert_der, ca_cert_der],
            key: key_der,
        })
    }

    /// Get CA certificate in PEM format
    pub fn get_ca_pem(&self) -> &str {
        &self.ca_cert_pem
    }

    /// Get CA certificate in DER format
    pub fn get_ca_der(&self) -> &[u8] {
        &self.ca_cert_der
    }

    /// Clear the certificate cache
    pub fn clear_cache(&self) {
        self.cert_cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        (self.cert_cache.len(), 1000) // (current, max)
    }
}
