//! Cross-platform agent process runtime.
//!
//! Unix:    process groups via setpgid + killpg (nix crate)
//! Windows: new process group via CREATE_NEW_PROCESS_GROUP + taskkill /F /T

use crate::AgentRuntime;
use apollo_core::types::{AgentSpec, AgentInstance, ExecutionStats};
use apollo_core::runtime_registry::{resolve_launch, ensure_runtime, instances_path};
use apollo_core::secrets::load_secrets;
use async_trait::async_trait;
use anyhow::{Result, anyhow};
use std::process::Stdio;
use tokio::process::Command;
use std::time::{SystemTime, Duration};
use std::path::{Path, PathBuf};
use std::fs::{self, OpenOptions};
use sysinfo::{System, Pid};

#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid as NixPid;

// ── ProcessRuntime ────────────────────────────────────────────────────────────

pub struct ProcessRuntime {
    pub base_dir: PathBuf,
}

impl ProcessRuntime {
    pub fn new(base_dir: PathBuf) -> Self {
        let base_dir = fs::canonicalize(&base_dir).unwrap_or(base_dir);
        Self { base_dir }
    }

    fn agent_code_dir(&self, name: &str) -> PathBuf {
        self.base_dir.join("agents").join(name)
    }

    fn tenant_workspace_dir(&self, tenant_id: &str, name: &str) -> PathBuf {
        self.base_dir.join("tenants").join(tenant_id).join(name)
    }

    fn tenant_log_file(&self, tenant_id: &str, name: &str) -> PathBuf {
        self.base_dir.join("logs").join(tenant_id).join(format!("{}.log", name))
    }

    fn runtimes_dir(&self) -> PathBuf {
        self.base_dir.join("runtimes")
    }

    fn pid_file(&self, tenant_id: &str, name: &str) -> PathBuf {
        self.tenant_workspace_dir(tenant_id, name).join(".apollo.pid")
    }

    /// Reserve a free ephemeral port by binding to :0 and reading back the OS-assigned address.
    /// The listener is closed before the agent is spawned (tiny window, but no collision risk).
    fn reserve_free_port(&self) -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .ok()
            .and_then(|l| l.local_addr().ok())
            .map(|a| a.port())
            .unwrap_or(0)
    }

    fn rotate_logs(&self, log_path: &PathBuf) -> Result<()> {
        if let Ok(meta) = fs::metadata(log_path) {
            if meta.len() > 10 * 1024 * 1024 {
                // Keep 5 generations: .log → .log.1 → … → .log.5
                for n in (1u8..5).rev() {
                    let from = log_path.with_extension(format!("log.{n}"));
                    let to   = log_path.with_extension(format!("log.{}", n + 1));
                    let _ = fs::rename(&from, &to);
                }
                let _ = fs::rename(log_path, log_path.with_extension("log.1"));
            }
        }
        Ok(())
    }

    fn harden_path(&self, root: &PathBuf, sub: &PathBuf) -> Result<PathBuf> {
        let joined = root.join(sub);
        let canonical = fs::canonicalize(&joined).unwrap_or(joined);
        if !canonical.starts_with(root) {
            return Err(anyhow!("Security violation: path escape detected"));
        }
        Ok(canonical)
    }
}

// ── Platform-specific process group control ───────────────────────────────────

#[cfg(unix)]
fn spawn_in_group(cmd: &mut Command) {
    unsafe {
        cmd.pre_exec(|| {
            let _ = nix::unistd::setpgid(NixPid::from_raw(0), NixPid::from_raw(0));
            Ok(())
        });
    }
}

#[cfg(windows)]
fn spawn_in_group(cmd: &mut Command) {
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(any(unix, windows)))]
fn spawn_in_group(_cmd: &mut Command) {}

/// Kill a process and all its children (process group / tree).
pub fn kill_group(pid: u32, force: bool) {
    #[cfg(unix)]
    {
        let sig = if force { Signal::SIGKILL } else { Signal::SIGTERM };
        let _ = signal::kill(NixPid::from_raw(-(pid as i32)), sig);
    }
    #[cfg(windows)]
    {
        let flag = if force { "/F" } else { "" };
        let mut args = vec!["/T", "/PID", &pid.to_string()];
        if force { args.insert(0, "/F"); }
        let _ = std::process::Command::new("taskkill").args(&args).output();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = std::process::Command::new("kill")
            .arg(if force { "-9" } else { "-15" })
            .arg(pid.to_string())
            .output();
    }
}

/// Build sanitized PATH string appropriate for the current platform.
fn build_path(runtimes_dir: &Path, runtime_kind: &str) -> String {
    let runtime_bin_dir = runtimes_dir.join(runtime_kind);

    #[cfg(unix)]
    {
        let mut parts: Vec<String> = vec![
            "/usr/local/bin".into(),
            "/usr/bin".into(),
            "/bin".into(),
            "/usr/sbin".into(),
            "/sbin".into(),
            "/opt/homebrew/bin".into(),  // Apple Silicon Homebrew
            "/opt/homebrew/sbin".into(),
            "/usr/local/go/bin".into(),  // Go
        ];
        // Locally installed runtimes take precedence
        if runtime_bin_dir.exists() {
            parts.insert(0, runtime_bin_dir.to_string_lossy().to_string());
        }
        // Python framework paths on macOS
        #[cfg(target_os = "macos")]
        for ver in &["3.13", "3.12", "3.11", "3.10", "3.9"] {
            let p = format!(
                "/Library/Frameworks/Python.framework/Versions/{}/bin",
                ver
            );
            if std::path::Path::new(&p).exists() {
                parts.push(p);
            }
        }
        parts.join(":")
    }

    #[cfg(windows)]
    {
        let sys = std::env::var("PATH").unwrap_or_default();
        if runtime_bin_dir.exists() {
            format!("{};{}", runtime_bin_dir.display(), sys)
        } else {
            sys
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string())
    }
}

/// Sweep an orphaned agent by PID file and kill it if still running.
fn sweep_orphan(pid_file: &Path) {
    if let Ok(content) = fs::read_to_string(pid_file) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            let mut sys = System::new_all();
            sys.refresh_processes();
            if sys.process(Pid::from(pid as usize)).is_some() {
                kill_group(pid, true);
            }
        }
        let _ = fs::remove_file(pid_file);
    }
}

// ── Per-tenant sharded instance storage ──────────────────────────────────────

pub fn load_tenant_instances(base_dir: &Path, tenant_id: &str) -> Result<Vec<AgentInstance>> {
    let path = instances_path(base_dir, tenant_id);
    if !path.exists() {
        return Ok(vec![]);
    }
    let raw = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn save_tenant_instances(base_dir: &Path, tenant_id: &str, instances: &[AgentInstance]) -> Result<()> {
    let path = instances_path(base_dir, tenant_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(instances)?)?;
    Ok(())
}

pub fn save_instance(base_dir: &Path, instance: &AgentInstance) -> Result<()> {
    let mut instances = load_tenant_instances(base_dir, &instance.tenant_id)?;
    if let Some(pos) = instances.iter().position(|i| i.id == instance.id) {
        instances[pos] = instance.clone();
    } else {
        instances.push(instance.clone());
    }
    save_tenant_instances(base_dir, &instance.tenant_id, &instances)
}

/// Count all running instances across all tenants.
pub fn count_active_instances(base_dir: &Path) -> usize {
    let instances_dir = base_dir.join("instances");
    if !instances_dir.exists() {
        return 0;
    }
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(&instances_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('_') { continue; } // skip index
            if let Ok(raw) = fs::read_to_string(entry.path()) {
                if let Ok(instances) = serde_json::from_str::<Vec<AgentInstance>>(&raw) {
                    count += instances.iter().filter(|i| i.status == "running").count();
                }
            }
        }
    }
    count
}

/// Load all instances across all tenants (for startup recovery).
pub fn load_all_instances(base_dir: &Path) -> Vec<AgentInstance> {
    let instances_dir = base_dir.join("instances");
    let mut all = Vec::new();
    if let Ok(entries) = fs::read_dir(&instances_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('_') { continue; }
            if let Ok(raw) = fs::read_to_string(entry.path()) {
                if let Ok(instances) = serde_json::from_str::<Vec<AgentInstance>>(&raw) {
                    all.extend(instances);
                }
            }
        }
    }
    all
}

// ── AgentRuntime impl ────────────────────────────────────────────────────────

#[async_trait]
impl AgentRuntime for ProcessRuntime {
    async fn install(&self, spec: &AgentSpec) -> Result<()> {
        fs::create_dir_all(self.agent_code_dir(&spec.name))?;
        let runtimes_dir = self.runtimes_dir();
        fs::create_dir_all(&runtimes_dir)?;
        ensure_runtime(&spec.runtime, &runtimes_dir).await?;
        Ok(())
    }

    async fn activate(&self, tenant_id: &str, spec: &AgentSpec) -> Result<()> {
        fs::create_dir_all(self.tenant_workspace_dir(tenant_id, &spec.name))?;
        fs::create_dir_all(self.base_dir.join("logs").join(tenant_id))?;
        Ok(())
    }

    async fn start(&self, tenant_id: &str, spec: &AgentSpec) -> Result<AgentInstance> {
        let workspace   = self.tenant_workspace_dir(tenant_id, &spec.name);
        let code_dir    = self.agent_code_dir(&spec.name);
        let port        = self.reserve_free_port();
        let log_path    = self.tenant_log_file(tenant_id, &spec.name);
        let runtimes_dir = self.runtimes_dir();
        let pid_file    = self.pid_file(tenant_id, &spec.name);

        fs::create_dir_all(&workspace)?;
        if let Some(p) = log_path.parent() { fs::create_dir_all(p)?; }
        self.rotate_logs(&log_path)?;

        let entry_path = self.harden_path(&code_dir, &PathBuf::from(&spec.runtime.entry))?;

        // Resolve binary + args for any runtime type
        let (binary, args) = resolve_launch(&spec.runtime, &entry_path, &runtimes_dir)
            .map_err(|e| anyhow!("Runtime '{}' unavailable: {}", spec.runtime.kind, e))?;

        let mut cmd = Command::new(&binary);
        cmd.args(&args);

        // Isolate process group (platform-specific)
        spawn_in_group(&mut cmd);

        cmd.current_dir(&workspace);

        let log_file = OpenOptions::new().create(true).append(true).open(&log_path)?;
        cmd.stdout(Stdio::from(log_file.try_clone()?));
        cmd.stderr(Stdio::from(log_file));

        // Kill any leftover orphan from a previous crash
        sweep_orphan(&pid_file);

        // Sanitized environment
        cmd.env_clear();
        cmd.env("PATH", build_path(&runtimes_dir, &spec.runtime.kind));
        cmd.env("PYTHONUNBUFFERED", "1");
        cmd.env("NODE_NO_WARNINGS", "1");
        cmd.env("APOLLO_TENANT_ID", tenant_id);
        cmd.env("APOLLO_AGENT_NAME", &spec.name);
        cmd.env("APOLLO_PORT", port.to_string());
        cmd.env("APOLLO_WORKSPACE", workspace.to_string_lossy().to_string());
        cmd.env("NO_PROXY", "localhost,127.0.0.1,10.0.0.0/8,192.168.0.0/16");
        cmd.env("APOLLO_NETWORK_ALLOW_INTERNAL", "false");

        // Windows needs a few system vars to function
        #[cfg(windows)]
        for var in &["SYSTEMROOT", "SYSTEMDRIVE", "WINDIR", "TEMP", "TMP", "COMSPEC"] {
            if let Ok(val) = std::env::var(var) {
                cmd.env(var, val);
            }
        }

        // Agent-level env overrides from agent.yaml
        if let Some(env) = &spec.runtime.env {
            for (k, v) in env {
                if !k.starts_with("APOLLO_") && k != "PATH" {
                    cmd.env(k, v);
                }
            }
        }

        // Per-tenant secrets (stored in base_dir/secrets/{tenant}.json)
        let tenant_secrets = load_secrets(&self.base_dir, tenant_id);
        for (k, v) in &tenant_secrets.secrets {
            if !k.starts_with("APOLLO_") && k != "PATH" {
                cmd.env(k, v);
            }
        }

        // Persistent volume mounts — create dirs and expose as APOLLO_VOLUME_{NAME}
        for vol in &spec.volumes {
            let vol_path = self.base_dir
                .join("volumes")
                .join(tenant_id)
                .join(&spec.name)
                .join(&vol.name);
            let _ = fs::create_dir_all(&vol_path);
            let env_key = format!(
                "APOLLO_VOLUME_{}",
                vol.name.to_uppercase().replace('-', "_")
            );
            cmd.env(&env_key, vol_path.to_string_lossy().to_string());
        }

        let child  = cmd.spawn()?;
        let pid    = child.id();
        let now    = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs();

        // Write PID file for cross-platform orphan tracking
        if let Some(p) = pid {
            let _ = fs::write(&pid_file, p.to_string());
        }

        let pid_val       = pid.unwrap_or(0);
        let mem_limit_mb  = parse_memory_limit(&spec.resources.memory);
        let cpu_limit_pct = spec.resources.cpu * 100.0;

        // Resource enforcement monitor — runs until the process exits or is killed
        let agent_name_mon = spec.name.clone();
        let tenant_mon     = tenant_id.to_string();
        tokio::spawn(async move {
            let mut sys = System::new_all();
            let mut cpu_strikes = 0u32;
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                sys.refresh_processes();
                match sys.process(Pid::from(pid_val as usize)) {
                    None => break,
                    Some(proc) => {
                        let mem_mb = proc.memory() / 1024 / 1024;
                        if mem_mb > mem_limit_mb {
                            tracing::warn!(
                                agent = %agent_name_mon, tenant = %tenant_mon,
                                pid = pid_val, mem_mb, limit_mb = mem_limit_mb,
                                "OOM: killing agent — memory limit exceeded"
                            );
                            kill_group(pid_val, true);
                            break;
                        }
                        if proc.cpu_usage() > cpu_limit_pct {
                            cpu_strikes += 1;
                            if cpu_strikes >= 3 {
                                tracing::warn!(
                                    agent = %agent_name_mon, tenant = %tenant_mon,
                                    pid = pid_val, cpu = proc.cpu_usage(), limit = cpu_limit_pct,
                                    "CPU: killing agent — sustained CPU limit exceeded"
                                );
                                kill_group(pid_val, true);
                                break;
                            }
                        } else {
                            cpu_strikes = 0;
                        }
                    }
                }
            }
        });

        Ok(AgentInstance {
            id:         format!("{}-{}", spec.name, now % 100_000),
            agent_id:   spec.name.clone(),
            tenant_id:  tenant_id.to_string(),
            status:     "running".to_string(),
            pid,
            port:       Some(port),
            stats:      ExecutionStats { last_restart: now, ..Default::default() },
            created_at: now,
        })
    }

    async fn stop(&self, pid: u32) -> Result<()> {
        kill_group(pid, false);
        tokio::time::sleep(Duration::from_secs(2)).await;
        kill_group(pid, true);
        Ok(())
    }

    async fn status(&self, _id: &str) -> Result<String> { Ok("running".to_string()) }
    async fn health_check(&self, _id: &str) -> Result<bool> { Ok(true) }
    async fn shutdown(&self) -> Result<()> { Ok(()) }
}

fn parse_memory_limit(limit: &str) -> u64 {
    let s = limit.to_lowercase();
    if s.ends_with("gb") {
        s.trim_end_matches("gb").trim().parse::<u64>().unwrap_or(1) * 1024
    } else if s.ends_with("mb") {
        s.trim_end_matches("mb").trim().parse::<u64>().unwrap_or(512)
    } else {
        512
    }
}
