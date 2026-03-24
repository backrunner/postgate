use crate::error::{PostGateError, Result};
use moka::sync::Cache;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose, SanType,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use time::{Duration, OffsetDateTime};

/// Maximum number of cached host certificates
const CERT_CACHE_MAX_CAPACITY: u64 = 1000;

/// TTL for cached certificates (23 hours - less than cert validity to ensure fresh certs)
const CERT_CACHE_TTL_HOURS: u64 = 23;

/// Host certificate validity period in days
const HOST_CERT_VALIDITY_DAYS: i64 = 30;

/// Certificate Authority for generating TLS certificates
#[derive(Clone)]
pub struct CertificateAuthority {
    /// CA certificate in DER format
    ca_cert_der: Vec<u8>,
    /// CA certificate in PEM format
    ca_cert_pem: String,
    /// CA private key
    ca_key_pair: Arc<KeyPair>,
    /// CA issuer for signing host certificates (rcgen 0.14+)
    ca_issuer: Arc<Issuer<'static, KeyPair>>,
    /// Cache of generated host certificates (with TTL and max size)
    cert_cache: Cache<String, Arc<CertifiedKey>>,
}

/// A certified key containing certificate chain and private key
pub struct CertifiedKey {
    pub cert_chain: Vec<CertificateDer<'static>>,
    pub key: PrivateKeyDer<'static>,
}

impl CertificateAuthority {
    /// Create a new Certificate Authority (generates fresh CA)
    pub fn new() -> Result<Self> {
        let (_ca_cert, ca_key_pair, ca_cert_der, ca_cert_pem) = Self::generate_ca()?;

        // Create a second KeyPair for the Issuer (KeyPair doesn't implement Clone in rcgen 0.14)
        let key_pem = ca_key_pair.serialize_pem();
        let issuer_key_pair = KeyPair::from_pem(&key_pem)
            .map_err(|e| PostGateError::Certificate(format!("Failed to re-parse CA key: {}", e)))?;

        // Create issuer from the generated CA for signing host certs
        let cert_der_ref = CertificateDer::from(ca_cert_der.as_slice());
        let ca_issuer = Issuer::from_ca_cert_der(&cert_der_ref, issuer_key_pair)
            .map_err(|e| PostGateError::Certificate(format!("Failed to create CA issuer: {}", e)))?;

        Ok(Self {
            ca_cert_der,
            ca_cert_pem,
            ca_key_pair: Arc::new(ca_key_pair),
            ca_issuer: Arc::new(ca_issuer),
            cert_cache: Self::create_cache(),
        })
    }

    /// Create a certificate cache with TTL and max capacity
    fn create_cache() -> Cache<String, Arc<CertifiedKey>> {
        Cache::builder()
            .max_capacity(CERT_CACHE_MAX_CAPACITY)
            .time_to_live(StdDuration::from_secs(CERT_CACHE_TTL_HOURS * 3600))
            .build()
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

        // Parse certificate PEM to get DER bytes
        let pem_parsed = pem::parse(&cert_pem)
            .map_err(|e| PostGateError::Certificate(format!("Failed to parse CA cert PEM: {}", e)))?;
        
        let cert_der_bytes = pem_parsed.contents().to_vec();
        let cert_der_ref = CertificateDer::from(cert_der_bytes.as_slice());

        // Parse using x509-parser to validate and extract info
        let (_, x509_cert) = x509_parser::parse_x509_certificate(&cert_der_bytes)
            .map_err(|e| PostGateError::Certificate(format!("Failed to parse X509 certificate: {}", e)))?;

        // Create a second KeyPair for the Issuer (KeyPair doesn't implement Clone in rcgen 0.14)
        let issuer_key_pair = KeyPair::from_pem(&key_pem)
            .map_err(|e| PostGateError::Certificate(format!("Failed to re-parse CA key for issuer: {}", e)))?;

        // Create Issuer from the existing CA certificate for signing host certs (rcgen 0.14+)
        let ca_issuer = Issuer::from_ca_cert_der(&cert_der_ref, issuer_key_pair)
            .map_err(|e| PostGateError::Certificate(format!("Failed to create CA issuer: {}", e)))?;

        // Verify the loaded certificate is a CA
        if !x509_cert.is_ca() {
            return Err(PostGateError::Certificate(
                "Loaded certificate is not a CA".into(),
            ));
        }

        Ok(Self {
            ca_cert_der: cert_der_bytes,
            ca_cert_pem: cert_pem,
            ca_key_pair: Arc::new(key_pair),
            ca_issuer: Arc::new(ca_issuer),
            cert_cache: Self::create_cache(),
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
            return Ok(cached);
        }

        // Generate new certificate
        let certified_key = self.generate_host_cert(host)?;
        let certified_key = Arc::new(certified_key);

        // Cache it (with automatic TTL and LRU eviction)
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

        // Set validity period (30 days - short-lived for security)
        let now = OffsetDateTime::now_utc();
        params.not_before = now;
        params.not_after = now + Duration::days(HOST_CERT_VALIDITY_DAYS);

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

        // Sign with CA using the Issuer (rcgen 0.14+)
        let cert = params
            .signed_by(&key_pair, &self.ca_issuer)
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
    #[allow(dead_code)]
    pub fn get_ca_der(&self) -> &[u8] {
        &self.ca_cert_der
    }

    /// Clear the certificate cache
    #[allow(dead_code)]
    pub fn clear_cache(&self) {
        self.cert_cache.invalidate_all();
    }

    /// Get cache statistics
    #[allow(dead_code)]
    pub fn cache_stats(&self) -> (u64, u64) {
        (self.cert_cache.entry_count(), CERT_CACHE_MAX_CAPACITY)
    }
}
