//! Usage from each engine's own local session history. Only timestamps, models
//! and token counts leave a file, and only history files are opened: no credential
//! file is ever read. Files are large, so they are streamed and unchanged ones
//! are served from memory. Claude and Codex write one log per conversation; Muse
//! writes a `session.jsonl` per session, including its subagents.

mod limits;
#[cfg(test)]
#[path = "usage/limits_tests.rs"]
pub(crate) mod limits_tests;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use orbyn_contracts::{
    DailyUsage, EngineLimits, EngineUsage, LimitWindow, ModelPrice, ModelUsage, PooledWindow,
    ServerSettings, TierUsage, TokenCosts, UsageReport,
};
use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::util::{new_id, now, parse_time, write_private};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Tokens {
    fn add(&mut self, other: Tokens) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }

    fn total(self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }
}

/// `(day, model, tier) -> tokens` for one file.
type FileTotals = BTreeMap<(String, String, String), Tokens>;

pub const STANDARD_TIER: &str = "standard";

/// The tiers history names. Anything else is counted as `other`, never dropped.
fn tier_of(name: Option<&str>) -> String {
    match name.map(str::trim).filter(|name| !name.is_empty()) {
        None => STANDARD_TIER.to_owned(),
        Some(name @ ("standard" | "fast" | "priority" | "batch" | "flex")) => name.to_owned(),
        Some("default" | "auto") => STANDARD_TIER.to_owned(),
        Some(_) => "other".to_owned(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Engine {
    Claude,
    Codex,
    Muse,
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn day_of(entry: &Value) -> String {
    entry
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|stamp| stamp.get(..10))
        .unwrap_or("unknown")
        .to_owned()
}

/// Claude writes a message repeatedly as it streams, so the last line of each
/// message stands. Input and cache counts are already separate. Fast mode is
/// `speed: fast`; otherwise the `service_tier` names the tier.
pub fn scan_claude(reader: impl BufRead) -> FileTotals {
    let mut messages: HashMap<String, (String, String, String, Tokens)> = HashMap::new();
    for line in reader.lines().map_while(Result::ok) {
        if !line.contains("\"usage\"") {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(message) = entry.get("message") else {
            continue;
        };
        let Some(usage) = message.get("usage") else {
            continue;
        };
        let Some(id) = message.get("id").and_then(Value::as_str) else {
            continue;
        };
        let model = message
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        let tier = if usage.get("speed").and_then(Value::as_str) == Some("fast") {
            "fast".to_owned()
        } else {
            tier_of(usage.get("service_tier").and_then(Value::as_str))
        };
        messages.insert(
            id.to_owned(),
            (
                day_of(&entry),
                model,
                tier,
                Tokens {
                    input: number(usage, "input_tokens"),
                    output: number(usage, "output_tokens"),
                    cache_read: number(usage, "cache_read_input_tokens"),
                    cache_write: number(usage, "cache_creation_input_tokens"),
                },
            ),
        );
    }
    let mut totals = FileTotals::new();
    for (day, model, tier, tokens) in messages.into_values() {
        totals.entry((day, model, tier)).or_default().add(tokens);
    }
    totals
}

/// Codex logs a running total with each turn, and repeats it unchanged. Its input
/// count includes cached input, so that part is moved out. A turn context's
/// `service_tier` names the tier of the turns after it.
pub fn scan_codex(reader: impl BufRead) -> FileTotals {
    let mut totals = FileTotals::new();
    let mut model = "unknown".to_owned();
    let mut tier = STANDARD_TIER.to_owned();
    let mut last_total = None;
    for line in reader.lines().map_while(Result::ok) {
        if !line.contains("token_count") && !line.contains("turn_context") {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let payload = entry.get("payload").unwrap_or(&Value::Null);
        if entry.get("type").and_then(Value::as_str) == Some("turn_context") {
            if let Some(name) = payload.get("model").and_then(Value::as_str) {
                model = name.to_owned();
            }
            tier = tier_of(payload.get("service_tier").and_then(Value::as_str));
            continue;
        }
        if payload.get("type").and_then(Value::as_str) != Some("token_count") {
            continue;
        }
        let Some(info) = payload.get("info").filter(|info| !info.is_null()) else {
            continue;
        };
        let total = info
            .pointer("/total_token_usage/total_tokens")
            .and_then(Value::as_u64);
        if total == last_total {
            continue;
        }
        last_total = total;
        let Some(last) = info.get("last_token_usage") else {
            continue;
        };
        let cached = number(last, "cached_input_tokens");
        totals
            .entry((day_of(&entry), model.clone(), tier.clone()))
            .or_default()
            .add(Tokens {
                input: number(last, "input_tokens").saturating_sub(cached),
                output: number(last, "output_tokens"),
                cache_read: cached,
                cache_write: number(last, "cache_write_input_tokens"),
            });
    }
    totals
}

/// One inference from a Muse session log: its identity, day, model and tokens.
type Inference = (String, String, String, Tokens);

/// Muse records each finished inference as a `runtime.session` event whose
/// counters are inclusive: the cached part is already inside `input_tokens` and
/// reasoning is already inside `output_tokens`. A runtime envelope can be copied
/// into another session's log, so each inference keeps the id that identifies the
/// inference (`source_run_record_id`, else its own `id`) and is counted once over
/// the whole history. The record shape is the one T3's
/// `packages/provider-muse/src/server/usage.ts` reads (MIT, commit 9168ca73d),
/// including its field precedence and its rejection of unsupported schemas.
pub fn scan_muse(reader: impl BufRead) -> Vec<Inference> {
    let mut rows = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        if !line.contains("model_completed") && !line.contains("automated_review_completed") {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(row) = muse_row(&entry) {
            rows.push(row);
        }
    }
    rows
}

/// A non-empty string field, for records whose identifiers cannot be blank.
fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

fn muse_row(entry: &Value) -> Option<Inference> {
    if number(entry, "schema_version") != 1
        || entry.get("record_type").and_then(Value::as_str) != Some("event")
        || entry.get("payload_type").and_then(Value::as_str) != Some("runtime.session")
        || !matches!(number(entry, "payload_schema_version"), 1 | 3)
    {
        return None;
    }
    let id = text(entry.get("id"))?;
    let stream = entry.get("stream")?;
    if stream.get("kind").and_then(Value::as_str) != Some("session")
        || text(stream.get("id")).is_none()
    {
        return None;
    }
    let day = day_of_micros(entry.get("recorded_at").and_then(Value::as_u64)?)?;
    let event = entry.pointer("/payload/event")?;
    if !matches!(
        event.get("kind").and_then(Value::as_str),
        Some("model_completed" | "automated_review_completed")
    ) {
        return None;
    }
    let model = match event.get("model")? {
        Value::String(name) if !name.is_empty() => name.clone(),
        model => text(model.get("model_id"))?.to_owned(),
    };
    let usage = event.get("usage")?;
    let input = number(usage, "input_tokens");
    let cache_read = ["cache_read_tokens", "cached_input_tokens", "cached_tokens"]
        .into_iter()
        .find_map(|key| usage.get(key).and_then(Value::as_u64))
        .unwrap_or(0)
        .min(input);
    let cache_write = number(usage, "cache_write_tokens").min(input - cache_read);
    let output = number(usage, "output_tokens");
    // A record without tokens proves no spend, so it is not counted at all.
    if input + output == 0 {
        return None;
    }
    let key = entry
        .pointer("/payload/source_run_record_id")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .unwrap_or(id);
    Some((
        key.to_owned(),
        day,
        model,
        Tokens {
            input: input - cache_read - cache_write,
            output,
            cache_read,
            cache_write,
        },
    ))
}

/// Muse's `recorded_at` is microseconds from the Unix epoch. A value that names
/// no day is dropped rather than counted on a made-up one.
fn day_of_micros(micros: u64) -> Option<String> {
    let nanos = i128::from(micros).checked_mul(1_000)?;
    let at = time::OffsetDateTime::from_unix_timestamp_nanos(nanos).ok()?;
    Some(at.date().to_string())
}

fn jsonl_files(root: &Path, name: Option<&str>, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // History is plain folders; a symlink could lead anywhere.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            jsonl_files(&path, name, found);
        } else if kind.is_file()
            && path.extension().is_some_and(|ext| ext == "jsonl")
            && name.is_none_or(|name| path.file_name().is_some_and(|file| file == name))
        {
            found.push(path);
        }
    }
}

/// The launch profile that owns this process. The packaged smoke harness declares
/// [`PROFILE_ENV`] for the app it starts; every other launch is a person's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryProfile {
    Personal,
    Smoke,
}

/// What a launcher sets to declare the profile it starts: `smoke`, and nothing else.
pub(crate) const PROFILE_ENV: &str = "ORBYN_LAUNCH_PROFILE";
/// The sandbox root the smoke harness declares for the profile it hands out.
pub(crate) const SANDBOX_ENV: &str = "ORBYN_SMOKE_SANDBOX";

impl HistoryProfile {
    fn declared(value: Option<&std::ffi::OsStr>) -> Self {
        match value.and_then(std::ffi::OsStr::to_str) {
            Some("smoke") => Self::Smoke,
            _ => Self::Personal,
        }
    }

    fn current() -> Self {
        Self::declared(std::env::var_os(PROFILE_ENV).as_deref())
    }
}

/// The environment one history root is resolved from, so both launch profiles are
/// verifiable without changing this process's own variables.
pub(crate) struct HistoryEnv {
    pub(crate) profile: HistoryProfile,
    pub(crate) sandbox: Option<PathBuf>,
    pub(crate) claude_config_dir: Option<PathBuf>,
    pub(crate) codex_home: Option<PathBuf>,
    pub(crate) xdg_data_home: Option<PathBuf>,
    pub(crate) home: Option<PathBuf>,
    pub(crate) account_home: Option<PathBuf>,
}

impl HistoryEnv {
    fn from_process() -> Self {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
        Self {
            profile: HistoryProfile::current(),
            sandbox: var(SANDBOX_ENV),
            claude_config_dir: var("CLAUDE_CONFIG_DIR"),
            codex_home: var("CODEX_HOME"),
            xdg_data_home: var("XDG_DATA_HOME"),
            home: crate::util::home_dir(),
            account_home: crate::util::account_home(),
        }
    }

    /// One engine's history folder, resolved for the profile that owns this process.
    fn root(&self, engine: Engine) -> Option<PathBuf> {
        match engine {
            Engine::Claude => self
                .base(self.claude_config_dir.as_deref(), ".claude")
                .map(|root| root.join("projects")),
            Engine::Codex => self
                .base(self.codex_home.as_deref(), ".codex")
                .map(|root| root.join("sessions")),
            // Muse resolves its own home when it starts, and no Muse driver is here to
            // ask. Its recorded logs sit under the XDG data directory
            // (`~/.local/share/muse/sessions/2026/10/11/<session>/session.jsonl`).
            Engine::Muse => self
                .base(self.xdg_data_home.as_deref(), ".local/share")
                .map(|root| root.join("muse").join("sessions")),
        }
    }

    /// The folder an engine keeps under its own profile: the variable that names it,
    /// but only while that path is this launch's own, else the person's own home.
    fn base(&self, named: Option<&Path>, under_home: &str) -> Option<PathBuf> {
        if let Some(named) = named.filter(|named| self.owns(named)) {
            return Some(named.to_path_buf());
        }
        self.own_home().map(|home| home.join(under_home))
    }

    /// Whether a relocated path is this launch's own. The smoke profile owns its
    /// sandbox; a personal launch owns nothing inside a sandbox it did not declare, so a
    /// harness profile inherited from a launcher never becomes the person's history.
    fn owns(&self, path: &Path) -> bool {
        self.profile == HistoryProfile::Smoke
            || self
                .sandbox
                .as_deref()
                .is_none_or(|sandbox| !path.starts_with(sandbox))
    }

    /// The home this launch reads: the declared sandbox for the smoke profile; for a
    /// person, the home the environment names while it is this launch's own, else the
    /// account's own home from the user database.
    fn own_home(&self) -> Option<PathBuf> {
        if self.profile == HistoryProfile::Smoke {
            return self.home.clone();
        }
        match self.home.clone().filter(|home| self.owns(home)) {
            Some(home) => Some(home),
            // The inherited home is the harness's, so nothing in it is this launch's to
            // read. Without an account home the root is unknown rather than the sandbox.
            None => self.account_home.clone(),
        }
    }
}

pub(crate) fn history_root(engine: Engine) -> Option<PathBuf> {
    HistoryEnv::from_process().root(engine)
}

/// What a file held when last read, keyed by its size and modified time.
#[derive(Clone)]
struct Cached {
    size: u64,
    modified: Option<SystemTime>,
    scan: Scan,
}

/// One file's scan. Muse keeps a row per inference, because the same inference can
/// appear in another session's log and must still count once over the history.
#[derive(Clone)]
enum Scan {
    Totals(FileTotals),
    Inferences(Vec<Inference>),
}

impl Scan {
    /// Adds this file to the history being combined. `counted` holds the Muse
    /// inference ids already seen in other files.
    fn add(self, combined: &mut FileTotals, counted: &mut HashSet<String>) {
        match self {
            Scan::Totals(totals) => {
                for (key, tokens) in totals {
                    combined.entry(key).or_default().add(tokens);
                }
            }
            Scan::Inferences(rows) => {
                for (inference, day, model, tokens) in rows {
                    if counted.insert(inference) {
                        combined
                            .entry((day, model, STANDARD_TIER.to_owned()))
                            .or_default()
                            .add(tokens);
                    }
                }
            }
        }
    }
}

#[derive(Default, Clone)]
struct ScanCache {
    files: HashMap<PathBuf, Cached>,
}

fn scan_engine(
    engine: Engine,
    cache: &mut ScanCache,
    stop: Option<&tokio_util::sync::CancellationToken>,
) -> (String, u32, u32, FileTotals) {
    let Some(root) = history_root(engine).filter(|root| root.is_dir()) else {
        return ("no_history".to_owned(), 0, 0, FileTotals::new());
    };
    let mut files = Vec::new();
    // Muse names every session log `session.jsonl`, and its folders hold other
    // `.jsonl` files that are not inference history.
    jsonl_files(
        &root,
        (engine == Engine::Muse).then_some("session.jsonl"),
        &mut files,
    );
    let (mut scanned, mut skipped) = (0, 0);
    let mut combined = FileTotals::new();
    let mut counted = HashSet::new();
    for path in files {
        // A warm-up ends between files when the service is quitting.
        if stop.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
            break;
        }
        let Ok(meta) = std::fs::metadata(&path) else {
            skipped += 1;
            continue;
        };
        let (size, modified) = (meta.len(), meta.modified().ok());
        let cached = cache
            .files
            .get(&path)
            .filter(|entry| entry.size == size && entry.modified == modified)
            .map(|entry| entry.scan.clone());
        let scan = match cached {
            Some(scan) => scan,
            None => {
                let Ok(file) = std::fs::File::open(&path) else {
                    skipped += 1;
                    continue;
                };
                let reader = BufReader::new(file);
                let scan = match engine {
                    Engine::Claude => Scan::Totals(scan_claude(reader)),
                    Engine::Codex => Scan::Totals(scan_codex(reader)),
                    Engine::Muse => Scan::Inferences(scan_muse(reader)),
                };
                cache.files.insert(
                    path,
                    Cached {
                        size,
                        modified,
                        scan: scan.clone(),
                    },
                );
                scan
            }
        };
        scanned += 1;
        scan.add(&mut combined, &mut counted);
    }
    ("scanned".to_owned(), scanned, skipped, combined)
}

fn cost_split(tokens: Tokens, price: &ModelPrice) -> TokenCosts {
    let rate = |count: u64, per_million: f64| count as f64 / 1_000_000.0 * per_million;
    TokenCosts {
        input: rate(tokens.input, price.input),
        output: rate(tokens.output, price.output),
        cache_read: rate(tokens.cache_read, price.cache_read),
        cache_write: rate(tokens.cache_write, price.cache_write),
    }
}

fn split_total(costs: &TokenCosts) -> f64 {
    costs.input + costs.output + costs.cache_read + costs.cache_write
}

/// A tier's own rate, or the model's standard rate for the standard tier. Another
/// tier never borrows the standard rate, since engines bill those differently.
fn price_for<'a>(settings: &'a ServerSettings, model: &str, tier: &str) -> Option<&'a ModelPrice> {
    let tier_of_price =
        |price: &ModelPrice| price.tier.as_deref().unwrap_or(STANDARD_TIER).to_owned();
    settings
        .model_prices
        .iter()
        .find(|price| price.model == model && tier_of_price(price) == tier)
}

/// One day's tokens and the priced part of their cost.
#[derive(Clone, Copy, Default)]
struct Day {
    tokens: u64,
    cost: Option<f64>,
}

impl Day {
    fn add(&mut self, tokens: Tokens, cost: Option<f64>) {
        self.tokens += tokens.total();
        if let Some(cost) = cost {
            self.cost = Some(self.cost.unwrap_or(0.0) + cost);
        }
    }
}

fn daily(days: BTreeMap<String, Day>) -> Vec<DailyUsage> {
    days.into_iter()
        .map(|(date, day)| DailyUsage {
            date,
            tokens: day.tokens,
            estimated_cost: day.cost,
        })
        .collect()
}

fn model_usage(
    model: String,
    tiers: BTreeMap<String, Tokens>,
    days: BTreeMap<String, Day>,
    settings: &ServerSettings,
) -> ModelUsage {
    let mut tokens = Tokens::default();
    let mut split: Option<TokenCosts> = None;
    let mut unpriced = 0;
    let tiers = tiers
        .into_iter()
        .map(|(tier, counted)| {
            tokens.add(counted);
            let costs = price_for(settings, &model, &tier).map(|price| cost_split(counted, price));
            match &costs {
                Some(costs) => {
                    let sum = split.get_or_insert(TokenCosts {
                        input: 0.0,
                        output: 0.0,
                        cache_read: 0.0,
                        cache_write: 0.0,
                    });
                    sum.input += costs.input;
                    sum.output += costs.output;
                    sum.cache_read += costs.cache_read;
                    sum.cache_write += costs.cache_write;
                }
                None => unpriced += counted.total(),
            }
            TierUsage {
                tier,
                tokens: counted.total(),
                estimated_cost: costs.as_ref().map(split_total),
            }
        })
        .collect();
    ModelUsage {
        estimated_cost: split.as_ref().map(split_total),
        cost_split: split,
        tiers,
        unpriced_tokens: unpriced,
        days: daily(days),
        model,
        input_tokens: tokens.input,
        output_tokens: tokens.output,
        cache_read_tokens: tokens.cache_read,
        cache_write_tokens: tokens.cache_write,
    }
}

/// `YYYY-MM-DD` of the first day inside a period of `days`, today included.
fn period_start(days: u32) -> String {
    let today = time::OffsetDateTime::now_utc().date();
    (today - time::Duration::days(i64::from(days.max(1)) - 1)).to_string()
}

fn summarize(
    engine: &str,
    (status, scanned, skipped, totals): (String, u32, u32, FileTotals),
    settings: &ServerSettings,
    since: Option<&str>,
) -> EngineUsage {
    let resolve = |model: &str| {
        settings
            .model_aliases
            .iter()
            .find(|alias| alias.model == model)
            .map_or_else(|| model.to_owned(), |alias| alias.counts_as.clone())
    };
    let mut models: BTreeMap<String, BTreeMap<String, Tokens>> = BTreeMap::new();
    let mut model_days: BTreeMap<String, BTreeMap<String, Day>> = BTreeMap::new();
    let mut days: BTreeMap<String, Day> = BTreeMap::new();
    for ((day, model, tier), tokens) in totals {
        if since.is_some_and(|since| day.as_str() < since) {
            continue;
        }
        let model = resolve(&model);
        let cost =
            price_for(settings, &model, &tier).map(|price| split_total(&cost_split(tokens, price)));
        model_days
            .entry(model.clone())
            .or_default()
            .entry(day.clone())
            .or_default()
            .add(tokens, cost);
        models
            .entry(model)
            .or_default()
            .entry(tier)
            .or_default()
            .add(tokens);
        days.entry(day).or_default().add(tokens, cost);
    }
    EngineUsage {
        engine: engine.to_owned(),
        status,
        files_scanned: scanned,
        files_skipped: skipped,
        models: models
            .into_iter()
            .map(|(model, tiers)| {
                let days = model_days.remove(&model).unwrap_or_default();
                model_usage(model, tiers, days, settings)
            })
            .collect(),
        days: daily(days),
    }
}

pub struct UsageCenter {
    host_id: String,
    cache: Arc<Mutex<ScanCache>>,
    limits: limits::LimitsReads,
}

impl UsageCenter {
    /// Keeps a stable id for this machine in app data.
    pub fn new(data_dir: &Path) -> Result<Self> {
        let path = data_dir.join("host-id");
        let host_id = match std::fs::read_to_string(&path) {
            Ok(text) if !text.trim().is_empty() => text.trim().to_owned(),
            _ => {
                let id = new_id();
                write_private(&path, id.as_bytes())?;
                id
            }
        };
        Ok(Self {
            host_id,
            cache: Arc::default(),
            limits: limits::LimitsReads::default(),
        })
    }

    /// Scans off the async threads, since history can run to gigabytes. Unchanged
    /// files come from memory, so a scan is cheap and a price change shows at once.
    pub async fn report(
        &self,
        settings: &ServerSettings,
        days: Option<u32>,
    ) -> Result<UsageReport> {
        let cache = self.cache.clone();
        let scans = tokio::task::spawn_blocking(move || {
            let mut cache = cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            [
                ("claude", scan_engine(Engine::Claude, &mut cache, None)),
                ("codex", scan_engine(Engine::Codex, &mut cache, None)),
                ("muse", scan_engine(Engine::Muse, &mut cache, None)),
            ]
        })
        .await
        .map_err(|error| CoreError::Internal(error.to_string()))?;
        let since = days.map(period_start);
        Ok(UsageReport {
            host_id: self.host_id.clone(),
            scanned_at: now(),
            engines: scans
                .into_iter()
                .map(|(name, scan)| summarize(name, scan, settings, since.as_deref()))
                .collect(),
        })
    }

    /// Reads history into memory ahead of the first request, which otherwise waits
    /// for gigabytes of files. It works on a copy of what is known and merges the
    /// result in at the end, so a request never waits on the cache lock for the scan.
    /// `stop` ends it between files when the service quits; what it read is kept.
    pub async fn warm(&self, stop: tokio_util::sync::CancellationToken) {
        let cache = self.cache.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let mut local = cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            scan_engine(Engine::Claude, &mut local, Some(&stop));
            scan_engine(Engine::Codex, &mut local, Some(&stop));
            scan_engine(Engine::Muse, &mut local, Some(&stop));
            let mut shared = cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for (path, entry) in local.files {
                shared.files.entry(path).or_insert(entry);
            }
        })
        .await;
    }

    /// Forgets every scan and limit, so the next read goes back to the sources.
    pub fn forget(&self) {
        *self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = ScanCache::default();
        self.limits.forget();
    }

    pub fn set_notify(&self, notify: &crate::util::swr::Notify) {
        self.limits.set_notify(notify.clone());
    }
}

/// Claude's driver supplies sanitized windows; Codex supplies a `rateLimits`
/// block with reset times as Unix seconds.
fn limit_windows(report: &Value) -> Vec<LimitWindow> {
    if let Some(windows) = report.get("windows") {
        return serde_json::from_value(windows.clone()).unwrap_or_default();
    }
    let Some(limits) = report.get("rateLimits") else {
        return Vec::new();
    };
    ["primary", "secondary"]
        .into_iter()
        .filter_map(|id| {
            let window = limits.get(id)?;
            let minutes = window.get("windowDurationMins").and_then(Value::as_u64);
            Some(LimitWindow {
                id: id.to_owned(),
                label: window_label(minutes),
                used_percent: window.get("usedPercent")?.as_f64()?,
                window_minutes: minutes,
                resets_at: window
                    .get("resetsAt")
                    .and_then(Value::as_i64)
                    .and_then(rfc3339),
            })
        })
        .collect()
}

fn rfc3339(unix: i64) -> Option<String> {
    let at = time::OffsetDateTime::from_unix_timestamp(unix).ok()?;
    at.format(&time::format_description::well_known::Rfc3339)
        .ok()
}

fn window_label(minutes: Option<u64>) -> String {
    match minutes {
        Some(minutes) if minutes % 1440 == 0 && minutes >= 1440 => {
            format!("{} days", minutes / 1440)
        }
        Some(minutes) if minutes % 60 == 0 && minutes >= 60 => format!("{} hours", minutes / 60),
        Some(minutes) => format!("{minutes} minutes"),
        None => "Window".to_owned(),
    }
}

fn pool_windows(engines: &[EngineLimits]) -> Vec<PooledWindow> {
    let mut pooled: Vec<PooledWindow> = Vec::new();
    for engine in engines {
        for window in &engine.windows {
            let slot = pooled.iter().position(|known| {
                window.window_minutes.is_some() && known.window_minutes == window.window_minutes
            });
            let slot = slot.unwrap_or_else(|| {
                pooled.push(PooledWindow {
                    label: window.label.clone(),
                    window_minutes: window.window_minutes,
                    used_percent: 0.0,
                    resets_at: None,
                    engines: Vec::new(),
                });
                pooled.len() - 1
            });
            let entry = &mut pooled[slot];
            entry.used_percent = entry.used_percent.max(window.used_percent);
            if !entry.engines.contains(&engine.engine) {
                entry.engines.push(engine.engine.clone());
            }
            entry.resets_at = match (entry.resets_at.take(), &window.resets_at) {
                (Some(known), Some(new)) => Some(if parse_time(new) < parse_time(&known) {
                    new.clone()
                } else {
                    known
                }),
                (known, new) => known.or_else(|| new.clone()),
            };
        }
    }
    pooled.sort_by_key(|window| window.window_minutes);
    pooled
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbyn_contracts::ModelAlias;
    use serde_json::json;
    use std::time::Duration;

    fn lines(items: &[&str]) -> std::io::Cursor<Vec<u8>> {
        std::io::Cursor::new(items.join("\n").into_bytes())
    }

    fn only(totals: &FileTotals) -> Tokens {
        assert_eq!(totals.len(), 1, "{totals:?}");
        *totals.values().next().unwrap()
    }

    /// An environment a smoke launcher handed out, inherited by another launch: every
    /// profile path sits inside the sandbox and the real account is elsewhere.
    fn inherited_smoke_env() -> HistoryEnv {
        let sandbox = PathBuf::from("/smoke/root");
        let home = sandbox.join("home");
        HistoryEnv {
            profile: HistoryProfile::Personal,
            sandbox: Some(sandbox),
            claude_config_dir: Some(home.join(".claude")),
            codex_home: Some(home.join(".codex")),
            xdg_data_home: Some(home.join(".local").join("share")),
            home: Some(home),
            account_home: Some(PathBuf::from("/people/ana")),
        }
    }

    fn engines() -> [Engine; 3] {
        [Engine::Claude, Engine::Codex, Engine::Muse]
    }

    #[test]
    fn only_a_declared_smoke_launch_is_the_smoke_profile() {
        use std::ffi::OsStr;
        assert_eq!(HistoryProfile::declared(None), HistoryProfile::Personal);
        assert_eq!(
            HistoryProfile::declared(Some(OsStr::new("personal"))),
            HistoryProfile::Personal
        );
        assert_eq!(
            HistoryProfile::declared(Some(OsStr::new("smoke"))),
            HistoryProfile::Smoke
        );
    }

    #[test]
    fn a_personal_launch_reads_the_accounts_own_history_not_an_inherited_smoke_profile() {
        let env = inherited_smoke_env();
        assert_eq!(
            env.root(Engine::Claude),
            Some(PathBuf::from("/people/ana/.claude/projects"))
        );
        assert_eq!(
            env.root(Engine::Codex),
            Some(PathBuf::from("/people/ana/.codex/sessions"))
        );
        assert_eq!(
            env.root(Engine::Muse),
            Some(PathBuf::from("/people/ana/.local/share/muse/sessions"))
        );
        // The negative case: no engine reads a path the harness owns.
        let sandbox = env.sandbox.clone().expect("the smoke's sandbox");
        for engine in engines() {
            let root = env.root(engine).expect("a root");
            assert!(!root.starts_with(&sandbox), "{engine:?} got {root:?}");
        }
    }

    #[test]
    fn a_declared_smoke_launch_keeps_reading_its_own_sandbox_history() {
        let mut env = inherited_smoke_env();
        env.profile = HistoryProfile::Smoke;
        assert_eq!(
            env.root(Engine::Claude),
            Some(PathBuf::from("/smoke/root/home/.claude/projects"))
        );
        assert_eq!(
            env.root(Engine::Codex),
            Some(PathBuf::from("/smoke/root/home/.codex/sessions"))
        );
        assert_eq!(
            env.root(Engine::Muse),
            Some(PathBuf::from("/smoke/root/home/.local/share/muse/sessions"))
        );
        // The same environment resolves to a different path per profile.
        let personal = inherited_smoke_env();
        for engine in engines() {
            assert_ne!(env.root(engine), personal.root(engine), "{engine:?}");
        }
    }

    #[test]
    fn a_personal_launch_keeps_a_deliberate_config_override() {
        // Without a declared sandbox, a person's own relocation stays authoritative.
        let mut env = inherited_smoke_env();
        env.sandbox = None;
        assert_eq!(
            env.root(Engine::Codex),
            Some(PathBuf::from("/smoke/root/home/.codex/sessions"))
        );
        assert_eq!(
            env.root(Engine::Claude),
            Some(PathBuf::from("/smoke/root/home/.claude/projects"))
        );
    }

    #[test]
    fn a_personal_launch_reports_no_history_rather_than_the_sandbox_it_inherited() {
        // The account home could not be resolved: the inherited sandbox is still refused.
        let mut env = inherited_smoke_env();
        env.account_home = None;
        for engine in engines() {
            assert_eq!(env.root(engine), None, "{engine:?}");
        }
    }

    #[test]
    fn windows_pool_by_length_and_keep_the_worst_number() {
        let report = |percent: f64, resets: i64| {
            json!({ "rateLimits": { "primary": {
                "usedPercent": percent, "windowDurationMins": 300, "resetsAt": resets,
            }}})
        };
        let limits = |engine: &str, report: Value| EngineLimits {
            engine: engine.into(),
            checked_at: String::new(),
            windows: limit_windows(&report),
            plan: None,
            limits: report,
        };
        let pooled = pool_windows(&[
            limits("a", report(10.0, 2_000_000_000)),
            limits("b", report(60.0, 1_900_000_000)),
        ]);
        assert_eq!(pooled.len(), 1);
        assert_eq!(pooled[0].label, "5 hours");
        assert_eq!(pooled[0].used_percent, 60.0);
        assert_eq!(pooled[0].engines, ["a", "b"]);
        assert_eq!(
            parse_time(pooled[0].resets_at.as_ref().unwrap()),
            Some(1_900_000_000)
        );
        assert!(limit_windows(&json!({ "other": 1 })).is_empty());
    }

    #[test]
    fn scoped_windows_keep_one_engine_in_a_shared_pool() {
        let report = json!({ "windows": [
            { "id": "seven_day", "label": "Weekly", "used_percent": 20.0, "window_minutes": 10080, "resets_at": null },
            { "id": "model:Example", "label": "Weekly · Example", "used_percent": 70.0, "window_minutes": 10080, "resets_at": null }
        ] });
        let pooled = pool_windows(&[EngineLimits {
            engine: "claude".into(),
            checked_at: String::new(),
            windows: limit_windows(&report),
            plan: None,
            limits: report,
        }]);
        assert_eq!(pooled.len(), 1);
        assert_eq!(pooled[0].used_percent, 70.0);
        assert_eq!(pooled[0].engines, ["claude"]);
        assert!(pooled[0].resets_at.is_none());
    }

    #[test]
    fn unknown_lengths_do_not_pool_unrelated_windows() {
        let engine = |id: &str, labels: &[&str]| EngineLimits {
            engine: id.into(),
            checked_at: String::new(),
            plan: None,
            limits: Value::Null,
            windows: labels
                .iter()
                .enumerate()
                .map(|(index, label)| LimitWindow {
                    id: index.to_string(),
                    label: (*label).into(),
                    used_percent: 20.0,
                    window_minutes: None,
                    resets_at: None,
                })
                .collect(),
        };
        let pooled = pool_windows(&[
            engine("codex", &["Window"]),
            engine("claude", &["Monthly spend"]),
        ]);
        assert_eq!(pooled.len(), 2);
        assert_eq!(pooled[0].label, "Window");
        assert_eq!(pooled[0].engines, ["codex"]);
        assert_eq!(pooled[1].label, "Monthly spend");
        assert_eq!(pooled[1].engines, ["claude"]);

        let pooled = pool_windows(&[engine("claude", &["Window", "Window"])]);
        assert_eq!(
            pooled.len(),
            2,
            "a shared label does not establish a shared budget"
        );
        assert!(
            pooled
                .iter()
                .all(|window| window.window_minutes.is_none() && window.engines == ["claude"])
        );
    }

    #[test]
    fn a_period_keeps_only_its_days() {
        let mut totals = FileTotals::new();
        let tokens = Tokens {
            input: 5,
            ..Tokens::default()
        };
        totals.insert(
            ("2000-01-01".into(), "m".into(), STANDARD_TIER.into()),
            tokens,
        );
        totals.insert((period_start(1), "m".into(), STANDARD_TIER.into()), tokens);
        let settings = crate::store::default_settings();
        let scan = ("scanned".to_owned(), 1, 0, totals);
        let usage = summarize("claude", scan.clone(), &settings, Some(&period_start(7)));
        assert_eq!(usage.days.len(), 1);
        assert_eq!(usage.models[0].input_tokens, 5);
        assert_eq!(
            summarize("claude", scan, &settings, None).models[0].input_tokens,
            10
        );
    }

    #[test]
    fn claude_counts_a_streamed_message_once_with_its_final_numbers() {
        let totals = scan_claude(lines(&[
            r#"{"timestamp":"2026-10-30T01:00:00Z","message":{"id":"m1","model":"opus","usage":{"input_tokens":5,"output_tokens":1}}}"#,
            r#"{"timestamp":"2026-10-30T01:00:01Z","message":{"id":"m1","model":"opus","usage":{"input_tokens":5,"output_tokens":9,"cache_read_input_tokens":3,"cache_creation_input_tokens":2}}}"#,
            "not json with \"usage\" in it",
            r#"{"message":{"usage":{"input_tokens":100}}}"#,
        ]));
        assert_eq!(
            only(&totals),
            Tokens {
                input: 5,
                output: 9,
                cache_read: 3,
                cache_write: 2
            }
        );
        assert!(totals.contains_key(&(
            "2026-10-30".to_owned(),
            "opus".to_owned(),
            "standard".to_owned()
        )));
    }

    #[test]
    fn codex_skips_repeated_totals_and_moves_cached_input_out() {
        let event = |total: u64| {
            format!(
                r#"{{"timestamp":"2026-10-30T02:00:00Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"total_tokens":{total}}},"last_token_usage":{{"input_tokens":100,"cached_input_tokens":40,"output_tokens":7}}}}}}}}"#
            )
        };
        let context = r#"{"type":"turn_context","payload":{"model":"gpt-x"}}"#;
        let empty = r#"{"type":"event_msg","payload":{"type":"token_count","info":null}}"#;
        let (a, b) = (event(107), event(214));
        let totals = scan_codex(lines(&[context, empty, &a, &a, &b]));
        assert_eq!(
            only(&totals),
            Tokens {
                input: 120,
                output: 14,
                cache_read: 80,
                cache_write: 0
            }
        );
        assert!(totals.contains_key(&(
            "2026-10-30".to_owned(),
            "gpt-x".to_owned(),
            "standard".to_owned()
        )));
    }

    #[test]
    fn prices_follow_aliases_and_an_unpriced_model_has_no_cost() {
        let mut settings = crate::store::default_settings();
        settings.model_prices = vec![ModelPrice {
            model: "base".into(),
            input: 2.0,
            output: 10.0,
            cache_read: 1.0,
            cache_write: 0.0,
            tier: None,
        }];
        settings.model_aliases = vec![ModelAlias {
            model: "base-2026".into(),
            counts_as: "base".into(),
        }];
        let mut totals = FileTotals::new();
        let tokens = Tokens {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 1_000_000,
            cache_write: 0,
        };
        totals.insert(
            ("d".into(), "base-2026".into(), STANDARD_TIER.into()),
            tokens,
        );
        totals.insert(("d".into(), "mystery".into(), STANDARD_TIER.into()), tokens);
        let usage = summarize("x", ("scanned".into(), 1, 0, totals), &settings, None);
        assert_eq!(usage.models[0].model, "base");
        assert_eq!(usage.models[0].estimated_cost, Some(13.0));
        assert_eq!(usage.models[1].estimated_cost, None);
        assert_eq!(usage.days[0].tokens, 6_000_000);
        assert_eq!(
            usage.models[0].days.len(),
            1,
            "each model keeps its own trend"
        );
        assert_eq!(usage.models[0].days[0].tokens, 3_000_000);
        assert_eq!(
            usage.days[0].estimated_cost,
            Some(13.0),
            "the priced part of the day"
        );
        assert_eq!(usage.models[1].days[0].estimated_cost, None);
    }

    #[test]
    fn tiers_come_from_history_and_are_priced_only_by_their_own_rate() {
        let claude = scan_claude(lines(&[
            r#"{"timestamp":"2026-10-30T01:00:00Z","message":{"id":"a","model":"m","usage":{"input_tokens":10,"speed":"standard","service_tier":"standard"}}}"#,
            r#"{"timestamp":"2026-10-30T01:00:00Z","message":{"id":"b","model":"m","usage":{"input_tokens":20,"speed":"fast","service_tier":"standard"}}}"#,
            r#"{"timestamp":"2026-10-30T01:00:00Z","message":{"id":"c","model":"m","usage":{"input_tokens":30,"service_tier":"priority"}}}"#,
            r#"{"timestamp":"2026-10-30T01:00:00Z","message":{"id":"d","model":"m","usage":{"input_tokens":40,"service_tier":"mystery"}}}"#,
        ]));
        let tiers: Vec<&str> = claude.keys().map(|(_, _, tier)| tier.as_str()).collect();
        assert_eq!(tiers, ["fast", "other", "priority", "standard"]);

        let codex = scan_codex(lines(&[
            r#"{"type":"turn_context","payload":{"model":"g","service_tier":"fast"}}"#,
            r#"{"timestamp":"2026-10-30T02:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":5},"last_token_usage":{"input_tokens":5}}}}"#,
        ]));
        assert!(codex.contains_key(&("2026-10-30".into(), "g".into(), "fast".into())));

        let mut settings = crate::store::default_settings();
        let price = |tier: Option<&str>, input: f64| ModelPrice {
            model: "m".into(),
            input,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            tier: tier.map(str::to_owned),
        };
        let million = |input: u64| Tokens {
            input: input * 1_000_000,
            output: 1_000_000,
            ..Tokens::default()
        };
        settings.model_prices = vec![price(None, 1.0), price(Some("fast"), 6.0)];
        let mut tiers = BTreeMap::new();
        tiers.insert("standard".to_owned(), million(1));
        tiers.insert("fast".to_owned(), million(2));
        tiers.insert("priority".to_owned(), million(3));
        let usage = model_usage("m".into(), tiers, BTreeMap::new(), &settings);
        assert_eq!(usage.estimated_cost, Some(13.0));
        let split = usage.cost_split.unwrap();
        assert_eq!((split.input, split.output), (13.0, 0.0));
        assert_eq!(
            usage.unpriced_tokens, 4_000_000,
            "priority has no rate of its own"
        );
        let priority = usage
            .tiers
            .iter()
            .find(|tier| tier.tier == "priority")
            .unwrap();
        assert_eq!(priority.estimated_cost, None);
        assert_eq!(
            usage
                .tiers
                .iter()
                .find(|tier| tier.tier == "fast")
                .unwrap()
                .estimated_cost,
            Some(12.0)
        );
    }

    #[test]
    fn only_jsonl_files_are_considered() {
        let dir = crate::testing::TempDir::new();
        let nested = dir.path().join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("s.jsonl"), "").unwrap();
        std::fs::write(nested.join("auth.json"), "").unwrap();
        std::fs::write(nested.join(".credentials.json"), "").unwrap();
        let mut found = Vec::new();
        jsonl_files(dir.path(), None, &mut found);
        assert_eq!(found, [nested.join("s.jsonl")]);
    }

    /// A Muse transcript entry as Muse writes it: microsecond `recorded_at`, an
    /// inclusive usage block, and either a model name or a model object.
    fn muse_entry(id: &str, recorded_at: u64, event: Value) -> Value {
        json!({
            "schema_version": 1,
            "record_type": "event",
            "payload_type": "runtime.session",
            "payload_schema_version": 1,
            "id": id,
            "recorded_at": recorded_at,
            "stream": { "kind": "session", "id": "session-1" },
            "payload": { "source_run_record_id": "inference-1", "event": event },
        })
    }

    fn muse_rows(entries: &[Value]) -> Vec<Inference> {
        let text = entries
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        scan_muse(std::io::Cursor::new(text.into_bytes()))
    }

    #[test]
    fn muse_counts_a_finished_inference_once_with_its_inclusive_counters() {
        // The microsecond stamp is T3's own usage fixture for this record shape.
        let rows = muse_rows(&[
            muse_entry(
                "envelope-1",
                1_785_578_400_123_456,
                json!({
                    "kind": "model_completed",
                    "model": "muse-wonder-full-1.0",
                    "usage": {
                        "input_tokens": 100,
                        "cached_tokens": 60,
                        "cache_read_tokens": 60,
                        "cache_write_tokens": 10,
                        "output_tokens": 20,
                        "reasoning_tokens": 8,
                    },
                }),
            ),
            muse_entry(
                "envelope-2",
                1_785_578_400_123_456,
                json!({
                    "kind": "automated_review_completed",
                    "model": { "model_id": "review-model", "provider_id": "meta" },
                    "usage": { "input_tokens": 100, "cached_input_tokens": 60, "output_tokens": 20 },
                }),
            ),
        ]);
        assert_eq!(
            rows[0],
            (
                "inference-1".to_owned(),
                "2026-08-01".to_owned(),
                "muse-wonder-full-1.0".to_owned(),
                Tokens {
                    input: 30,
                    output: 20,
                    cache_read: 60,
                    cache_write: 10,
                },
            )
        );
        assert_eq!(rows[1].2, "review-model");
        assert_eq!(
            rows[1].3,
            Tokens {
                input: 40,
                output: 20,
                cache_read: 60,
                cache_write: 0,
            }
        );
    }

    #[test]
    fn muse_ignores_lines_that_are_not_a_supported_finished_inference() {
        let event =
            |kind: &str, usage: Value| json!({ "kind": kind, "model": "m", "usage": usage });
        let mut entries = vec![
            muse_entry(
                "kept",
                1_785_578_400_000_000,
                event("model_completed", json!({ "input_tokens": 5 })),
            ),
            // Not a record at all, or not the shape a token total comes from.
            json!({ "record_type": "event" }),
            json!("a string is not a record"),
            muse_entry(
                "x",
                1_785_578_400_000_000,
                event("workflow_completed", json!({ "input_tokens": 5 })),
            ),
            muse_entry(
                "x",
                1_785_578_400_000_000,
                event("usage_summary", json!({ "input_tokens": 5 })),
            ),
            // A record whose counters cannot prove any spend.
            muse_entry(
                "x",
                1_785_578_400_000_000,
                event("model_completed", json!({})),
            ),
            muse_entry(
                "x",
                1_785_578_400_000_000,
                event(
                    "model_completed",
                    json!({ "input_tokens": -5, "output_tokens": "20" }),
                ),
            ),
        ];
        let mut unsupported = muse_entry(
            "x",
            1_785_578_400_000_000,
            event("model_completed", json!({ "input_tokens": 5 })),
        );
        unsupported["schema_version"] = json!(2);
        entries.push(unsupported);
        let mut wrong_stream = muse_entry(
            "x",
            1_785_578_400_000_000,
            event("model_completed", json!({ "input_tokens": 5 })),
        );
        wrong_stream["stream"]["kind"] = json!("run");
        entries.push(wrong_stream);
        let mut no_model = muse_entry(
            "x",
            1_785_578_400_000_000,
            event("model_completed", json!({ "input_tokens": 5 })),
        );
        no_model["payload"]["event"]
            .as_object_mut()
            .unwrap()
            .remove("model");
        entries.push(no_model);

        let rows = muse_rows(&entries);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].0, "inference-1");
        assert_eq!(rows[0].1, "2026-08-01");
        assert_eq!(rows[0].2, "m");
        assert_eq!(rows[0].3.input, 5);
    }

    #[test]
    fn muse_counts_a_copied_inference_once_across_session_logs() {
        let row = (
            "inference-1".to_owned(),
            "2026-08-01".to_owned(),
            "m".to_owned(),
            Tokens {
                input: 5,
                ..Tokens::default()
            },
        );
        let mut combined = FileTotals::new();
        let mut counted = HashSet::new();
        Scan::Inferences(vec![row.clone()]).add(&mut combined, &mut counted);
        Scan::Inferences(vec![row]).add(&mut combined, &mut counted);
        assert_eq!(only(&combined).input, 5);
        assert!(combined.contains_key(&(
            "2026-08-01".to_owned(),
            "m".to_owned(),
            STANDARD_TIER.to_owned()
        )));
    }

    #[test]
    fn muse_reads_only_session_logs_and_a_missing_home_reports_nothing() {
        let dir = crate::testing::TempDir::new();
        let nested = dir.path().join("2026/08/01/session-1");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("session.jsonl"), "{not json\n").unwrap();
        std::fs::write(nested.join("telemetry.jsonl"), "").unwrap();
        let mut found = Vec::new();
        jsonl_files(dir.path(), Some("session.jsonl"), &mut found);
        assert_eq!(found, [nested.join("session.jsonl")]);
        assert!(
            scan_muse(
                std::fs::File::open(&found[0])
                    .map(std::io::BufReader::new)
                    .unwrap()
            )
            .is_empty(),
            "a malformed log is not a panic and not usage"
        );
        let mut missing = Vec::new();
        jsonl_files(
            &dir.path().join("absent"),
            Some("session.jsonl"),
            &mut missing,
        );
        assert!(missing.is_empty());
    }

    #[test]
    fn a_muse_inference_with_no_day_is_dropped_rather_than_dated() {
        assert_eq!(
            day_of_micros(1_785_578_400_123_456).as_deref(),
            Some("2026-08-01")
        );
        assert_eq!(day_of_micros(0).as_deref(), Some("1970-01-01"));
        assert_eq!(day_of_micros(u64::MAX), None);
    }

    #[test]
    fn the_host_id_is_stable() {
        let dir = crate::testing::TempDir::new();
        let first = UsageCenter::new(dir.path()).unwrap().host_id;
        assert_eq!(UsageCenter::new(dir.path()).unwrap().host_id, first);
    }

    #[tokio::test]
    async fn a_warm_up_stopped_by_quit_returns_and_leaves_the_cache_free() {
        let dir = crate::testing::TempDir::new();
        let center = UsageCenter::new(dir.path()).unwrap();
        let stop = tokio_util::sync::CancellationToken::new();
        stop.cancel();
        tokio::time::timeout(Duration::from_secs(10), center.warm(stop))
            .await
            .expect("a cancelled warm-up ends at once");
        // A request that arrives afterwards is not left waiting on the lock.
        assert!(center.cache.try_lock().is_ok());
    }
}
