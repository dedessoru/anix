use anix_domain::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

fn failure(e: impl std::fmt::Display) -> Error {
    Error::Storage(e.to_string())
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelManifest {
    pub schema_version: u32,
    pub logical_name: String,
    pub repository: String,
    pub revision: String,
    pub filename: String,
    pub directory: String,
    pub size: u64,
    pub sha256: String,
    pub license: String,
    pub runtime: String,
    pub family: String,
    pub parameters: String,
    pub quantization: String,
    pub url: String,
}
impl ModelManifest {
    pub fn read(path: &Path) -> Result<Self> {
        let manifest: Self =
            toml::from_str(&fs::read_to_string(path).map_err(failure)?).map_err(failure)?;
        manifest.validate()?;
        Ok(manifest)
    }
    fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.size == 0
            || self.revision.len() != 40
            || !self.revision.bytes().all(|b| b.is_ascii_hexdigit())
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::Validation("unpinned model manifest".into()));
        }
        for part in [&self.filename, &self.directory] {
            if part.is_empty() || part.contains(['/', '\\', ':']) || part == "." || part == ".." {
                return Err(Error::Validation("unsafe model path".into()));
            }
        }
        Ok(())
    }
    pub fn model_path(&self, root: &Path) -> PathBuf {
        root.join("models")
            .join(&self.directory)
            .join(&self.filename)
    }
    pub fn verify(&self, path: &Path) -> Result<()> {
        if fs::metadata(path).map_err(failure)?.len() != self.size {
            return Err(Error::Validation("model size mismatch".into()));
        }
        let mut reader = File::open(path).map_err(failure)?;
        let mut hash = Sha256::new();
        let mut buf = [0u8; 65536];
        loop {
            let n = reader.read(&mut buf).map_err(failure)?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
        }
        if format!("{:x}", hash.finalize()) != self.sha256 {
            return Err(Error::Validation("model checksum mismatch".into()));
        }
        Ok(())
    }
}
struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
pub fn bootstrap(root: &Path, manifest: &ModelManifest) -> Result<PathBuf> {
    manifest.validate()?;
    let target = manifest.model_path(root);
    if target.exists() {
        manifest.verify(&target)?;
        return Ok(target);
    }
    let parent = target
        .parent()
        .ok_or_else(|| Error::Validation("model parent".into()))?;
    fs::create_dir_all(parent).map_err(failure)?;
    let staging = Staging(parent.join(format!(
        "{}.{}.partial",
        manifest.filename,
        anix_domain::ActionId::new()
    )));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging.0)
        .map_err(failure)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(1800))
        .build()
        .map_err(failure)?;
    let mut response = client
        .get(&manifest.url)
        .send()
        .map_err(failure)?
        .error_for_status()
        .map_err(failure)?;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buf = [0u8; 65536];
    loop {
        let n = response.read(&mut buf).map_err(failure)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > manifest.size {
            return Err(Error::Validation("oversized download".into()));
        }
        hash.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(failure)?;
    }
    if count != manifest.size || format!("{:x}", hash.finalize()) != manifest.sha256 {
        return Err(Error::Validation("download verification failed".into()));
    }
    file.sync_all().map_err(failure)?;
    drop(file);
    // hard_link atomically installs without overwriting a winner of concurrent bootstrap.
    match fs::hard_link(&staging.0, &target) {
        Ok(()) => {}
        Err(e) if target.exists() => {
            manifest.verify(&target)?;
            let _ = e;
        }
        Err(e) => return Err(failure(e)),
    }
    Ok(target)
}
