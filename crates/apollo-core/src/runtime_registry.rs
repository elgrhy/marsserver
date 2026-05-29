//! Extensible runtime dispatch and auto-installation for any language or runtime.
//!
//! Apollo resolves how to launch an agent process from its `runtime.type` field.
//! Known runtimes have built-in defaults. Unknown runtimes can supply a `command`
//! template in `agent.yaml`. Any missing runtime can be auto-installed via a URL
//! specified in `runtime.install`.

use crate::types::{AgentRuntimeConfig, RuntimeInstallConfig};
use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

// ── Launch Resolution ─────────────────────────────────────────────────────────

/// Resolve the binary and arguments needed to launch an agent process.
///
/// Resolution order:
/// 1. If `runtime.command` is set, parse it as a template (substituting `{entry}`).
/// 2. Otherwise, use the built-in dispatch table for known runtime types.
/// 3. Unknown types: execute `{entry}` as a native binary.
pub fn resolve_launch(
    runtime: &AgentRuntimeConfig,
    entry_path: &Path,
    runtimes_dir: &Path,
) -> Result<(String, Vec<String>)> {
    if let Some(template) = &runtime.command {
        return parse_template(template, entry_path);
    }

    let entry_str = entry_path.to_string_lossy().to_string();
    let kind = runtime.kind.as_str();

    let (binary, prefix_args): (&str, Vec<&str>) = match kind {
        "python3" | "python"        => ("python3",  vec![]),
        "node"    | "nodejs"        => ("node",      vec![]),
        "go"                        => ("go",        vec!["run"]),
        "deno"                      => ("deno",      vec!["run"]),
        "bun"                       => ("bun",       vec!["run"]),
        "ruby"                      => ("ruby",      vec![]),
        "php"                       => ("php",       vec![]),
        "perl"                      => ("perl",      vec![]),
        "java"                      => ("java",      vec!["-jar"]),
        "dotnet" | ".net"           => ("dotnet",    vec!["run"]),
        "gx"                        => ("gx",        vec!["run"]),
        "rust"   | "rustc"          => return native_binary(&entry_str),
        "shell"  | "sh"  | "bash"   => ("sh",        vec!["-c"]),
        "powershell" | "pwsh"       => ("pwsh",      vec!["-File"]),
        _                           => return native_binary(&entry_str),
    };

    // Prefer locally-installed runtime over system one
    let resolved_binary = prefer_local(binary, kind, runtimes_dir);
    let mut args: Vec<String> = prefix_args.iter().map(|s| s.to_string()).collect();
    args.push(entry_str);

    Ok((resolved_binary, args))
}

fn native_binary(entry_str: &str) -> Result<(String, Vec<String>)> {
    Ok((entry_str.to_string(), vec![]))
}

/// Parse a command template string like `"gx run {entry}"` or `"deno run --allow-net {entry}"`.
fn parse_template(template: &str, entry_path: &Path) -> Result<(String, Vec<String>)> {
    let entry_str = entry_path.to_string_lossy();
    let expanded = template.replace("{entry}", &entry_str);
    let mut parts = shell_words(expanded.trim()).into_iter();
    let binary = parts.next()
        .ok_or_else(|| anyhow!("Empty command template in agent.yaml"))?;
    Ok((binary, parts.collect()))
}

/// Naive shell word splitter that respects quoted strings.
fn shell_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = '"';

    for c in s.chars() {
        if in_quote {
            if c == quote_char {
                in_quote = false;
            } else {
                current.push(c);
            }
        } else if c == '"' || c == '\'' {
            in_quote = true;
            quote_char = c;
        } else if c.is_whitespace() {
            if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Return the path to a locally-installed runtime binary, or fall back to the
/// system binary name (which will be found via PATH).
fn prefer_local(binary: &str, kind: &str, runtimes_dir: &Path) -> String {
    // Check runtimes_dir/{kind}/{binary} (Unix) or runtimes_dir/{kind}/{binary}.exe (Windows)
    let candidates = if cfg!(windows) {
        vec![
            runtimes_dir.join(kind).join(format!("{}.exe", binary)),
            runtimes_dir.join(kind).join(binary),
        ]
    } else {
        vec![runtimes_dir.join(kind).join(binary)]
    };

    for candidate in candidates {
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
    }
    binary.to_string()
}

// ── Runtime Auto-Install ──────────────────────────────────────────────────────

/// Ensure the required runtime is available on this node.
///
/// Checks in order:
/// 1. System PATH (`which`).
/// 2. Local runtimes store (`base_dir/runtimes/{kind}/`).
/// 3. If neither found and `runtime.install` provides a URL, downloads and installs.
///
/// Returns `Ok(())` if the runtime is available after this call.
/// Returns `Err` if the runtime is unavailable and no install URL is provided.
pub async fn ensure_runtime(runtime: &AgentRuntimeConfig, runtimes_dir: &Path) -> Result<()> {
    let kind = &runtime.kind;

    // Shell builtins and native binaries need no runtime
    if matches!(kind.as_str(), "shell" | "sh" | "rust" | "rustc") {
        return Ok(());
    }

    // Derive the binary name from the runtime type
    let binary = runtime_binary_name(kind);

    // Already on PATH?
    if which::which(&binary).is_ok() {
        return Ok(());
    }

    // Already locally installed?
    if prefer_local(&binary, kind, runtimes_dir) != binary {
        return Ok(());
    }

    // Custom command template — extract the binary from it
    if let Some(template) = &runtime.command {
        let first = template.split_whitespace().next().unwrap_or("");
        if !first.is_empty() && which::which(first).is_ok() {
            return Ok(());
        }
    }

    // Attempt auto-install from agent.yaml's install config
    if let Some(install) = &runtime.install {
        if let Some(url) = platform_install_url(install) {
            tracing::info!(kind, url, "runtime not found — auto-installing");
            install_runtime_binary(kind, url, runtimes_dir, install.sha256.as_deref()).await?;
            return Ok(());
        }
    }

    Err(anyhow!(
        "Runtime '{}' (binary: '{}') is not installed on this node.\n\
         Add an 'install' block to your agent.yaml with download URLs, or install it manually.",
        kind,
        binary
    ))
}

fn runtime_binary_name(kind: &str) -> String {
    match kind {
        "python3" | "python" => "python3",
        "node" | "nodejs"    => "node",
        "dotnet" | ".net"    => "dotnet",
        "powershell"         => "pwsh",
        _                    => kind,
    }
    .to_string()
}

fn platform_install_url(install: &RuntimeInstallConfig) -> Option<&str> {
    if cfg!(target_os = "linux") {
        install.linux.as_deref()
    } else if cfg!(target_os = "macos") {
        install.macos.as_deref().or(install.linux.as_deref())
    } else if cfg!(windows) {
        install.windows.as_deref()
    } else {
        install.linux.as_deref()
    }
}

/// Download a runtime binary to `runtimes_dir/{kind}/` and make it executable.
/// `expected_sha256` is the hex digest declared in `agent.yaml`; when set, the
/// download is rejected if the digest does not match (supply-chain protection).
async fn install_runtime_binary(
    kind: &str,
    url: &str,
    runtimes_dir: &Path,
    expected_sha256: Option<&str>,
) -> Result<()> {
    let dest_dir = runtimes_dir.join(kind);
    fs::create_dir_all(&dest_dir)?;

    // Safe filename: strip query string and replace unsafe chars
    let raw_name = url.split('/').last().and_then(|s| s.split('?').next()).unwrap_or(kind);
    let filename: String = raw_name.chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    let filename = if filename.is_empty() { kind.to_string() } else { filename };
    let dest_path = dest_dir.join(&filename);

    tracing::info!(kind, url, "auto-installing runtime");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .context("build HTTP client")?;

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to download runtime from {}", url))?;

    if !response.status().is_success() {
        return Err(anyhow!("HTTP {} downloading runtime {}", response.status(), url));
    }

    let bytes = response.bytes().await?;

    // Verify SHA-256 digest if declared in agent.yaml
    if let Some(expected) = expected_sha256 {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let actual = hex::encode(hasher.finalize());
        if actual != expected.to_lowercase() {
            return Err(anyhow!(
                "SHA-256 mismatch for runtime '{}': expected {}, got {}",
                kind, expected, actual
            ));
        }
        tracing::info!(kind, "runtime checksum verified");
    } else {
        tracing::warn!(
            kind,
            "no sha256 declared in agent.yaml install block — skipping integrity check"
        );
    }

    // Handle archives
    let lower = filename.to_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        use flate2::read::GzDecoder;
        use tar::Archive;
        let cursor = std::io::Cursor::new(&bytes);
        let gz = GzDecoder::new(cursor);
        let mut archive = Archive::new(gz);
        archive.unpack(&dest_dir)?;
    } else if lower.ends_with(".zip") {
        let cursor = std::io::Cursor::new(&bytes);
        let mut zip = zip::ZipArchive::new(cursor)?;
        zip.extract(&dest_dir)?;
    } else {
        // Raw binary
        fs::write(&dest_path, &bytes)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dest_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&dest_path, perms)?;
        }
    }

    tracing::info!(kind, dest = ?dest_dir, "runtime installed");
    Ok(())
}

// ── Scalable Instance Storage ─────────────────────────────────────────────────
// Instances are sharded by tenant: base_dir/instances/{tenant_id}.json
// This keeps per-tenant operations O(1) regardless of total fleet size.

pub fn instances_path(base_dir: &Path, tenant_id: &str) -> PathBuf {
    base_dir.join("instances").join(format!("{}.json", sanitize_id(tenant_id)))
}

pub fn all_instances_index_path(base_dir: &Path) -> PathBuf {
    base_dir.join("instances").join("_index.json")
}

fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
