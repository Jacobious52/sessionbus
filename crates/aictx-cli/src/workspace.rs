use anyhow::{Context, Result};
use serde_json::json;
use sessionbus_core::{sha256_hex, ArtifactKind, CreateArtifactRequest, WorkspaceInfo};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) fn workspace_watch_artifact(workspace: &Path) -> Result<CreateArtifactRequest> {
    let root = git_output_in(["rev-parse", "--show-toplevel"], workspace)?
        .unwrap_or_else(|| workspace.display().to_string());
    let branch = git_output_in(["branch", "--show-current"], workspace)?;
    let head = git_output_in(["rev-parse", "--short", "HEAD"], workspace)?;
    let status = git_output_in(["status", "--short"], workspace)?.unwrap_or_default();
    let body = format!(
        "workspace watch\nroot\t{}\nbranch\t{}\nhead\t{}\nstatus\n{}",
        root,
        branch.as_deref().unwrap_or(""),
        head.as_deref().unwrap_or(""),
        status
    );
    Ok(CreateArtifactRequest {
        kind: ArtifactKind::ToolInvocation,
        title: Some("workspace watch".to_string()),
        uri: Some(format!("file://{}", root)),
        body: Some(body),
        metadata: json!({
            "adapter": "workspace-watch",
            "workspace": root,
            "branch": branch,
            "head": head,
            "status": status
        }),
        snapshot: true,
    })
}

pub(crate) fn print_workspace() -> Result<()> {
    print!("{}", workspace_text()?);
    Ok(())
}

pub(crate) fn workspace_text() -> Result<String> {
    let workspace = detect_workspace()?;
    let mut out = String::new();
    out.push_str(&format!("root\t{}\n", workspace.root));
    if let Some(branch) = workspace.git_branch {
        out.push_str(&format!("branch\t{}\n", branch));
    }
    if let Some(head) = workspace.head {
        out.push_str(&format!("head\t{}\n", head));
    }
    if let Some(remote) = workspace.git_remote {
        out.push_str(&format!("remote\t{}\n", remote));
    }
    if let Some(status) = git_output(["status", "--short"])? {
        out.push_str("dirty\n");
        out.push_str(&status);
        if !status.ends_with('\n') {
            out.push('\n');
        }
    } else {
        out.push_str("clean\n");
    }
    Ok(out)
}

pub(crate) fn detect_workspace() -> Result<WorkspaceInfo> {
    let root = git_output(["rev-parse", "--show-toplevel"])?
        .unwrap_or(std::env::current_dir()?.display().to_string());
    Ok(WorkspaceInfo {
        root,
        git_remote: git_output(["config", "--get", "remote.origin.url"])?,
        git_branch: git_output(["branch", "--show-current"])?,
        head: git_output(["rev-parse", "--short", "HEAD"])?,
    })
}

pub(crate) fn git_output<const N: usize>(args: [&str; N]) -> Result<Option<String>> {
    git_output_in(args, &std::env::current_dir()?)
}

pub(crate) fn git_output_in<const N: usize>(args: [&str; N], cwd: &Path) -> Result<Option<String>> {
    let output = Command::new("git").args(args).current_dir(cwd).output();
    let Ok(output) = output else {
        return Ok(None);
    };
    if !output.status.success() {
        return Ok(None);
    }
    let value = String::from_utf8(output.stdout)?.trim().to_string();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

pub(crate) fn active_session_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("sessionbus")
            .join("current-session")
    } else {
        PathBuf::from(".sessionbus").join("current-session")
    }
}

pub(crate) fn write_active_session(session_id: &str) -> Result<()> {
    let path = active_session_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, session_id)?;
    Ok(())
}

pub(crate) fn workspace_session_path(root: &str) -> PathBuf {
    let workspace_key = sha256_hex(root);
    active_session_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(".sessionbus"))
        .join("workspaces")
        .join(workspace_key)
        .join("current-session")
}

pub(crate) fn write_workspace_session(root: &str, session_id: &str) -> Result<()> {
    let path = workspace_session_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, session_id)?;
    Ok(())
}

pub(crate) fn clear_active_session_if_matches(session_id: &str) -> Result<()> {
    let path = active_session_path();
    let Ok(current) = fs::read_to_string(&path) else {
        return Ok(());
    };
    if current.trim() == session_id {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub(crate) fn clear_workspace_session_if_matches(root: &str, session_id: &str) -> Result<()> {
    let path = workspace_session_path(root);
    let Ok(current) = fs::read_to_string(&path) else {
        return Ok(());
    };
    if current.trim() == session_id {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub(crate) fn resolve_session(session: Option<String>) -> Result<String> {
    if let Some(session) = session {
        return Ok(session);
    }
    if let Ok(workspace) = detect_workspace() {
        let path = workspace_session_path(&workspace.root);
        if let Ok(value) = fs::read_to_string(&path) {
            let session_id = value.trim();
            if !session_id.is_empty() {
                return Ok(session_id.to_string());
            }
        }
    }
    let path = active_session_path();
    fs::read_to_string(&path)
        .map(|value| value.trim().to_string())
        .with_context(|| {
            format!(
                "no active session found at {}; run `aictx start \"...\"` or pass --session",
                path.display()
            )
        })
}

pub(crate) fn absolutize(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}
