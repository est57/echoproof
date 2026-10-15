//! A per-project certificate authority, so the outbound proxy can read HTTPS calls from test
//! builds that trust it. The CA certificate is rebuilt from its key and fixed parameters
//! instead of being parsed, which keeps the issuer name and key identifier identical.

use anyhow::{Context, Result, bail};
use chrono::{Datelike, Duration, Utc};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, date_time_ymd,
};
use std::path::{Path, PathBuf};

use crate::paths::write_private;

pub const DIR: &str = ".echoproof";
const CA_FILE: &str = "ca.pem";
const KEY_FILE: &str = "ca-key.pem";
const VALID_DAYS: i64 = 90;

pub struct Authority {
    key: KeyPair,
    params: CertificateParams,
}

pub struct Leaf {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
}

pub fn ca_path(dir: &Path) -> PathBuf {
    dir.join(CA_FILE)
}

/// Writes a new CA into `dir` and returns the certificate path.
pub fn init(dir: &Path) -> Result<PathBuf> {
    let key = KeyPair::generate()?;
    let cert = params(VALID_DAYS).self_signed(&key)?;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    write_private(&dir.join(KEY_FILE), key.serialize_pem().as_bytes())?;
    std::fs::write(dir.join(CA_FILE), cert.pem())?;
    Ok(ca_path(dir))
}

impl Authority {
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(KEY_FILE);
        check_private(&path)?;
        let pem = std::fs::read_to_string(&path).with_context(|| {
            format!(
                "cannot read {}; run `echoproof ca init` first",
                path.display()
            )
        })?;
        Ok(Self {
            key: KeyPair::from_pem(&pem).context("invalid CA key")?,
            params: params(VALID_DAYS),
        })
    }

    /// A short-lived certificate for one host, kept in memory only.
    pub fn issue(&self, host: &str) -> Result<Leaf> {
        let key = KeyPair::generate()?;
        let mut leaf = CertificateParams::new(vec![host.to_string()])?;
        leaf.distinguished_name.push(DnType::CommonName, host);
        let today = Utc::now().date_naive();
        let start = today - Duration::days(1);
        let end = today + Duration::days(30);
        leaf.not_before = date_time_ymd(start.year(), start.month() as u8, start.day() as u8);
        leaf.not_after = date_time_ymd(end.year(), end.month() as u8, end.day() as u8);
        leaf.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        leaf.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        leaf.use_authority_key_identifier_extension = true;
        let issuer = Issuer::from_params(&self.params, &self.key);
        let cert = leaf.signed_by(&key, &issuer)?;
        Ok(Leaf {
            cert_der: cert.der().to_vec(),
            key_der: key.serialize_der(),
        })
    }
}

fn params(valid_days: i64) -> CertificateParams {
    let mut params = CertificateParams::default();
    let mut name = DistinguishedName::new();
    name.push(DnType::OrganizationName, "Echoproof");
    name.push(
        DnType::CommonName,
        "Echoproof test CA (do not trust system-wide)",
    );
    params.distinguished_name = name;
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let today = Utc::now().date_naive();
    let end = today + Duration::days(valid_days);
    params.not_before = date_time_ymd(today.year(), today.month() as u8, today.day() as u8);
    params.not_after = date_time_ymd(end.year(), end.month() as u8, end.day() as u8);
    params
}

/// The key signs certificates every test build trusts, so it must not be readable by others.
fn check_private(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path)
            && meta.permissions().mode() & 0o077 != 0
        {
            bail!(
                "{} is readable by other users; run `chmod 600 {}`",
                path.display(),
                path.display()
            );
        }
    }
    Ok(())
}

pub fn command(action: crate::cli::CaAction) -> Result<()> {
    use crate::cli::CaAction;
    let dir = Path::new(DIR);
    match action {
        CaAction::Init => {
            if dir.join(KEY_FILE).exists() {
                bail!(
                    "{} already exists; delete {DIR}/ to start over",
                    dir.join(KEY_FILE).display()
                );
            }
            let cert = init(dir)?;
            ignore_in_git(dir)?;
            eprintln!(
                "wrote {} (valid {VALID_DAYS} days) and its key. Trust it only in test builds, \
                 never system-wide: see `echoproof ca env --runtime <php|node|python|go>`.",
                cert.display()
            );
        }
        CaAction::Env { runtime, proxy } => {
            let cert = std::fs::canonicalize(ca_path(dir))
                .context("no CA yet; run `echoproof ca init` first")?;
            print!("{}", crate::ca_env::lines(runtime, &cert, &proxy));
        }
    }
    Ok(())
}

fn ignore_in_git(dir: &Path) -> Result<()> {
    std::fs::write(dir.join(".gitignore"), "*\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("echoproof-ca-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn issues_host_certificates_from_a_saved_key() {
        let dir = temp_dir("issue");
        let cert = init(&dir).unwrap();
        assert!(
            std::fs::read_to_string(cert)
                .unwrap()
                .contains("BEGIN CERTIFICATE")
        );
        let authority = Authority::load(&dir).unwrap();
        let leaf = authority.issue("pay.example.test").unwrap();
        assert!(!leaf.cert_der.is_empty() && !leaf.key_der.is_empty());
        assert!(authority.issue("127.0.0.1").is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_key_other_users_can_read() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("perms");
        init(&dir).unwrap();
        let key = dir.join(KEY_FILE);
        assert_eq!(
            std::fs::metadata(&key).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
        let error = Authority::load(&dir).err().unwrap().to_string();
        assert!(error.contains("readable by other users"), "{error}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
