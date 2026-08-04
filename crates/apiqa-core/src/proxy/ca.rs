//! Local certificate authority generation and loading.

use std::path::Path;

use hudsucker::{
    certificate_authority::RcgenAuthority,
    rcgen::{
        BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
    },
    rustls::crypto::aws_lc_rs,
};
use sha2::{Digest, Sha256};

use super::model::CertificateInfo;

pub fn generate_ca(directory: &Path) -> anyhow::Result<CertificateInfo> {
    std::fs::create_dir_all(directory)?;
    let key = KeyPair::generate()?;
    let mut params = CertificateParams::default();
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, "App Tester Local Inspection CA");
    name.push(DnType::OrganizationName, "App Tester");
    params.distinguished_name = name;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let certificate = params.self_signed(&key)?;
    let cert_path = directory.join("app-tester-ca.pem");
    let key_path = directory.join("app-tester-ca-key.pem");
    std::fs::write(&cert_path, certificate.pem())?;
    std::fs::write(&key_path, key.serialize_pem())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
    }
    let fingerprint = format!("{:x}", Sha256::digest(certificate.der()));
    Ok(CertificateInfo {
        certificate_path: cert_path,
        fingerprint_sha256: fingerprint,
    })
}

pub fn load_authority(directory: &Path) -> anyhow::Result<RcgenAuthority> {
    let key = std::fs::read_to_string(directory.join("app-tester-ca-key.pem"))?;
    let cert = std::fs::read_to_string(directory.join("app-tester-ca.pem"))?;
    let key = KeyPair::from_pem(&key)?;
    let issuer = Issuer::from_ca_cert_pem(&cert, key)?;
    Ok(RcgenAuthority::new(
        issuer,
        1_000,
        aws_lc_rs::default_provider(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn ca_generation_writes_restricted_key() {
        let root = std::env::temp_dir().join(format!("app-tester-ca-{}", Uuid::new_v4()));
        let info = generate_ca(&root).unwrap();
        assert!(info.certificate_path.exists());
        assert_eq!(info.fingerprint_sha256.len(), 64);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn generated_ca_can_be_loaded_as_an_authority() {
        let root = std::env::temp_dir().join(format!("app-tester-ca-{}", Uuid::new_v4()));
        let info = generate_ca(&root).unwrap();
        assert!(load_authority(info.certificate_path.parent().unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }
}
