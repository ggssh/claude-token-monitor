use chrono::Utc;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStats {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_tokens: u64,
    pub estimated_cost_usd: f64,
    pub input_cost_usd: f64,
    pub output_cost_usd: f64,
    pub cache_discount_usd: f64,
    pub request_count: u64,
    pub cache_hit_pct: f64,
    pub max_input_per_request: u64,
    pub context_window: u64,
    pub context_usage_pct: f64,
    pub model: String,
}

impl Default for TokenStats {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            total_tokens: 0,
            estimated_cost_usd: 0.0,
            input_cost_usd: 0.0,
            output_cost_usd: 0.0,
            cache_discount_usd: 0.0,
            request_count: 0,
            cache_hit_pct: 0.0,
            max_input_per_request: 0,
            context_window: 0,
            context_usage_pct: 0.0,
            model: "unknown".into(),
        }
    }
}

fn is_valid_model(model: &str) -> bool {
    !model.is_empty() && !model.contains('<') && model != "unknown"
}

const CONTEXT_WINDOW: u64 = 200_000;

#[derive(Debug, Deserialize)]
struct SessionMessage {
    #[serde(rename = "type")]
    msg_type: String,
    message: Option<AssistantMessage>,
}

#[derive(Debug, Deserialize)]
struct AssistantMessage {
    role: String,
    model: Option<String>,
    usage: Option<UsageData>,
}

#[derive(Debug, Deserialize)]
struct UsageData {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
}

/// Lightweight struct for daily_activity parsing (avoids serde_json::Value DOM).
#[derive(Debug, Deserialize)]
struct DailyLine {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    timestamp: Option<String>,
    message: Option<DailyLineMessage>,
}

#[derive(Debug, Deserialize)]
struct DailyLineMessage {
    usage: Option<DailyUsage>,
}

#[derive(Debug, Deserialize)]
struct DailyUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
}

pub fn find_claude_projects_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let dir = PathBuf::from(home).join(".claude/projects");
    if dir.exists() {
        Some(dir)
    } else {
        None
    }
}

/// Iterate over all .jsonl session files across all project directories
fn for_each_session_file(mut f: impl FnMut(PathBuf)) {
    let projects_dir = match find_claude_projects_dir() {
        Some(d) => d,
        None => return,
    };
    let Ok(entries) = fs::read_dir(&projects_dir) else {
        return;
    };
    for project_dir in entries.flatten() {
        let project_path = project_dir.path();
        if !project_path.is_dir() || project_path.file_name().map_or(true, |n| n == "memory") {
            continue;
        }
        let Ok(sessions) = fs::read_dir(&project_path) else {
            continue;
        };
        for session in sessions.flatten() {
            let path = session.path();
            if path.extension().map_or(true, |e| e != "jsonl") {
                continue;
            }
            f(path);
        }
    }
}

pub fn find_active_session_file() -> Option<PathBuf> {
    let mut latest: Option<(PathBuf, SystemTime)> = None;

    for_each_session_file(|path| {
        if let Ok(meta) = path.metadata() {
            if let Ok(modified) = meta.modified() {
                if latest.as_ref().map_or(true, |(_, t)| modified > *t) {
                    latest = Some((path, modified));
                }
            }
        }
    });

    latest.map(|(p, _)| p)
}

pub fn parse_token_stats(file_path: &Path) -> TokenStats {
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return TokenStats::default(),
    };

    let mut stats = TokenStats::default();
    let mut model_name = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let msg: SessionMessage = match serde_json::from_str(line) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if msg.msg_type != "assistant" {
            continue;
        }

        if let Some(msg_data) = &msg.message {
            if msg_data.role != "assistant" {
                continue;
            }
            if let Some(m) = &msg_data.model {
                if is_valid_model(m) {
                    model_name = m.clone();
                }
            }
            if let Some(usage) = &msg_data.usage {
                stats.request_count += 1;
                let it = usage.input_tokens.unwrap_or(0);
                stats.input_tokens += it;
                stats.max_input_per_request = stats.max_input_per_request.max(it);
                stats.output_tokens += usage.output_tokens.unwrap_or(0);
                stats.cache_read_tokens += usage.cache_read_input_tokens.unwrap_or(0);
                stats.cache_write_tokens += usage.cache_creation_input_tokens.unwrap_or(0);
            }
        }
    }

    stats.total_tokens = stats.input_tokens
        + stats.output_tokens
        + stats.cache_read_tokens
        + stats.cache_write_tokens;
    stats.model = model_name;

    let total_input = stats.input_tokens + stats.cache_write_tokens + stats.cache_read_tokens;
    if total_input > 0 {
        stats.cache_hit_pct = (stats.cache_read_tokens as f64 / total_input as f64) * 100.0;
    }

    stats.context_window = CONTEXT_WINDOW;
    if stats.max_input_per_request > 0 {
        stats.context_usage_pct =
            (stats.max_input_per_request as f64 / CONTEXT_WINDOW as f64) * 100.0;
    }

    compute_costs(&mut stats);

    stats
}

fn model_prices(model: &str) -> (f64, f64) {
    match model {
        "deepseek-v4-pro" | "deepseek-v4" => (0.14, 1.10),
        "claude-sonnet-4-6" | "claude-sonnet-4-5" => (3.0, 15.0),
        "claude-opus-4-7" | "claude-opus-4-6" | "claude-opus-4-5" => (15.0, 75.0),
        "claude-haiku-4-5" => (0.80, 4.0),
        _ => (0.0, 0.0),
    }
}

fn compute_costs(stats: &mut TokenStats) {
    let (input_price, output_price) = model_prices(&stats.model);

    stats.input_cost_usd = (stats.input_tokens as f64 / 1_000_000.0) * input_price;
    stats.output_cost_usd = (stats.output_tokens as f64 / 1_000_000.0) * output_price;

    // cache read = 10% of input price; cache write = 1.25x input price
    let cache_read_cost = (stats.cache_read_tokens as f64 / 1_000_000.0) * input_price * 0.1;
    let cache_write_cost = (stats.cache_write_tokens as f64 / 1_000_000.0) * input_price * 1.25;

    // Money saved by using cache read instead of full input
    let potential_read_cost = (stats.cache_read_tokens as f64 / 1_000_000.0) * input_price;
    stats.cache_discount_usd = (potential_read_cost - cache_read_cost).max(0.0);

    stats.estimated_cost_usd = (stats.input_cost_usd
        + stats.output_cost_usd
        + cache_read_cost
        + cache_write_cost)
        .max(0.0);
}

pub type StatsUpdate = Arc<Mutex<TokenStats>>;

/// Generation counter — bumped by the file watcher when data changes.
/// Commands check this to avoid re-parsing when nothing changed.
static DATA_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn current_generation() -> u64 {
    DATA_GENERATION.load(Ordering::Relaxed)
}

/// Cached data with generation tracking.
#[derive(Debug, Clone)]
pub struct DataCache {
    pub generation: u64,
    pub sessions: Option<Vec<(String, TokenStats)>>,
    pub activity: Option<Vec<DailyActivity>>,
}

impl DataCache {
    pub fn new() -> Self {
        Self {
            generation: 0,
            sessions: None,
            activity: None,
        }
    }
}

/// Events sent from the notify callback to the watcher worker thread.
enum WatcherEvent {
    Modify(PathBuf),
    Create(PathBuf),
}

/// Spawn a background thread that watches all project directories for new/modified
/// session files. Uses a channel-based worker with trailing-edge debounce.
pub fn spawn_file_watcher(
    projects_dir: PathBuf,
    initial_session: Option<PathBuf>,
    stats: StatsUpdate,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let (event_tx, event_rx) = std::sync::mpsc::channel::<WatcherEvent>();

    // Notify callback: thin filter, just sends events through channel
    let watcher_projects = projects_dir.clone();
    let mut watcher = match notify::recommended_watcher(
        move |res: Result<Event, notify::Error>| {
            let event = match res {
                Ok(e) => e,
                Err(_) => return,
            };

            let is_session_file = |p: &Path| {
                p.extension().map_or(false, |e| e == "jsonl")
                    && p.parent()
                        .and_then(|pp| pp.parent())
                        .map_or(false, |gp| gp == watcher_projects)
            };
            let jsonl_paths: Vec<_> = event
                .paths
                .iter()
                .filter(|p| is_session_file(p))
                .cloned()
                .collect();

            if jsonl_paths.is_empty() {
                return;
            }

            match event.kind {
                EventKind::Modify(_) => {
                    for p in jsonl_paths {
                        let _ = event_tx.send(WatcherEvent::Modify(p));
                    }
                }
                EventKind::Create(_) => {
                    if let Some(p) = jsonl_paths.into_iter().next() {
                        let _ = event_tx.send(WatcherEvent::Create(p));
                    }
                }
                _ => {}
            }
        },
    ) {
        Ok(w) => w,
        Err(_) => return,
    };

    let _ = watcher.watch(&projects_dir, RecursiveMode::Recursive);

    // Worker thread: trailing-edge debounce + parsing
    std::thread::spawn(move || {
        let mut active_file = initial_session.clone();
        const DEBOUNCE: Duration = Duration::from_millis(500);

        // Initial parse
        if let Some(ref f) = active_file {
            let s = parse_token_stats(f);
            *stats.lock().unwrap_or_else(|e| e.into_inner()) = s;
        }

        loop {
            // Block waiting for the first event
            let first = match event_rx.recv() {
                Ok(e) => e,
                Err(_) => break, // channel closed
            };

            // Apply the event
            match first {
                WatcherEvent::Create(p) => active_file = Some(p),
                WatcherEvent::Modify(p) => {
                    if active_file.as_ref() != Some(&p) {
                        active_file = Some(p);
                    }
                }
            }

            // Drain remaining events with trailing-edge debounce
            let mut deadline = Instant::now() + DEBOUNCE;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match event_rx.recv_timeout(remaining) {
                    Ok(WatcherEvent::Create(p)) => {
                        active_file = Some(p);
                        deadline = Instant::now() + DEBOUNCE;
                    }
                    Ok(WatcherEvent::Modify(p)) => {
                        if active_file.as_ref() != Some(&p) {
                            active_file = Some(p);
                        }
                        deadline = Instant::now() + DEBOUNCE;
                    }
                    Err(_) => break,
                }
            }

            // Parse the active file
            if let Some(ref f) = active_file {
                let s = parse_token_stats(f);
                *stats.lock().unwrap_or_else(|e| e.into_inner()) = s;
                DATA_GENERATION.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Watcher dropped — block until shutdown
        let _ = shutdown_rx.blocking_recv();
    });
}

pub fn find_all_sessions() -> HashMap<String, TokenStats> {
    let mut sessions = HashMap::new();

    for_each_session_file(|path| {
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        sessions.insert(name, parse_token_stats(&path));
    });

    sessions
}

#[allow(dead_code)]
pub fn format_tokens(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

/// Try to find sessions modified today and aggregate their token usage
pub fn today_total() -> TokenStats {
    let mut total = TokenStats::default();
    let today = Utc::now().format("%Y-%m-%d").to_string();

    for_each_session_file(|path| {
        if let Ok(meta) = path.metadata() {
            if let Ok(modified) = meta.modified() {
                let dt: chrono::DateTime<Utc> = modified.into();
                if dt.format("%Y-%m-%d").to_string() == today {
                    let s = parse_token_stats(&path);
                    total.input_tokens += s.input_tokens;
                    total.output_tokens += s.output_tokens;
                    total.cache_read_tokens += s.cache_read_tokens;
                    total.cache_write_tokens += s.cache_write_tokens;
                    total.total_tokens += s.total_tokens;
                    total.estimated_cost_usd += s.estimated_cost_usd;
                    if is_valid_model(&s.model) {
                        total.model = s.model;
                    }
                }
            }
        }
    });

    total
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyActivity {
    pub date: String,
    pub count: u64,
}

/// Aggregate token usage per day over the last N days across all sessions
pub fn daily_activity(days: u32) -> Vec<DailyActivity> {
    use std::collections::BTreeMap;

    let mut day_map: BTreeMap<String, u64> = BTreeMap::new();

    // Pre-fill the date range
    let end = Utc::now();
    for i in 0..days {
        let d = end - chrono::Duration::days(i as i64);
        day_map.insert(d.format("%Y-%m-%d").to_string(), 0);
    }

    for_each_session_file(|path| {
        // Quick check: skip sessions older than our window
        if let Ok(meta) = path.metadata() {
            if let Ok(modified) = meta.modified() {
                let cutoff = end - chrono::Duration::days(days as i64);
                let mtime: chrono::DateTime<Utc> = modified.into();
                if mtime < cutoff {
                    return;
                }
            }
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return,
        };

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let msg: DailyLine = match serde_json::from_str(line) {
                Ok(m) => m,
                Err(_) => continue,
            };

            if msg.msg_type.as_deref() != Some("assistant") {
                continue;
            }

            let ts = match msg.timestamp.as_deref() {
                Some(t) if t.len() >= 10 => &t[..10],
                _ => continue,
            };

            let tokens = msg
                .message
                .as_ref()
                .and_then(|m| m.usage.as_ref())
                .map(|u| {
                    u.input_tokens.unwrap_or(0)
                        + u.output_tokens.unwrap_or(0)
                        + u.cache_read_input_tokens.unwrap_or(0)
                        + u.cache_creation_input_tokens.unwrap_or(0)
                })
                .unwrap_or(0);

            if tokens > 0 {
                *day_map.entry(ts.to_string()).or_insert(0) += tokens;
            }
        }
    });

    day_map
        .into_iter()
        .map(|(date, count)| DailyActivity { date, count })
        .collect()
}
