//! hvscan — CLI over the session adapters (M1 oracle target) + grammar (M7g).
//!
//! Flags: --json, --max-age <hours>, --limit <n>, --watch
//! Subcommand: cmd "<text>" — shared grammar harness

use hypervisor_lib::{run_grammar_cmd, scan_sessions, watch_sessions_cli};

fn main() {
    let mut args = std::env::args().skip(1).peekable();

    // M7g: `hvscan cmd "status"`
    if args.peek().map(|s| s.as_str()) == Some("cmd") {
        args.next();
        let text = args.next().unwrap_or_default();
        if text.is_empty() {
            eprintln!("usage: hvscan cmd \"<text>\"");
            std::process::exit(2);
        }
        std::process::exit(run_grammar_cmd(&text));
    }

    let mut json = false;
    let mut watch = false;
    let mut max_age: f64 = 48.0;
    let mut limit: usize = 8;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--watch" => watch = true,
            "--max-age" => {
                max_age = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(48.0);
            }
            "--limit" => {
                limit = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(8);
            }
            "-h" | "--help" => {
                eprintln!(
                    "usage: hvscan [--json] [--watch] [--max-age HOURS] [--limit N]\n       hvscan cmd \"<text>\""
                );
                return;
            }
            other => {
                eprintln!("unknown flag: {other}");
                std::process::exit(2);
            }
        }
    }

    if watch {
        if let Err(e) = watch_sessions_cli(max_age, limit) {
            eprintln!("watch failed: {e}");
            std::process::exit(1);
        }
        return;
    }

    let sessions = scan_sessions(max_age, limit, None);
    if json {
        for s in sessions {
            match serde_json::to_string(&s) {
                Ok(line) => println!("{line}"),
                Err(e) => eprintln!("serialize: {e}"),
            }
        }
    } else {
        for s in sessions {
            println!(
                "[{}] {} · {} · {} · {} · {}",
                s.state, s.harness, s.sid, s.repo, s.age, s.title
            );
        }
    }
}
