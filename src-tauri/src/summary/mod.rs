use serde::Serialize;
use sqlx::SqlitePool;

use crate::db::UserProfile;
use crate::llm::{self, ChatOptions, Message};
use crate::secrets;
use crate::AppState;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: i64,
    pub kind: String,
    pub period_start: String,
    pub period_end: String,
    pub content: String,
    pub source_count: i64,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub created_at: String,
}

fn daily_system_prompt(profile: &UserProfile) -> String {
    let mut prompt = format!(
        "You generate brief operational summaries for the user.\n\n{}\n\nOutput 2-4 sentences. No headers. Focus on what got done and what's outstanding.",
        profile.context_block(),
    );
    let pack_fragment = crate::packs::prompt_fragment();
    if !pack_fragment.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(&pack_fragment);
    }
    prompt
}

fn weekly_system_prompt(profile: &UserProfile) -> String {
    let mut prompt = format!(
        "You generate brief operational weekly summaries for the user.\n\n{}\n\nIdentify 1-2 patterns or recurring themes from the week. Output 4-6 sentences. No headers. Focus on what got done, what's outstanding, and patterns.",
        profile.context_block(),
    );
    let pack_fragment = crate::packs::prompt_fragment();
    if !pack_fragment.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(&pack_fragment);
    }
    prompt
}

pub async fn list(
    pool: &SqlitePool,
    kind: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<Summary>> {
    let limit = limit.clamp(1, 500);
    let rows = sqlx::query_as::<_, Summary>(
        "SELECT id, kind, period_start, period_end, content, source_count, model, provider, created_at
         FROM summary
         WHERE (?1 IS NULL OR kind = ?1)
         ORDER BY period_end DESC, id DESC
         LIMIT ?2",
    )
    .bind(kind)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, Default)]
struct DayContext {
    journal_entries: Vec<String>,
    completed_tasks: Vec<String>,
    coach_hours: Vec<String>,
}

impl DayContext {
    fn is_empty(&self) -> bool {
        self.journal_entries.is_empty()
            && self.completed_tasks.is_empty()
            && self.coach_hours.is_empty()
    }

    fn source_count(&self) -> i64 {
        (self.journal_entries.len() + self.completed_tasks.len() + self.coach_hours.len()) as i64
    }

    fn render(&self, label: &str) -> String {
        let mut out = format!("Context for {label}:\n");
        if !self.journal_entries.is_empty() {
            out.push_str("\nJournal entries:\n");
            for (i, j) in self.journal_entries.iter().enumerate() {
                out.push_str(&format!("- ({}) {}\n", i + 1, j));
            }
        }
        if !self.completed_tasks.is_empty() {
            out.push_str("\nCompleted tasks:\n");
            for t in &self.completed_tasks {
                out.push_str(&format!("- {t}\n"));
            }
        }
        if !self.coach_hours.is_empty() {
            out.push_str("\nCoach hours logged:\n");
            for h in &self.coach_hours {
                out.push_str(&format!("- {h}\n"));
            }
        }
        out
    }
}

async fn collect_day(pool: &SqlitePool, date: &str) -> anyhow::Result<DayContext> {
    let pattern = format!("{date}%");

    let journal_rows: Vec<(String,)> = sqlx::query_as(
        "SELECT raw_text FROM journal_entry WHERE created_at LIKE ?1 ORDER BY id ASC",
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    let task_rows: Vec<(String,)> = sqlx::query_as(
        "SELECT title FROM task WHERE status = 'done' AND completed_at LIKE ?1 ORDER BY id ASC",
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    let hours_rows: Vec<(String, f64, Option<String>)> = sqlx::query_as(
        "SELECT COALESCE(c.name, '?'), ch.hours, ch.description
         FROM coach_hours ch
         LEFT JOIN coach c ON c.id = ch.coach_id
         WHERE ch.session_date = ?1
         ORDER BY ch.id ASC",
    )
    .bind(date)
    .fetch_all(pool)
    .await?;

    Ok(DayContext {
        journal_entries: journal_rows.into_iter().map(|(t,)| t).collect(),
        completed_tasks: task_rows.into_iter().map(|(t,)| t).collect(),
        coach_hours: hours_rows
            .into_iter()
            .map(|(name, hours, desc)| {
                let d = desc.unwrap_or_default();
                if d.is_empty() {
                    format!("{name}: {hours}h")
                } else {
                    format!("{name}: {hours}h — {d}")
                }
            })
            .collect(),
    })
}

async fn upsert_summary(
    pool: &SqlitePool,
    kind: &str,
    period_start: &str,
    period_end: &str,
    content: &str,
    source_count: i64,
    provider: &str,
    model: &str,
) -> anyhow::Result<Summary> {
    sqlx::query(
        "INSERT INTO summary (kind, period_start, period_end, content, source_count, model, provider)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(kind, period_start) DO UPDATE SET
            period_end = excluded.period_end,
            content = excluded.content,
            source_count = excluded.source_count,
            model = excluded.model,
            provider = excluded.provider,
            created_at = CURRENT_TIMESTAMP",
    )
    .bind(kind)
    .bind(period_start)
    .bind(period_end)
    .bind(content)
    .bind(source_count)
    .bind(model)
    .bind(provider)
    .execute(pool)
    .await?;

    let row = sqlx::query_as::<_, Summary>(
        "SELECT id, kind, period_start, period_end, content, source_count, model, provider, created_at
         FROM summary WHERE kind = ?1 AND period_start = ?2",
    )
    .bind(kind)
    .bind(period_start)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn generate_daily(state: &AppState, date: &str) -> anyhow::Result<Summary> {
    let day = collect_day(&state.db.pool, date).await?;
    if day.is_empty() {
        anyhow::bail!("nothing to summarize for {date}");
    }

    let profile = state
        .db
        .user_profile()
        .await?
        .ok_or_else(|| anyhow::anyhow!("no user profile yet"))?;

    let api_key = match profile.llm_provider.as_str() {
        "claude" | "openai" => secrets::get_api_key(&profile.llm_provider),
        _ => None,
    };

    let provider = llm::build(
        &profile.llm_provider,
        api_key.as_deref(),
        profile.ollama_url.as_deref(),
        profile.model.as_deref(),
        state.http.clone(),
    )?;

    let user_msg = day.render(date);

    let resp = provider
        .chat(
            vec![Message::user(user_msg)],
            ChatOptions {
                system: Some(daily_system_prompt(&profile)),
                cache_system: true,
                json_mode: false,
                temperature: Some(0.3),
                max_tokens: Some(400),
            },
        )
        .await?;

    let model_used = if resp.model.is_empty() {
        profile.model.clone().unwrap_or_else(|| {
            llm::default_model(&profile.llm_provider).to_string()
        })
    } else {
        resp.model.clone()
    };

    upsert_summary(
        &state.db.pool,
        "daily",
        date,
        date,
        resp.content.trim(),
        day.source_count(),
        &profile.llm_provider,
        &model_used,
    )
    .await
}

fn add_days_iso(date: &str, days: i64) -> Option<String> {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    let base = ymd_to_days(y, m, d);
    let next = base + days;
    let (y2, m2, d2) = days_to_ymd(next);
    Some(format!("{y2:04}-{m2:02}-{d2:02}"))
}

fn ymd_to_days(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = (y - era * 400) as i64;
    let m = m as i64;
    let d = d as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era as i64 * 146097 + doe - 719468
}

fn days_to_ymd(days_since_epoch: i64) -> (i32, u32, u32) {
    let mut days = days_since_epoch + 719468;
    let era = if days >= 0 { days / 146097 } else { (days - 146096) / 146097 };
    days -= era * 146097;
    let yoe = (days - days / 1460 + days / 36524 - days / 146096) / 365;
    let y = (yoe + era * 400) as i32;
    let doy = days - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

pub async fn generate_weekly(state: &AppState, week_start: &str) -> anyhow::Result<Summary> {
    let week_end = add_days_iso(week_start, 6)
        .ok_or_else(|| anyhow::anyhow!("invalid week_start: {week_start}"))?;

    let mut all_days: Vec<(String, DayContext)> = Vec::new();
    let mut total_sources = 0i64;
    for offset in 0..7 {
        let d = add_days_iso(week_start, offset)
            .ok_or_else(|| anyhow::anyhow!("invalid date offset"))?;
        let day = collect_day(&state.db.pool, &d).await?;
        total_sources += day.source_count();
        all_days.push((d, day));
    }

    if total_sources == 0 {
        anyhow::bail!("nothing to summarize for week starting {week_start}");
    }

    let profile = state
        .db
        .user_profile()
        .await?
        .ok_or_else(|| anyhow::anyhow!("no user profile yet"))?;

    let api_key = match profile.llm_provider.as_str() {
        "claude" | "openai" => secrets::get_api_key(&profile.llm_provider),
        _ => None,
    };

    let provider = llm::build(
        &profile.llm_provider,
        api_key.as_deref(),
        profile.ollama_url.as_deref(),
        profile.model.as_deref(),
        state.http.clone(),
    )?;

    let mut user_msg = format!("Week {week_start} to {week_end}:\n\n");
    for (label, day) in &all_days {
        if day.is_empty() {
            continue;
        }
        user_msg.push_str(&day.render(label));
        user_msg.push('\n');
    }

    let resp = provider
        .chat(
            vec![Message::user(user_msg)],
            ChatOptions {
                system: Some(weekly_system_prompt(&profile)),
                cache_system: true,
                json_mode: false,
                temperature: Some(0.3),
                max_tokens: Some(700),
            },
        )
        .await?;

    let model_used = if resp.model.is_empty() {
        profile.model.clone().unwrap_or_else(|| {
            llm::default_model(&profile.llm_provider).to_string()
        })
    } else {
        resp.model.clone()
    };

    upsert_summary(
        &state.db.pool,
        "weekly",
        week_start,
        &week_end,
        resp.content.trim(),
        total_sources,
        &profile.llm_provider,
        &model_used,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_arithmetic() {
        assert_eq!(add_days_iso("2026-04-27", 0).as_deref(), Some("2026-04-27"));
        assert_eq!(add_days_iso("2026-04-27", 6).as_deref(), Some("2026-05-03"));
        assert_eq!(add_days_iso("2026-12-30", 7).as_deref(), Some("2027-01-06"));
        assert_eq!(add_days_iso("2024-02-28", 1).as_deref(), Some("2024-02-29"));
    }
}
