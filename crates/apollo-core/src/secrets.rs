//! Per-tenant secret storage with AES-256-GCM encryption at rest.
//!
//! A random 32-byte master key is auto-generated on first use and stored at
//! `{base_dir}/.master.key` with mode 0600 (Unix) or default DACL (Windows).
//! Each tenant's secret file is encrypted with AES-256-GCM; the nonce is
//! stored as the first 12 bytes of the ciphertext blob, which is base64-encoded
//! before being written to disk.
//!
//! Existing plaintext files (written by older nodes) are transparently migrated
//! to encrypted form on the next write.

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use aes_gcm::aead::rand_core::RngCore;
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const MASTER_KEY_FILE: &str = ".master.key";
const NONCE_LEN: usize = 12;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TenantSecrets {
    pub secrets: HashMap<String, String>,
}

// ── Master key management ──────────────────────────────────────────────────────

/// Load the master key, generating and persisting it if not present.
pub fn get_or_create_master_key(base_dir: &Path) -> Result<Vec<u8>> {
    let path = base_dir.join(MASTER_KEY_FILE);
    if path.exists() {
        let raw = fs::read(&path).context("read master key")?;
        if raw.len() == 32 {
            return Ok(raw);
        }
    }
    let mut key = vec![0u8; 32];
    OsRng.fill_bytes(&mut key);
    write_protected(&path, &key).context("write master key")?;
    Ok(key)
}

// ── Encryption helpers ────────────────────────────────────────────────────────

fn encrypt(plaintext: &[u8], master_key: &[u8]) -> Result<Vec<u8>> {
    let key  = Key::<Aes256Gcm>::from_slice(master_key);
    let cipher = Aes256Gcm::new(key);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encryption failed: {}", e))?;
    let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

fn decrypt(blob: &[u8], master_key: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < NONCE_LEN {
        return Err(anyhow::anyhow!("blob too short to contain nonce"));
    }
    let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
    let key    = Key::<Aes256Gcm>::from_slice(master_key);
    let cipher = Aes256Gcm::new(key);
    let nonce  = Nonce::from_slice(nonce_bytes);
    cipher.decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("decryption failed: {}", e))
}

// ── File paths ────────────────────────────────────────────────────────────────

fn secrets_path(base_dir: &Path, tenant_id: &str) -> PathBuf {
    base_dir
        .join("secrets")
        .join(format!("{}.json", sanitize(tenant_id)))
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn load_secrets(base_dir: &Path, tenant_id: &str) -> TenantSecrets {
    load_secrets_inner(base_dir, tenant_id).unwrap_or_default()
}

fn load_secrets_inner(base_dir: &Path, tenant_id: &str) -> Result<TenantSecrets> {
    let path = secrets_path(base_dir, tenant_id);
    if !path.exists() {
        return Ok(TenantSecrets::default());
    }

    let master_key = get_or_create_master_key(base_dir)?;
    let raw = fs::read_to_string(&path)?;
    let raw = raw.trim();

    // Try decrypting first (new format: base64-encoded encrypted blob)
    if let Ok(blob) = B64.decode(raw) {
        if let Ok(plain) = decrypt(&blob, &master_key) {
            return Ok(serde_json::from_slice(&plain)?);
        }
    }

    // Fallback: plaintext JSON (legacy format — migrate on next save)
    let secrets: TenantSecrets = serde_json::from_str(raw)?;
    Ok(secrets)
}

pub fn save_secrets(base_dir: &Path, tenant_id: &str, secrets: &TenantSecrets) -> Result<()> {
    let path = secrets_path(base_dir, tenant_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create secrets dir")?;
    }

    let master_key = get_or_create_master_key(base_dir)?;
    let plain = serde_json::to_vec_pretty(secrets)?;
    let blob  = encrypt(&plain, &master_key)?;
    let encoded = B64.encode(&blob);

    write_protected(&path, encoded.as_bytes()).context("write secrets file")?;
    Ok(())
}

pub fn delete_secrets(base_dir: &Path, tenant_id: &str) -> Result<()> {
    let path = secrets_path(base_dir, tenant_id);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Merge new secrets into the existing set and re-save encrypted.
pub fn upsert_secrets(base_dir: &Path, tenant_id: &str, new: HashMap<String, String>) -> Result<()> {
    let mut existing = load_secrets(base_dir, tenant_id);
    existing.secrets.extend(new);
    save_secrets(base_dir, tenant_id, &existing)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Write `data` to `path` with the most restrictive permissions available.
fn write_protected(path: &Path, data: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true).mode(0o600);
        let mut f = opts.open(path)?;
        use std::io::Write;
        f.write_all(data)?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        fs::write(path, data)?;
        Ok(())
    }
}

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
