use anyhow::{Context, Result, bail};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// Confines paths that come from an MCP client to the project directory.
pub struct Root(PathBuf);

impl Root {
    pub fn current() -> Result<Self> {
        Ok(Self(std::env::current_dir()?.canonicalize()?))
    }

    pub fn readable(&self, path: &str) -> Result<PathBuf> {
        let resolved = self
            .0
            .join(path)
            .canonicalize()
            .with_context(|| format!("cannot open {path}"))?;
        self.check(path, &resolved)?;
        Ok(resolved)
    }

    pub fn writable(&self, path: &str) -> Result<PathBuf> {
        let joined = self.0.join(path);
        let Some(Component::Normal(name)) = joined.components().next_back() else {
            bail!("{path} is not a file path");
        };
        let parent = joined
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .with_context(|| format!("the directory of {path} does not exist"))?;
        self.check(path, &parent)?;
        let resolved = parent.join(name);
        if resolved
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            bail!("refusing to write through symlink {path}");
        }
        Ok(resolved)
    }

    fn check(&self, original: &str, resolved: &Path) -> Result<()> {
        if !resolved.starts_with(&self.0) {
            bail!(
                "{original} is outside the project directory {}",
                self.0.display()
            );
        }
        Ok(())
    }
}

/// Writes a file only the current user can read.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    #[cfg(unix)]
    file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    file.write_all(bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Root;

    #[test]
    fn paths_stay_in_the_project() {
        let dir = std::env::temp_dir().join(format!("echoproof-paths-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/a.jsonl"), "").unwrap();
        let root = Root(dir.canonicalize().unwrap());
        assert!(root.readable("sub/a.jsonl").is_ok());
        assert!(root.readable("../../etc/passwd").is_err());
        assert!(root.readable("/etc/passwd").is_err());
        assert!(root.writable("sub/out.jsonl").is_ok());
        assert!(root.writable("../out.jsonl").is_err());
        assert!(root.writable("missing/out.jsonl").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/hosts", dir.join("link")).unwrap();
            assert!(root.readable("link").is_err());
            assert!(root.writable("link").is_err());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
