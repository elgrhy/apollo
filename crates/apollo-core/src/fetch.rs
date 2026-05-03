//! Remote agent source resolution — URLs, git repos, archives, local paths.

use anyhow::{anyhow, Context, Result};
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
async fn fetch_archive(url: &str, staging_dir: &Path) -> Result<PathBuf> {
    println!("[FETCH] Downloading agent from {}", url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to connect to {}", url))?;

    if !response.status().is_success() {
        return Err(anyhow!("HTTP {} downloading {}", response.status(), url));
    }

    let filename = url
        .split('/')
        .last()
        .and_then(|s| s.split('?').next())
        .unwrap_or("agent.download");

    let download_path = staging_dir.join(filename);
    let bytes = response.bytes().await.context("Failed to read response body")?;
    fs::write(&download_path, &bytes)?;

    println!("[FETCH] Downloaded {} bytes to {:?}", bytes.len(), download_path);

    let extract_dir = staging_dir.join("unpacked");
    fs::create_dir_all(&extract_dir)?;

    let lower = filename.to_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".tar.bz2") {
        extract_tar(&download_path, &extract_dir)?;
    } else if lower.ends_with(".zip") {
        extract_zip(&download_path, &extract_dir)?;
    } else {
        // Might be a bare directory listing or a single script — treat the staging dir itself.
        fs::copy(&download_path, extract_dir.join(filename))?;
    }

    find_agent_yaml_dir(&extract_dir)
}

/// Clone a git repository and return the directory containing `agent.yaml`.
async fn git_clone(url: &str, staging_dir: &Path) -> Result<PathBuf> {
    println!("[FETCH] Cloning {}", url);

    let clone_dir = staging_dir.join("repo");
    let output = tokio::process::Command::new("git")
        .args(["clone", "--depth", "1", url, clone_dir.to_str().unwrap_or("repo")])
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
