//! Bake POSTHOG_* into the binary via `option_env!`.
//!
//! Load order: process env (CI / shell exports) wins; otherwise repo-root
//! `.env` (gitignored) is read so `npm run tauri dev` picks up staging keys
//! without a manual `export`.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo_env = manifest_dir.join("../.env");
    println!("cargo:rerun-if-changed={}", repo_env.display());
    println!("cargo:rerun-if-env-changed=POSTHOG_PROJECT_KEY");
    println!("cargo:rerun-if-env-changed=POSTHOG_HOST");

    // Process env first so CI secrets / shell exports override .env.
    let mut key = env::var("POSTHOG_PROJECT_KEY").ok().filter(|s| !s.is_empty());
    let mut host = env::var("POSTHOG_HOST").ok().filter(|s| !s.is_empty());

    if key.is_none() || host.is_none() {
        if let Some(map) = read_dotenv(&repo_env) {
            if key.is_none() {
                key = map.get("POSTHOG_PROJECT_KEY").cloned();
            }
            if host.is_none() {
                host = map.get("POSTHOG_HOST").cloned();
            }
        }
    }

    if let Some(k) = key {
        println!("cargo:rustc-env=POSTHOG_PROJECT_KEY={k}");
    }
    if let Some(h) = host {
        println!("cargo:rustc-env=POSTHOG_HOST={h}");
    }

    tauri_build::build()
}

fn read_dotenv(path: &PathBuf) -> Option<std::collections::HashMap<String, String>> {
    let data = fs::read_to_string(path).ok()?;
    let mut map = std::collections::HashMap::new();
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let v = strip_quotes(v.trim());
        if !k.is_empty() && !v.is_empty() {
            map.insert(k.to_string(), v);
        }
    }
    Some(map)
}

fn strip_quotes(s: &str) -> String {
    let b = s.as_bytes();
    if b.len() >= 2
        && ((b[0] == b'"' && b[b.len() - 1] == b'"')
            || (b[0] == b'\'' && b[b.len() - 1] == b'\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}
