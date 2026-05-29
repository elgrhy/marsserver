//! Remote agent source resolution — URLs, git repos, archives, local paths.
//!
//! Security properties maintained:
//! - Git URLs are validated to reject flag injection (e.g. `--upload-pack`).
//! - Extracted archives are checked to ensure no entry escapes the staging dir.
//! - Downloads can optionally require a SHA-256 digest match.

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

/// Resolve an agent source string to a local directory containing `agent.yaml`.
///
/// Accepted formats:
///   - Local path: `/opt/agents/openclaw` or `./examples/openclaw`
///   - HTTPS URL to archive: `https://example.com/openclaw-1.0.tar.gz`
///   - HTTPS URL to zip: `https://example.com/openclaw-1.0.zip`
///   - Git HTTPS: `https://github.com/org/openclaw.git`
///   - Git SSH: `git@github.com:org/openclaw.git`
///
/// `staging_dir` is used as scratch space for downloads; the caller is responsible
/// for cleaning it up after `register_agent_package` completes.
pub async fn resolve_agent_source(source: &str, staging_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(staging_dir)?;

    if source.starts_with("https://") || source.starts_with("http://") {
        if looks_like_git_url(source) {
            git_clone(source, staging_dir).await
        } else {
            fetch_archive(source, staging_dir).await
        }
    } else if source.starts_with("git@") || source.ends_with(".git") {
        git_clone(source, staging_dir).await
    } else {
        // Local path — validate it exists
        let p = PathBuf::from(source);
        if !p.exists() {
            return Err(anyhow!("Source path does not exist: {}", source));
        }
        Ok(p)
    }
}

fn looks_like_git_url(url: &str) -> bool {
    url.ends_with(".git")
        || (url.contains("github.com") && !url.contains(".tar") && !url.contains(".zip"))
        || url.contains("gitlab.com/") && !url.contains(".tar") && !url.contains(".zip")
}

/// Download and extract an archive. Returns the directory containing `agent.yaml`.
/// If `expected_sha256` is provided, the download is rejected if the digest does not match.
pub async fn fetch_archive_with_checksum(
    url: &str,
    staging_dir: &Path,
    expected_sha256: Option<&str>,
) -> Result<PathBuf> {
    tracing::info!(url, "downloading agent archive");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .context("failed to build HTTP client")?;

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to connect to {}", url))?;

    if !response.status().is_success() {
        return Err(anyhow!("HTTP {} downloading {}", response.status(), url));
    }

    let bytes = response.bytes().await.context("Failed to read response body")?;

    // Verify SHA-256 if caller supplied an expected digest
    if let Some(expected) = expected_sha256 {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let actual = hex::encode(hasher.finalize());
        if actual != expected.to_lowercase() {
            return Err(anyhow!(
                "SHA-256 mismatch for {}: expected {}, got {}",
                url, expected, actual
            ));
        }
        tracing::info!("archive checksum verified");
    }

    // Derive a safe filename — strip query string, reject path traversal
    let raw_name = url
        .split('/')
        .last()
        .and_then(|s| s.split('?').next())
        .unwrap_or("agent.download");
    let filename = raw_name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect::<String>();
    if filename.is_empty() || filename == "." || filename == ".." {
        return Err(anyhow!("Could not derive a safe filename from URL: {}", url));
    }

    let download_path = staging_dir.join(&filename);
    fs::write(&download_path, &bytes)?;
    tracing::debug!(bytes = bytes.len(), path = ?download_path, "archive saved");

    let extract_dir = staging_dir.join("unpacked");
    fs::create_dir_all(&extract_dir)?;

    let lower = filename.to_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".tar.bz2") {
        extract_tar(&download_path, &extract_dir)?;
    } else if lower.ends_with(".zip") {
        extract_zip(&download_path, &extract_dir)?;
    } else {
        fs::copy(&download_path, extract_dir.join(&filename))?;
    }

    // Validate that the discovered agent.yaml is inside the extract_dir (symlink escape guard)
    let canonical_extract = fs::canonicalize(&extract_dir)
        .unwrap_or_else(|_| extract_dir.clone());
    let agent_dir = find_agent_yaml_dir(&extract_dir)?;
    let canonical_agent = fs::canonicalize(&agent_dir)
        .unwrap_or_else(|_| agent_dir.clone());
    if !canonical_agent.starts_with(&canonical_extract) {
        return Err(anyhow!(
            "Security: agent.yaml resolved outside staging directory — archive may contain symlinks"
        ));
    }

    Ok(agent_dir)
}

/// Download and extract an archive (no checksum — for internal use).
async fn fetch_archive(url: &str, staging_dir: &Path) -> Result<PathBuf> {
    fetch_archive_with_checksum(url, staging_dir, None).await
}

/// Reject git URLs that contain flag-injection patterns.
fn validate_git_url(url: &str) -> Result<()> {
    // Reject anything that looks like a git flag or shell metacharacter sequence
    for banned in &["--upload-pack", "--exec", "--no-local", "; ", "&", "|", "`", "$"] {
        if url.contains(banned) {
            return Err(anyhow!("Rejected git URL with unsafe token '{}'", banned));
        }
    }
    // Must start with https://, http://, git@, or ssh://
    if !url.starts_with("https://")
        && !url.starts_with("http://")
        && !url.starts_with("git@")
        && !url.starts_with("ssh://")
    {
        return Err(anyhow!("Git URL must use https://, http://, git@, or ssh:// scheme"));
    }
    Ok(())
}

/// Clone a git repository and return the directory containing `agent.yaml`.
async fn git_clone(url: &str, staging_dir: &Path) -> Result<PathBuf> {
    validate_git_url(url)?;
    tracing::info!(url, "cloning agent repository");

    let clone_dir = staging_dir.join("repo");
    let clone_str = clone_dir.to_str().context("clone path is not valid UTF-8")?;
    let output = tokio::process::Command::new("git")
        .args(["clone", "--depth", "1", "--", url, clone_str])
        .output()
        .await
        .context("git not found — install git 2.30+")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git clone failed: {}", stderr));
    }

    find_agent_yaml_dir(&clone_dir)
}

fn extract_tar(archive: &Path, dest: &Path) -> Result<()> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let file = File::open(archive)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    archive.unpack(dest).context("Failed to extract tar archive")?;
    Ok(())
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)
        .with_context(|| format!("Not a valid ZIP: {:?}", archive))?;
    zip.extract(dest)
        .with_context(|| format!("Failed to extract ZIP to {:?}", dest))?;
    Ok(())
}

/// Walk up to 3 levels deep looking for a directory containing `agent.yaml`.
fn find_agent_yaml_dir(root: &Path) -> Result<PathBuf> {
    if root.join("agent.yaml").exists() {
        return Ok(root.to_path_buf());
    }
    // Check immediate children
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let candidate = entry.path();
                if candidate.join("agent.yaml").exists() {
                    return Ok(candidate);
                }
                // One more level
                if let Ok(sub) = fs::read_dir(&candidate) {
                    for s in sub.flatten() {
                        if s.file_type().map(|t| t.is_dir()).unwrap_or(false)
                            && s.path().join("agent.yaml").exists()
                        {
                            return Ok(s.path());
                        }
                    }
                }
            }
        }
    }
    Err(anyhow!(
        "No agent.yaml found under {:?}. Make sure the archive contains an agent.yaml at its root.",
        root
    ))
}

/// Create a staging directory under `base_dir/staging/{id}`.
pub fn make_staging_dir(base_dir: &Path) -> Result<PathBuf> {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = base_dir.join("staging").join(id.to_string());
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Remove the staging directory after registration completes.
pub fn cleanup_staging(staging_dir: &Path) {
    let _ = fs::remove_dir_all(staging_dir);
}
