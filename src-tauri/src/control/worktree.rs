//! M4: git worktrees so two sessions in one repo never share a working tree.
//!
//! This writes to the user's own git repos, but ONLY on an explicit spawn —
//! never on scan/tick. Adapters stay read-only; this is control-side, like
//! tmux spawning. Nothing here touches a harness config dir.

use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn git(cwd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The *shared* repo root for `cwd` — the main working tree's toplevel, which
/// is identical for the main tree and every linked worktree of the same repo.
/// `None` when `cwd` isn't inside a git repo (or git is missing).
///
/// Uses `--git-common-dir` (the shared `.git`, not a worktree's private gitdir),
/// resolved to an absolute path so main-tree and worktree cwds map to the same
/// identity — that's what makes "is this repo already busy?" correct.
pub fn repo_root(cwd: &str) -> Option<String> {
    let common = git(cwd, &["rev-parse", "--git-common-dir"]).ok()?;
    let common_path = Path::new(&common);
    let abs = if common_path.is_absolute() {
        common_path.to_path_buf()
    } else {
        Path::new(cwd).join(common_path)
    };
    let abs = abs.canonicalize().ok()?;
    if abs.file_name().and_then(|n| n.to_str()) == Some(".git") {
        abs.parent().map(|p| p.to_string_lossy().into_owned())
    } else {
        // Bare repo: no working tree to share — treat the gitdir as the root.
        Some(abs.to_string_lossy().into_owned())
    }
}

/// Header label for a repo root (the last path component).
pub fn repo_label(repo_root: &str) -> String {
    Path::new(repo_root)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("repo")
        .to_string()
}

pub struct Created {
    pub path: String,
    pub branch: String,
}

/// Add a fresh worktree off `repo_root` on a new `hv-<id>` branch, placed in a
/// sibling dir `<repo>.hv-<id>` so it's discoverable next to the repo.
pub fn add(repo_root: &str) -> Result<Created, String> {
    let branch = format!("hv-{}", short_id());
    let root = Path::new(repo_root);
    let label = repo_label(repo_root);
    let parent = root.parent().unwrap_or_else(|| Path::new("."));
    let path = parent
        .join(format!("{label}.{branch}"))
        .to_string_lossy()
        .into_owned();
    git(repo_root, &["worktree", "add", &path, "-b", &branch])?;
    Ok(Created { path, branch })
}

fn short_id() -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:08x}", (n as u64) & 0xffff_ffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git");
    }

    fn init_repo(dir: &Path) {
        run(dir, &["init", "-q"]);
        run(dir, &["config", "user.email", "t@t"]);
        run(dir, &["config", "user.name", "t"]);
        std::fs::write(dir.join("f.txt"), "hi").unwrap();
        run(dir, &["add", "."]);
        run(dir, &["commit", "-qm", "init"]);
    }

    #[test]
    fn shared_root_holds_across_main_tree_and_worktree() {
        let base = std::env::temp_dir().join(format!("hv-wt-{}", short_id_nanos()));
        let repo = base.join("myrepo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let repo_s = repo.to_string_lossy().into_owned();

        let root = repo_root(&repo_s).expect("repo root");
        assert_eq!(
            Path::new(&root).canonicalize().unwrap(),
            repo.canonicalize().unwrap()
        );
        assert_eq!(repo_label(&root), "myrepo");

        let created = add(&root).expect("worktree add");
        assert!(Path::new(&created.path).exists(), "worktree dir created");
        assert!(created.branch.starts_with("hv-"));

        // The whole point: a session inside the worktree resolves to the SAME
        // shared root, so busy-detection treats them as one repo.
        let wt_root = repo_root(&created.path).expect("worktree root");
        assert_eq!(
            Path::new(&wt_root).canonicalize().unwrap(),
            repo.canonicalize().unwrap()
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn non_repo_has_no_root() {
        let dir = std::env::temp_dir().join(format!("hv-wt-plain-{}", short_id_nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(repo_root(&dir.to_string_lossy()).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn short_id_nanos() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }
}
