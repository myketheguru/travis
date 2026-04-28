# 🧠 MEMORY & CONTEXT SYSTEM PRD

*(Travis Cognitive Layer)*

---

# 1. 🎯 Goal

Enable Travis to:

* Remember user actions, notes, and patterns **indefinitely**
* Retrieve **relevant context instantly**
* Maintain **situational awareness across time**
* Improve responses with **historical + behavioral understanding**

---

# 2. 🧩 Core Principle

> Not all memory is equal.

You need **different memory types**, each optimized for a purpose.

---

# 3. 🧠 Memory Architecture (4 Layers)

---

## 3.1 🟢 Layer 1 — Working Memory (Short-Term)

### Purpose

* Current conversation/session awareness

### Storage

* In-memory (or Redis)

### TTL

* Session-based

### Contents

* Last N messages
* Current task
* Active entities

---

## 3.2 🔵 Layer 2 — Structured Memory (Source of Truth)

### Purpose

* Reliable, queryable data

### Storage

* SQLite (local) or PostgreSQL (remote)

### Examples

* Tasks
* Invoices
* Coach hours
* Reminders

---

👉 This is what prevents hallucination

---

## 3.3 🟣 Layer 3 — Semantic Memory (Vector DB)

### Purpose

* Long-term recall of notes, conversations, journal entries

### Storage

* Local vector DB:

  * SQLite + embeddings OR
  * something like LanceDB / Chroma

---

### Stored Data

```json
{
  "text": "Had session with John...",
  "embedding": [...],
  "metadata": {
    "type": "journal",
    "date": "2026-04-26",
    "entities": ["John"]
  }
}
```

---

## 3.4 🟡 Layer 4 — Behavioral Memory (Patterns)

### Purpose

* Learn habits

### Storage

* Event logs + aggregation

### Examples

* Logs hours every Friday
* Creates invoices after sessions

---

# 4. 🔄 Memory Ingestion Pipeline

---

## Step 1: Capture

From:

* Journal input
* Commands
* System actions

---

## Step 2: Process (LLM-assisted)

Extract:

* entities
* intent
* tasks
* timestamps

---

## Step 3: Store

* Raw → Semantic memory
* Structured → DB
* Events → Event log

---

## Step 4: Index

* Generate embeddings
* Store for retrieval

---

# 5. 🔍 Context Retrieval Strategy (CRITICAL)

---

## Problem

LLMs cannot read “everything”

---

## Solution

> **Retrieve only what matters**

---

## Retrieval Pipeline

### Step 1: Intent detection

User:

> “Did I invoice John last week?”

---

### Step 2: Multi-source retrieval

#### Structured DB

* invoices where name = John

#### Semantic search

* notes mentioning John last week

#### Event logs

* actions taken

---

### Step 3: Context assembly

```json
{
  "structured": [...],
  "notes": [...],
  "recent_actions": [...]
}
```

---

### Step 4: Feed to LLM

👉 Only relevant context goes in

---

# 6. 🧠 Context Prioritization Algorithm

Not all data is equal.

---

## Ranking factors

1. Recency
2. Relevance (embedding similarity)
3. Entity match
4. Task linkage

---

## Example scoring

```text
score = (0.4 * similarity) + (0.3 * recency) + (0.3 * entity_match)
```

---

# 7. 🧩 Memory Compression (VERY IMPORTANT)

Without this, system breaks over time.

---

## Strategy

### Summarization layers

#### Daily summary

* “What happened today”

#### Weekly summary

* patterns, key events

---

### Replace:

❌ thousands of raw entries

With:
✅ summarized + indexed chunks

---

# 8. 🧠 Persistent Identity Model

Travis should maintain a model of the user:

---

## Stored Profile

```json
{
  "preferences": {
    "work_style": "batching",
    "reminder_style": "gentle"
  },
  "habits": [
    "logs hours Friday",
    "creates invoices weekly"
  ],
  "entities": {
    "John": "Coach",
    "Dept": "Department of Education"
  }
}
```

---

👉 This enables:

* personalization
* better suggestions

---

# 9. 🔁 Continuous Learning Loop

---

## Observe

Track:

* actions
* sequences
* timing

---

## Detect patterns

Example:

```text
A → B → C repeated 5 times
```

---

## Suggest automation

```text
“Automate this workflow?”
```

---

## Store as rule

---

# 10. ⚡ Performance Strategy

---

## Local-first retrieval

* semantic DB local
* structured DB local

---

## LLM usage only when needed

* parsing
* reasoning

---

## Caching

* cache embeddings
* cache frequent queries

---

# 11. 🔐 Privacy Strategy

---

## Sensitive data stays local

* journal
* financial data

---

## Optional cloud usage

* only for reasoning

---

## Future option

* fully local LLM fallback

---

# 12. 🧪 Failure Handling

---

## If retrieval fails

* fallback to structured DB
* ask clarification

---

## If ambiguity

Travis:

> “Do you mean John the coach?”

---

# 13. 🧠 Example End-to-End

---

User:

> “What did I say about invoices last week?”

---

System:

1. Semantic search → journal
2. Structured DB → invoices
3. Events → actions
4. Summaries → weekly summary

---

Response:

```text
Last week:

• Planned to create 3 invoices
• Created 2
• One (Coach John) still pending
```

---

# 14. 🏗️ Tech Stack

---

## Storage

* SQLite (structured)
* Vector DB (Chroma / LanceDB)

---

## Embeddings

* OpenAI / local embedding model

---

## Processing

* Background workers

---

# 15. 🚀 MVP Scope (keep it realistic)

---

## MUST

* Semantic memory (journal)
* Structured DB
* Basic retrieval
* Context injection

---

## LATER

* pattern detection
* summarization layers
* identity model

---

# ⚠️ Final Reality Check

You don’t get:

> “perfect memory + perfect understanding”

You get:

> **relevant recall + useful context at the right time**

That’s what feels like intelligence.

---

