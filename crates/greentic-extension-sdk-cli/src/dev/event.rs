//! `DevEvent` enum + human/JSON formatters.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// One lifecycle event emitted by the dev loop.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DevEvent {
    Ready {
        ext_id: String,
        ext_version: String,
        kind: String,
        registry: String,
        watched_files: usize,
    },
    ChangeDetected {
        path: String,
    },
    Debouncing {
        window_ms: u64,
    },
    BuildStart {
        profile: String,
    },
    BuildOk {
        duration_ms: u64,
        wasm_size: u64,
    },
    BuildFailed {
        duration_ms: u64,
    },
    PackOk {
        pack_name: String,
        size: u64,
    },
    InstallOk {
        registry: String,
        version: String,
    },
    InstallSkipped {
        reason: String,
    },
    Idle {
        last_build_ok: bool,
    },
    Shutdown,
    Error {
        message: String,
    },
}

/// Emission sink: stdout printer used by `run_once` / `run_watch`.
pub trait Emitter: Send {
    fn emit(&mut self, event: &DevEvent);
}

/// Selects between human-readable and JSONL output.
pub enum Format {
    Human,
    Json,
}

impl Format {
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        match s {
            "human" => Ok(Format::Human),
            "json" => Ok(Format::Json),
            other => anyhow::bail!("unknown --format: {other} (use human|json)"),
        }
    }
}

pub struct StdoutEmitter {
    pub format: Format,
}

impl Emitter for StdoutEmitter {
    fn emit(&mut self, event: &DevEvent) {
        match self.format {
            Format::Human => println!("{}", format_human(event)),
            Format::Json => println!("{}", format_json(event)),
        }
    }
}

/// Render a single event as the line humans see in `gtdx dev`.
pub fn format_human(event: &DevEvent) -> String {
    let ts = timestamp_human();
    match event {
        DevEvent::Ready {
            ext_id,
            ext_version,
            kind,
            registry,
            watched_files,
        } => format!(
            "[{ts}] ready. id={ext_id}@{ext_version} kind={kind} registry={registry} watching={watched_files}",
        ),
        DevEvent::ChangeDetected { path } => format!("[{ts}] change detected: {path}"),
        DevEvent::Debouncing { window_ms } => format!("[{ts}] debouncing ({window_ms}ms)..."),
        DevEvent::BuildStart { profile } => format!("[{ts}] building ({profile}, incremental)..."),
        DevEvent::BuildOk {
            duration_ms,
            wasm_size,
        } => format!(
            "[{ts}] \u{2713} build ok ({}s, {} KB)",
            fmt_secs(*duration_ms),
            wasm_size / 1024,
        ),
        DevEvent::BuildFailed { duration_ms } => format!(
            "[{ts}] \u{2717} build failed ({}s). Fix errors above and save to retry.",
            fmt_secs(*duration_ms),
        ),
        DevEvent::PackOk { pack_name, size } => {
            format!("[{ts}] \u{2713} packed {pack_name} ({} KB)", size / 1024)
        }
        DevEvent::InstallOk { registry, version } => {
            format!("[{ts}] \u{2713} installed {version} into {registry}. ready.")
        }
        DevEvent::InstallSkipped { reason } => format!("[{ts}] skipped install: {reason}"),
        DevEvent::Idle { last_build_ok } => {
            let tag = if *last_build_ok {
                ""
            } else {
                " (last build failed)"
            };
            format!("[{ts}] idle{tag}.")
        }
        DevEvent::Shutdown => format!("[{ts}] shutting down."),
        DevEvent::Error { message } => format!("[{ts}] error: {message}"),
    }
}

/// Render an event as a single JSON line with a UTC timestamp.
pub fn format_json(event: &DevEvent) -> String {
    #[derive(Serialize)]
    struct Envelope<'a> {
        ts: String,
        #[serde(flatten)]
        event: &'a DevEvent,
    }
    let env = Envelope {
        ts: timestamp_utc_iso8601(),
        event,
    };
    serde_json::to_string(&env)
        .unwrap_or_else(|e| format!("{{\"event\":\"error\",\"message\":\"{e}\"}}"))
}

fn timestamp_human() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    let secs = now.as_secs() % 86_400;
    let h = secs / 3_600;
    let m = (secs % 3_600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn timestamp_utc_iso8601() -> String {
    use time::OffsetDateTime;
    use time::format_description::well_known::Iso8601;
    OffsetDateTime::now_utc()
        .format(&Iso8601::DEFAULT)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

// Millisecond precision loss is acceptable for a human-readable "X.Ys" display.
#[allow(clippy::cast_precision_loss)]
fn fmt_secs(ms: u64) -> String {
    format!("{:.1}", ms as f64 / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_build_ok_includes_check_mark_and_size() {
        let e = DevEvent::BuildOk {
            duration_ms: 2100,
            wasm_size: 48_512,
        };
        let line = format_human(&e);
        assert!(line.contains("\u{2713}"));
        assert!(line.contains("build ok"));
        assert!(line.contains("2.1s"));
        assert!(line.contains("47 KB"));
    }

    #[test]
    fn human_idle_shows_last_build_status() {
        let ok = format_human(&DevEvent::Idle {
            last_build_ok: true,
        });
        let fail = format_human(&DevEvent::Idle {
            last_build_ok: false,
        });
        assert!(ok.contains("idle."));
        assert!(fail.contains("(last build failed)"));
    }

    #[test]
    fn json_shape_has_ts_and_event_tag() {
        let e = DevEvent::BuildOk {
            duration_ms: 2100,
            wasm_size: 48_512,
        };
        let line = format_json(&e);
        let v: serde_json::Value = serde_json::from_str(&line).expect("valid json");
        assert_eq!(v["event"], "build_ok");
        assert_eq!(v["duration_ms"], 2100);
        assert_eq!(v["wasm_size"], 48_512);
        assert!(
            v["ts"].as_str().unwrap().ends_with('Z')
                || v["ts"].as_str().unwrap().contains("+00:00")
        );
    }

    #[test]
    fn json_change_detected_preserves_path() {
        let e = DevEvent::ChangeDetected {
            path: "src/lib.rs".into(),
        };
        let line = format_json(&e);
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["event"], "change_detected");
        assert_eq!(v["path"], "src/lib.rs");
    }

    #[test]
    fn format_parse_roundtrip() {
        assert!(matches!(Format::parse("human").unwrap(), Format::Human));
        assert!(matches!(Format::parse("json").unwrap(), Format::Json));
        assert!(Format::parse("yaml").is_err());
    }
}
