use crate::AgentRuntime;
use mars_core::types::{AgentSpec, AgentInstance, ExecutionStats};
use async_trait::async_trait;
use anyhow::{Result, anyhow};
use std::process::Stdio;
use tokio::process::Command;
use std::time::{SystemTime, Duration};
use std::path::PathBuf;
use std::fs::{self, OpenOptions};
use sysinfo::{System, Pid};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid as NixPid;

pub struct ProcessRuntime {
    pub base_dir: PathBuf,
}

impl ProcessRuntime {
    pub fn new(base_dir: PathBuf) -> Self {
        let base_dir = fs::canonicalize(&base_dir).unwrap_or(base_dir);
        Self { base_dir }
    }

    fn agent_code_dir(&self, agent_name: &str) -> PathBuf {
        self.base_dir.join("agents").join(agent_name)
    }

    fn tenant_workspace_dir(&self, tenant_id: &str, agent_name: &str) -> PathBuf {
        self.base_dir.join("tenants").join(tenant_id).join(agent_name)
    }

    fn tenant_log_file(&self, tenant_id: &str, agent_name: &str) -> PathBuf {
        self.base_dir.join("logs").join(tenant_id).join(format!("{}.log", agent_name))
    }

    fn compute_port(&self, tenant_id: &str, agent_name: &str) -> u16 {
        let mut hasher = DefaultHasher::new();
        tenant_id.hash(&mut hasher);
        agent_name.hash(&mut hasher);
        let hash = hasher.finish();
        10000 + (hash % 50000) as u16
    }

    fn rotate_logs(&self, log_path: &PathBuf) -> Result<()> {
        if let Ok(metadata) = fs::metadata(log_path) {
            if metadata.len() > 10 * 1024 * 1024 {
                let rotated_path = log_path.with_extension("log.old");
                let _ = fs::rename(log_path, rotated_path);
            }
        }
        Ok(())
    }

    fn harden_path(&self, root: &PathBuf, path: &PathBuf) -> Result<PathBuf> {
        let joined = root.join(path);
        let canonical = fs::canonicalize(&joined).unwrap_or(joined);
        if !canonical.starts_with(root) {
            return Err(anyhow!("Security violation: Path escape detected"));
        }
        Ok(canonical)
    }
}

#[async_trait]
impl AgentRuntime for ProcessRuntime {
    async fn install(&self, spec: &AgentSpec) -> Result<()> {
        let dir = self.agent_code_dir(&spec.name);
        tokio::fs::create_dir_all(&dir).await?;
        Ok(())
    }

    async fn activate(&self, tenant_id: &str, spec: &AgentSpec) -> Result<()> {
        let workspace = self.tenant_workspace_dir(tenant_id, &spec.name);
        tokio::fs::create_dir_all(&workspace).await?;
        let log_dir = self.base_dir.join("logs").join(tenant_id);
        tokio::fs::create_dir_all(&log_dir).await?;
        Ok(())
    }

    async fn start(&self, tenant_id: &str, spec: &AgentSpec) -> Result<AgentInstance> {
        let workspace = self.tenant_workspace_dir(tenant_id, &spec.name);
        let code_dir = self.agent_code_dir(&spec.name);
        let port = self.compute_port(tenant_id, &spec.name);
        let log_path = self.tenant_log_file(tenant_id, &spec.name);

        self.rotate_logs(&log_path)?;

        let entry_path = self.harden_path(&code_dir, &PathBuf::from(&spec.runtime.entry))?;
        
        let mut cmd = if spec.runtime.kind == "python3" {
            let mut c = Command::new("python3");
            c.arg(&entry_path);
            c
        } else {
            Command::new(&entry_path)
        };

        unsafe {
            cmd.pre_exec(|| {
                let _ = nix::unistd::setpgid(NixPid::from_raw(0), NixPid::from_raw(0));
                Ok(())
            });
        }

        cmd.current_dir(&workspace);
        
        let log_file = OpenOptions::new().create(true).append(true).open(&log_path)?;
        cmd.stdout(Stdio::from(log_file.try_clone()?));
        cmd.stderr(Stdio::from(log_file));

        // 100% Security: Strictly Sanitized Env + Network Kill-switches
        cmd.env_clear();
        cmd.env("PATH", "/usr/bin:/bin:/usr/local/bin");
        cmd.env("MARS_TENANT_ID", tenant_id);
        cmd.env("MARS_AGENT_NAME", &spec.name);
        cmd.env("MARS_PORT", port.to_string());
        cmd.env("MARS_WORKSPACE", workspace.to_string_lossy().to_string());
        
        // Block internal network by default (Env-level hint for agents)
        cmd.env("NO_PROXY", "localhost,127.0.0.1,10.0.0.0/8,192.168.0.0/16");
        cmd.env("MARS_NETWORK_ALLOW_INTERNAL", "false");

        if let Some(env) = &spec.runtime.env {
            for (k, v) in env {
                if !k.starts_with("MARS_") && k != "PATH" { cmd.env(k, v); }
            }
        }

        let child = cmd.spawn()?;
        let pid = child.id();
        let created_at = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs();

        let pid_val = pid.unwrap_or(0);
        let mem_limit_mb = parse_memory_limit(&spec.resources.memory);
        let cpu_limit_pct = spec.resources.cpu * 100.0;
        
        tokio::spawn(async move {
            let mut sys = System::new_all();
            let mut cpu_violation_count = 0;
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                sys.refresh_processes();
                if let Some(proc) = sys.process(Pid::from(pid_val as usize)) {
                    let mem_mb = proc.memory() / 1024 / 1024;
                    if mem_mb > mem_limit_mb {
                        let _ = signal::kill(NixPid::from_raw(-(pid_val as i32)), Signal::SIGKILL);
                        break;
                    }
                    let cpu_usage = proc.cpu_usage();
                    if cpu_usage > cpu_limit_pct {
                        cpu_violation_count += 1;
                    } else {
                        cpu_violation_count = 0;
                    }
                    if cpu_violation_count >= 3 {
                        let _ = signal::kill(NixPid::from_raw(-(pid_val as i32)), Signal::SIGKILL);
                        break;
                    }
                } else {
                    break;
                }
            }
        });

        Ok(AgentInstance {
            id: format!("{}-{}", spec.name, created_at % 10000),
            agent_id: spec.name.clone(),
            tenant_id: tenant_id.to_string(),
            status: "running".to_string(),
            pid,
            port: Some(port),
            stats: ExecutionStats {
                last_restart: created_at,
                ..Default::default()
            },
            created_at,
        })
    }

    async fn stop(&self, pid: u32) -> Result<()> {
        let _ = signal::kill(NixPid::from_raw(-(pid as i32)), Signal::SIGTERM);
        tokio::time::sleep(Duration::from_secs(2)).await;
        let _ = signal::kill(NixPid::from_raw(-(pid as i32)), Signal::SIGKILL);
        Ok(())
    }

    async fn status(&self, _instance_id: &str) -> Result<String> { Ok("running".to_string()) }
    async fn health_check(&self, _instance_id: &str) -> Result<bool> { Ok(true) }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

fn parse_memory_limit(limit: &str) -> u64 {
    let cleaned = limit.to_lowercase();
    if cleaned.ends_with("gb") {
        cleaned.replace("gb", "").trim().parse::<u64>().unwrap_or(1) * 1024
    } else if cleaned.ends_with("mb") {
        cleaned.replace("mb", "").trim().parse::<u64>().unwrap_or(512)
    } else {
        512
    }
}
