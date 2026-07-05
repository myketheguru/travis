//! T2T auto-draft — task 311 slice B.
//!
//! When another Travis sends this user a query, the query lands in
//! their inbox in the `pending` state with no draft response. This
//! command uses the desktop's local LLM to draft a reply, then POSTs
//! it back via `t2t_draft_reply` so the user sees a review-ready
//! draft the next time they open the T2tConvoCard.
//!
//! The frontend polls the inbox periodically (see useAttentionItems)
//! and calls this command for each pending query one time. Idempotent
//! at the cloud layer — a re-draft just overwrites the previous.

use tauri::State;

use crate::cloud::t2t;
use crate::llm::{self, ChatOptions, Message};
use crate::secrets;
use crate::AppState;

const DRAFT_SYSTEM_PROMPT: &str = r#"
You are Travis, replying on the user's behalf to a question that came
in from another user's Travis via Travis-to-Travis (T2T).

Rules:
- Keep the reply short — 1 to 3 sentences unless the question demands
  more. Users skim these; long replies waste their time.
- Answer directly. If you can, use what you know about the user's
  work + memory to give a specific answer.
- If you genuinely don't know or don't have permission to share
  something, say so briefly. Do not fabricate.
- Speak in the user's voice — first person, warm, direct.
- Do not sign the reply. Do not use greetings ("Hi", "Hey"). Just the
  reply content.
- This is a DRAFT — the user will review and can edit before sending.

Output: just the reply text. No preamble, no wrapping, no markdown.
"#;

#[tauri::command]
pub async fn t2t_auto_draft(
    state: State<'_, AppState>,
    query_id: String,
) -> Result<String, String> {
    auto_draft_inner(&state, &query_id)
        .await
        .map_err(|e| e.to_string())
}

async fn auto_draft_inner(state: &AppState, query_id: &str) -> anyhow::Result<String> {
    // Fetch the query from inbox — we filter by id so we know it's
    // one addressed to this user.
    let inbox = t2t::inbox(&state.http).await?;
    let q = inbox
        .into_iter()
        .find(|q| q.id == query_id)
        .ok_or_else(|| anyhow::anyhow!("query {query_id} not in inbox"))?;

    // Bail if there's already a draft — don't overwrite the user's
    // in-progress edit.
    if q.drafted_response.as_ref().is_some_and(|s| !s.trim().is_empty()) {
        return Ok(q.drafted_response.unwrap_or_default());
    }

    // Build a provider. Uses whatever the user has configured — Travis
    // Cloud for hosted users, Claude/OpenAI/Ollama for BYOK.
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

    // Format the incoming context for the LLM.
    let asker = q
        .from_name
        .as_deref()
        .or(q.from_email.as_deref())
        .unwrap_or("another Travis");

    let user_msg = format!("{asker} asked (via T2T):\n\n{}", q.question);

    let resp = provider
        .chat(
            vec![Message::user(user_msg)],
            ChatOptions {
                system: Some(DRAFT_SYSTEM_PROMPT.to_string()),
                cache_system: true,
                cache_conversation: false,
                json_mode: false,
                temperature: Some(0.4),
                max_tokens: Some(300),
            },
        )
        .await?;

    let draft = resp.content.trim().to_string();
    if draft.is_empty() {
        anyhow::bail!("LLM returned an empty draft");
    }

    // Post the draft back to the cloud — this moves the query state
    // to `drafted` and surfaces in the recipient's attention strip.
    t2t::draft_reply(&state.http, query_id, &draft).await?;
    Ok(draft)
}
