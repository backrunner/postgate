use crate::error::{PostGateError, Result};
use dashmap::DashMap;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose, SanType,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
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
    /// Create a new Certificate Authority
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
