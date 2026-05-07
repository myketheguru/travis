use serde::Serialize;
use tauri::State;

use crate::conversation::{self, ConversationMessage};
use crate::domain::task::{self, TaskFilter};
use crate::llm::{self, ChatOptions, Message, Role};
use crate::memory::{self, MemoryHit};
use crate::secrets;
use crate::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskResponse {
    pub conversation_id: i64,
    pub answer: String,
    pub messages: Vec<ConversationMessage>,
    pub sources: Vec<MemoryHit>,
    pub model: String,
}

/// Bulk (re)index every journal entry's raw_text. Idempotent — replaces existing rows.
#[tauri::command]
pub async fn index_all_journal_entries(state: State<'_, AppState>) -> Result<usize, String> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, raw_text FROM journal_entry ORDER BY id ASC")
            .fetch_all(&state.db.pool)
            .await
            .map_err(|e| e.to_string())?;

    let mut count = 0usize;
    for (id, raw) in rows {
        if raw.trim().is_empty() {
            continue;
        }
        match memory::index_journal_entry(&state.db.pool, id, &raw).await {
            Ok(()) => count += 1,
            Err(e) => {
                tracing::warn!("index_all: failed for journal#{id}: {e}");
            }
        }
    }
    Ok(count)
}

/// Extract loose entity-like tokens from a question: capitalized words / multi-word names.
/// Filters out a small set of sentence starters.
fn extract_entities(question: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    let stop_starters: &[&str] = &[
        "What", "Who", "When", "Where", "Why", "How", "Did", "Do", "Does", "Is", "Are", "Was",
        "Were", "Will", "Would", "Should", "Could", "Can", "The", "A", "An", "I", "My", "Me",
        "Last", "Next", "This", "That", "These", "Those", "Today", "Yesterday", "Tomorrow",
    ];

    let is_cap_word = |w: &str| -> bool {
        let mut chars = w.chars();
        match chars.next() {
            Some(c) if c.is_ascii_uppercase() => w.chars().all(|c| c.is_ascii_alphanumeric()),
            _ => false,
        }
    };

    let mut tokens: Vec<&str> = Vec::new();
    for raw_tok in question.split_whitespace() {
        // Strip surrounding punctuation but keep internal hyphens/apostrophes minimal.
        let trimmed = raw_tok.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        if !trimmed.is_empty() {
            tokens.push(trimmed);
        }
    }

    for tok in tokens {
        if is_cap_word(tok) && !stop_starters.contains(&tok) {
            current.push(tok);
        } else {
            if !current.is_empty() {
                out.push(current.join(" "));
                current.clear();
            }
        }
    }
    if !current.is_empty() {
        out.push(current.join(" "));
    }
    // Dedup case-insensitively while preserving order.
    let mut seen: Vec<String> = Vec::new();
    let mut deduped: Vec<String> = Vec::new();
    for e in out {
        let key = e.to_lowercase();
        if !seen.contains(&key) {
            seen.push(key);
            deduped.push(e);
        }
    }
    deduped
}

fn format_hits(hits: &[MemoryHit]) -> String {
    if hits.is_empty() {
        return "(no relevant notes found)".into();
    }
    let mut s = String::new();
    for (i, h) in hits.iter().enumerate() {
        let preview: String = h.text.chars().take(500).collect();
        s.push_str(&format!(
            "{n}. [{kind}#{id} @ {at}] {text}\n",
            n = i + 1,
            kind = h.kind,
            id = h.source_id,
            at = h.created_at,
            text = preview
        ));
    }
    s
}

fn format_tasks(tasks: &[crate::domain::task::Task]) -> String {
    if tasks.is_empty() {
        return "(no open tasks)".into();
    }
    let mut s = String::new();
    for t in tasks {
        let due = t.due_at.as_deref().unwrap_or("no due");
        s.push_str(&format!(
            "- #{id} [p{pri} due:{due}] {title}\n",
            id = t.id,
            pri = t.priority,
            due = due,
            title = t.title
        ));
    }
    s
}

#[tauri::command]
pub async fn ask_travis(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    question: String,
    conversation_id: Option<i64>,
) -> Result<AskResponse, String> {
    let q = question.trim().to_string();
    if q.is_empty() {
        return Err("empty question".into());
    }

    // Open a fresh QA conversation or continue the one the user already has open.
    let conv_id = match conversation_id {
        Some(cid) => {
            let existing = conversation::fetch(&state.db.pool, cid)
                .await
                .map_err(|e| e.to_string())?;
            if existing.status == "resolved" {
                let title = q.chars().take(60).collect::<String>();
                conversation::open(&state.db.pool, "qa", Some(&title))
                    .await
                    .map_err(|e| e.to_string())?
                    .id
            } else {
                cid
            }
        }
        None => {
            let title = q.chars().take(60).collect::<String>();
            conversation::open(&state.db.pool, "qa", Some(&title))
                .await
                .map_err(|e| e.to_string())?
                .id
        }
    };

    // Append the user's question to the thread before we run retrieval, so the
    // history we send to the LLM includes their latest turn.
    let _ = conversation::append(&state.db.pool, conv_id, "user", &q, None).await;

    // Retrieval is grounded in the LATEST question only — prior turns provide
    // conversational context, fresh retrieval picks the most relevant snippets.
    let entities = extract_entities(&q);
    let hits = memory::retrieve(&state.db.pool, &q, &entities, 5)
        .await
        .map_err(|e| e.to_string())?;

    let open_tasks = task::list(
        &state.db.pool,
        TaskFilter {
            status: Some("open".into()),
            ..Default::default()
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    let open_tasks: Vec<_> = open_tasks.into_iter().take(10).collect();

    let context = format!(
        "## Memory (relevant snippets)\n{memory}\n\n## Open tasks\n{tasks}",
        memory = format_hits(&hits),
        tasks = format_tasks(&open_tasks),
    );

    let profile = state
        .db
        .user_profile()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no user profile yet".to_string())?;

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
    )
    .map_err(|e| e.to_string())?;

    let first = profile.first_name();
    let mut system = format!(
        "You are Travis, a personal operations assistant.\n\n{user_context}\n\n\
This is a continuous chat — you have prior turns of context above the current question. \
Answer grounded in the supplied retrieval context (memory snippets + open tasks) when relevant. \
If the context doesn't have enough info, say so plainly. \
Be conversational but concise — short paragraphs or bullets, not essays. \
\n\nIf {first} asks for something operational (set a reminder, draft an invoice, send an email), \
acknowledge that this Ask surface is for retrieval/conversation. The Cmd/Ctrl+J overlay is where \
ops capture happens. Offer to capture the intent there if it's something Travis CAN do; if a \
capability isn't connected yet (e.g. email when no Gmail/Outlook is linked), say so honestly and \
offer to log it.",
        user_context = profile.context_block(),
        first = first,
    );

    // Append vertical-pack guidance (PACKS_AUDIT.md step 10).
    let pack_fragment = crate::packs::prompt_fragment();
    if !pack_fragment.is_empty() {
        system.push_str("\n\n");
        system.push_str(&pack_fragment);
    }

    // Build history from conversation_message — drop the just-appended user
    // message because we send the contextualized version below.
    let prior = conversation::messages(&state.db.pool, conv_id, Some(20))
        .await
        .map_err(|e| e.to_string())?;
    let mut messages: Vec<Message> = Vec::new();
    let take = prior.len().saturating_sub(1);
    for m in prior.iter().take(take) {
        let role = match m.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            _ => continue,
        };
        messages.push(Message {
            role,
            content: m.content.clone(),
            ..Default::default()
        });
    }
    let user_msg = format!("{context}\n\nQuestion: {q}");
    messages.push(Message::user(user_msg));

    let resp = match provider
        .chat(
            messages,
            ChatOptions {
                system: Some(system),
                cache_system: true,
                json_mode: false,
                temperature: Some(0.4),
                max_tokens: Some(1024),
            },
        )
        .await
    {
        Ok(r) => {
            state.health.clear(&app);
            r
        }
        Err(e) => {
            let msg = e.to_string();
            let kind = crate::health::classify_llm_error(&msg);
            state.health.report(&app, kind, format!("Ask Travis failed: {msg}"));
            return Err(msg);
        }
    };

    // Persist the assistant reply with the source list as audit payload.
    let payload = serde_json::json!({
        "kind": "qa_answer",
        "model": resp.model,
        "sources": hits,
    })
    .to_string();
    let _ = conversation::append(
        &state.db.pool,
        conv_id,
        "assistant",
        &resp.content,
        Some(&payload),
    )
    .await;
    let _ = conversation::set_status(&state.db.pool, conv_id, "open").await;

    let messages = conversation::messages(&state.db.pool, conv_id, None)
        .await
        .map_err(|e| e.to_string())?;

    Ok(AskResponse {
        conversation_id: conv_id,
        answer: resp.content,
        messages,
        sources: hits,
        model: resp.model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_capitalized_entities() {
        let ents = extract_entities("Did I invoice John Smith from PS 142 last week?");
        assert!(ents.iter().any(|e| e == "John Smith"));
        assert!(ents.iter().any(|e| e == "PS"));
    }

    #[test]
    fn skips_leading_question_word() {
        let ents = extract_entities("What did Maria say about the Department?");
        assert!(!ents.iter().any(|e| e == "What"));
        assert!(ents.iter().any(|e| e == "Maria"));
        assert!(ents.iter().any(|e| e == "Department"));
    }
}
