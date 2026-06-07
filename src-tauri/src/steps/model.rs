//! Step event model — what the frontend listens for.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    ToolCall,
    Action,
    CodeExecution,
    Thinking,
    WorkflowOp,
}

impl StepKind {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            StepKind::ToolCall => "tool_call",
            StepKind::Action => "action",
            StepKind::CodeExecution => "code_execution",
            StepKind::Thinking => "thinking",
            StepKind::WorkflowOp => "workflow_op",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Running,
    Ok,
    Failed,
    Cancelled,
}

impl StepStatus {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            StepStatus::Running => "running",
            StepStatus::Ok => "ok",
            StepStatus::Failed => "failed",
            StepStatus::Cancelled => "cancelled",
        }
    }
}

/// Events emitted to the `step-event` Tauri channel.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum StepEvent {
    Started {
        step_id: String,
        parent_step_id: Option<String>,
        conversation_id: i64,
        kind: StepKind,
        name: String,
        detail: Option<String>,
        started_at: String,
    },
    Note {
        step_id: String,
        text: String,
    },
    Result {
        step_id: String,
        status: StepStatus,
        summary: Option<String>,
        error: Option<String>,
    },
    Completed {
        step_id: String,
        duration_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct StepRow {
    pub id: String,
    pub conversation_id: i64,
    pub parent_step_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub detail: Option<String>,
    pub status: String,
    pub summary: Option<String>,
    pub notes_json: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration_ms: Option<i64>,
}
