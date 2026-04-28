# 🧠 PRODUCT REQUIREMENTS DOCUMENT (PRD)

## Product Name

**Travis (Working Title)**

---

# 1. 📌 Overview

Travis is a **local-first AI operations assistant** designed to help a COO automate operational workflows, reduce cognitive load, and act as a **thinking + execution layer** across daily work.

It combines:

* Natural language interface (chat + command palette)
* Structured workflow automation (invoices, hours, compliance)
* Intelligent journaling (thought → task conversion)
* Reminder + nudge system
* OTA updates for continuous improvement

---

# 2. 🎯 Goals

### Primary Goals

* Reduce time spent on repetitive operational tasks by **≥50%**
* Ensure **zero missed compliance steps** (invoices, signing sheets)
* Provide a **single interface for thinking, planning, and execution**

### Secondary Goals

* Learn user behavior patterns over time
* Provide proactive suggestions
* Enable remote updates without friction

---

# 3. 👤 Target User

* COO / Operations Lead
* Handles:

  * Invoicing
  * Work orders
  * Coach coordination
  * Compliance submissions
  * Communication

---

# 4. 🧩 Core Features

---

## 4.1 Command Overlay (Primary Interface)

### Trigger

* Global shortcut: `Cmd + J`

### Capabilities

* Natural language commands
* Journal input
* Task execution
* Suggestions

### Requirements

* Opens in <200ms
* Auto-focus input
* ESC closes instantly

---

## 4.2 Journal System (Thinking Layer)

### Description

Free-form input system that converts thoughts into structured actions.

### Functional Requirements

* Accept unstructured input
* Store raw note
* Extract:

  * Tasks
  * Reminders
  * Entities

### Example

Input:

> “Need to follow up with Dept and invoice John”

Output:

* Task: Follow up with Dept
* Task: Create invoice for John

---

## 4.3 Task & Workflow System

### Entities

* Task
* Invoice
* SigningSheet
* CoachHours

### Workflow Rules

Invoice:

* Must have matching signing sheet
* Dates must align
* Coach hours must exist

---

## 4.4 Reminder System

### Types

#### 1. Time-based

* Scheduled notifications

#### 2. Context-based

* Triggered by missing actions

#### 3. Behavioral

* Based on patterns

---

## 4.5 Automation Engine

### Capabilities

* Rule-based automations
* Event-driven triggers

### Example

* After logging hours → suggest signing sheet
* After sheet → suggest invoice

---

## 4.6 Agent System

### Architecture

* Tool-based (no direct DB access)
* Uses orchestration (LangGraph optional)

### Agents

* Task Execution Agent
* Validation Agent
* Ops Assistant Agent
* Learning Agent

---

## 4.7 Mobile Companion

### Scope

* Chat-only interface

### Capabilities

* Log hours
* Set reminders
* View tasks

---

## 4.8 OTA Update System

### Requirements

* Background update checks
* Secure updates
* User-controlled installation

---

## 4.9 Feature Flag System

### Purpose

* Enable/disable features remotely
* Roll out changes safely

---

# 5. 🏗️ Technical Architecture

---

## 5.1 Frontend

* Tauri (Rust + WebView)
* React (UI)
* State: Zustand

---

## 5.2 Backend (Hybrid)

### Local

* SQLite (local storage)
* Background scheduler

### Remote

* API (FastAPI or NestJS)
* LLM integration
* Workflow logic (optional remote)

---

## 5.3 Agent Layer

* Tool-based execution
* Optional: LangGraph orchestration

---

## 5.4 Infrastructure

* GitHub (code + releases)
* GitHub Actions (CI/CD)
* GitHub Pages (update metadata)

---

# 6. 🔄 Data Flow

---

## Journal Flow

```text
User Input → LLM Parser → Structured Data → Stored + Actions Suggested
```

---

## Task Execution Flow

```text
User Command → Intent Parsing → Tool Execution → Validation → Response
```

---

## Update Flow

```text
App → Check JSON → Compare Version → Download → Install
```

---

# 7. ⚙️ GitHub Actions Pipeline (macOS Build)

---

## Requirements

* Build macOS `.app` or `.dmg`
* Attach to GitHub Release
* Generate updater artifacts

---

## Example Workflow

```yaml
name: Build Tauri App

on:
  push:
    tags:
      - 'v*'

jobs:
  build-macos:
    runs-on: macos-latest

    steps:
      - uses: actions/checkout@v3

      - name: Install Node
        uses: actions/setup-node@v3
        with:
          node-version: 18

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Install dependencies
        run: npm install

      - name: Build Tauri App
        run: npm run tauri build

      - name: Upload Release
        uses: softprops/action-gh-release@v1
        with:
          files: src-tauri/target/release/bundle/**/*
```

---

# 8. 🔄 Tauri Updater Setup (Step-by-Step)

---

## Step 1: Enable updater

`tauri.conf.json`

```json
{
  "updater": {
    "active": true,
    "endpoints": [
      "https://yourdomain.com/update.json"
    ]
  }
}
```

---

## Step 2: Generate keys

```bash
tauri signer generate
```

---

## Step 3: Sign builds

Tauri automatically signs during build.

---

## Step 4: Create update JSON

```json
{
  "version": "1.0.1",
  "notes": "Bug fixes",
  "pub_date": "2026-04-26T12:00:00Z",
  "platforms": {
    "darwin-x86_64": {
      "url": "https://github.com/.../app.dmg",
      "signature": "SIGNATURE"
    }
  }
}
```

---

## Step 5: Host JSON

* GitHub Pages (recommended)

---

## Step 6: App-side check

```javascript
import { checkUpdate } from '@tauri-apps/api/updater';

const update = await checkUpdate();

if (update.shouldUpdate) {
  // prompt user
}
```

---

# 9. 🎛️ Feature Flag + Remote Config System

---

## Requirements

* Fetch config at app startup
* Cache locally
* Fallback if offline

---

## Remote Config Example

```json
{
  "features": {
    "auto_invoice": true,
    "smart_reminders": true,
    "voice_mode": false
  }
}
```

---

## Fetch Logic

```javascript
const config = await fetch('/config.json').then(res => res.json());
```

---

## Local Fallback

* Store last config in SQLite

---

## Usage Example

```javascript
if (config.features.auto_invoice) {
  showAutoInvoiceOption();
}
```

---

## Advanced (later)

* User-level flags
* A/B testing
* Gradual rollout

---

# 10. 🔐 Security Requirements

* Signed updates
* Encrypted local storage (optional)
* Role-based access (if multi-user later)
* Audit logs for all actions

---

# 11. 📊 Observability

* Event logging
* Error tracking
* Update success/failure tracking

---

# 12. 🚀 MVP Scope (STRICT)

---

## Must Have

* Command overlay
* Journal + extraction
* Task system
* Invoice + signing sheet logic
* Reminder system
* OTA updates

---

## Excluded (for now)

* Full voice assistant
* Complex autonomous agents
* Multi-user system

---

# 13. 🧠 Success Metrics

* Daily active usage
* Tasks completed via Travis
* Time saved per workflow
* Reduction in missed tasks

---

# 14. ⚠️ Risks

---

## 1. Over-automation

Mitigation:

* Always confirm actions

---

## 2. Poor extraction accuracy

Mitigation:

* Allow easy correction

---

## 3. Update failures

Mitigation:

* rollback strategy

---
