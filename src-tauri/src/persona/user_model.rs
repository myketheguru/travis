//! Derived user-activity model (BRAIN.md capability #3a).
//!
//! Background pass that summarises the user's journal-capture patterns
//! into a structured JSON blob persisted on `user_profile`. The persona
//! block reads it on every prompt so Travis adapts timing + length
//! without being told.
//!
//! Cost shape: one aggregate SQL query over a recent window
//! (default 30 days). Runs daily; cheap on any realistic capture
//! volume.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

const LOOKBACK_DAYS: i64 = 30;

/// The shape persisted in `user_profile.derived_model_json`. Stable
/// enough to read across versions; new fields land additively.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UserModel {
    /// How far back the metrics look (days).
    pub window_days: i64,
    /// Captures in window.
    pub capture_count: i64,
    /// Average captures per active day (days with at least one capture).
    pub captures_per_active_day: f64,
    /// Median word count per capture.
    pub median_words: i64,
    /// Hour-of-day histogram (0-23) of capture times — the typical
    /// "when does the user capture" curve.
    pub active_hours: Vec<i64>,
    /// Two-or-three top hours derived from active_hours, formatted as
    /// "9am-11am" or "4pm" for prompt consumption.
    pub peak_window: String,
    /// Ratio of question-shaped captures (contain '?' or start with
    /// who/what/when/where/why/how) to total. 0.0–1.0.
    pub question_ratio: f64,
    /// Most-recent capture timestamp.
    pub latest_capture: Option<String>,
}

/// Run the derivation pass and write the result to user_profile.
/// Returns the new model (or None if there's no data yet).
pub async fn refresh(pool: &SqlitePool) -> anyhow::Result<Option<UserModel>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        created_at: String,
        raw: String,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT created_at, raw FROM journal_entry
         WHERE datetime(created_at) >= datetime('now', ?1)
         ORDER BY created_at ASC",
    )
    .bind(format!("-{LOOKBACK_DAYS} day"))
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(None);
    }

    // ----- word count distribution -----
    let mut word_counts: Vec<i64> = rows
        .iter()
        .map(|r| r.raw.split_whitespace().count() as i64)
        .collect();
    word_counts.sort_unstable();
    let median_words = if word_counts.is_empty() {
        0
    } else {
        word_counts[word_counts.len() / 2]
    };

    // ----- hour histogram -----
    let mut active_hours: Vec<i64> = vec![0; 24];
    let mut active_days: std::collections::HashSet<String> = Default::default();
    for r in &rows {
        if let Some(hour) = parse_hour_from_iso(&r.created_at) {
            if hour < 24 {
                active_hours[hour as usize] += 1;
            }
        }
        if r.created_at.len() >= 10 {
            active_days.insert(r.created_at[..10].to_string());
        }
    }

    let captures_per_active_day = if active_days.is_empty() {
        0.0
    } else {
        rows.len() as f64 / active_days.len() as f64
    };

    // ----- peak window: pick top 2 contiguous hour-clusters above mean -----
    let peak_window = derive_peak_window(&active_hours);

    // ----- question ratio -----
    let q_words = ["who", "what", "when", "where", "why", "how", "did", "can", "should"];
    let q_count: i64 = rows
        .iter()
        .map(|r| {
            let trimmed = r.raw.trim_start().to_lowercase();
            let has_qmark = trimmed.contains('?');
            let starts_q = q_words.iter().any(|w| {
                trimmed.starts_with(&format!("{w} ")) || trimmed == *w
            });
            if has_qmark || starts_q { 1 } else { 0 }
        })
        .sum();
    let question_ratio = q_count as f64 / rows.len() as f64;

    let latest_capture = rows.last().map(|r| r.created_at.clone());

    let model = UserModel {
        window_days: LOOKBACK_DAYS,
        capture_count: rows.len() as i64,
        captures_per_active_day,
        median_words,
        active_hours,
        peak_window,
        question_ratio,
        latest_capture,
    };

    let json = serde_json::to_string(&model)?;
    sqlx::query(
        "UPDATE user_profile
         SET derived_model_json = ?1,
             derived_model_at = CURRENT_TIMESTAMP,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = 1",
    )
    .bind(&json)
    .execute(pool)
    .await?;

    Ok(Some(model))
}

fn parse_hour_from_iso(s: &str) -> Option<i64> {
    // Accept "YYYY-MM-DD HH:MM:SS" or "YYYY-MM-DDTHH:MM:SS".
    let bytes = s.as_bytes();
    if bytes.len() < 13 {
        return None;
    }
    let sep = bytes.get(10).copied()?;
    if sep != b' ' && sep != b'T' {
        return None;
    }
    let h: i64 = std::str::from_utf8(&bytes[11..13]).ok()?.parse().ok()?;
    Some(h)
}

/// Render a "9am-11am, 4pm-6pm"-style peak window from the hourly
/// histogram. Picks the top contiguous clusters whose count is at
/// least 1.5× the mean. Capped at 2 windows so the prompt stays
/// terse.
fn derive_peak_window(hours: &[i64]) -> String {
    if hours.is_empty() {
        return String::new();
    }
    let total: i64 = hours.iter().sum();
    if total == 0 {
        return String::new();
    }
    let mean = total as f64 / hours.len() as f64;
    let threshold = (mean * 1.5).ceil() as i64;
    let mut windows: Vec<(usize, usize, i64)> = Vec::new();
    let mut i = 0;
    while i < hours.len() {
        if hours[i] >= threshold {
            let start = i;
            let mut sum = 0i64;
            while i < hours.len() && hours[i] >= threshold {
                sum += hours[i];
                i += 1;
            }
            windows.push((start, i - 1, sum));
        } else {
            i += 1;
        }
    }
    windows.sort_by(|a, b| b.2.cmp(&a.2));
    windows.truncate(2);
    windows.sort_by(|a, b| a.0.cmp(&b.0));
    windows
        .into_iter()
        .map(|(s, e, _)| {
            if s == e {
                fmt_hour(s as i64)
            } else {
                format!("{}–{}", fmt_hour(s as i64), fmt_hour((e + 1) as i64))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn fmt_hour(h: i64) -> String {
    let h = h % 24;
    match h {
        0 => "12am".into(),
        12 => "12pm".into(),
        1..=11 => format!("{h}am"),
        13..=23 => format!("{}pm", h - 12),
        _ => format!("{h}h"),
    }
}

/// Render a compact prompt block describing the user's patterns.
/// Empty string when no model has been derived yet.
pub fn format_for_prompt(model: &UserModel) -> String {
    if model.capture_count == 0 {
        return String::new();
    }
    let mut s = String::from("USER PATTERNS (derived from recent activity — adapt timing and length without commenting on it):\n");
    if !model.peak_window.is_empty() {
        s.push_str(&format!("- Most active: {}\n", model.peak_window));
    }
    s.push_str(&format!(
        "- Typical capture length: ~{} word{}\n",
        model.median_words,
        if model.median_words == 1 { "" } else { "s" }
    ));
    s.push_str(&format!(
        "- Capture cadence: {:.1}/active day ({} captures in {} days)\n",
        model.captures_per_active_day, model.capture_count, model.window_days
    ));
    let q_pct = (model.question_ratio * 100.0).round() as i64;
    s.push_str(&format!(
        "- Question-shaped turns: ~{q_pct}% (the rest are captures)\n"
    ));
    s
}

/// Parse the JSON blob from user_profile. Returns None for null or
/// invalid JSON — caller treats absence as "no model yet".
pub fn parse(json: &str) -> Option<UserModel> {
    serde_json::from_str(json).ok()
}
