# Travis v0.14.0 — Code Execution + Claude-Class Chat

**Status:** draft spec, ready to build
**Target ship:** ~6 weeks from start, 8 slices
**One-line summary:** Travis becomes capable of *any* document-shaping task a smart user asks for, via in-app Python execution, multimodal visual analysis, and a chat interface that shows its work — without losing its persistent-memory + local-first vertical-pack advantages.

---

## 1. Why this release exists

Taylor tried to generate an LTE invoice through Travis. Travis asked one slot question, "saved" something, and went silent. She lost the morning. Meanwhile in a Claude.ai conversation she handed over the same files (sample invoice + PO + WO + master coach-hours sheet + services catalog + pricing sheet) and Claude shipped multiple invoices and sign-in sheets over 3 days — visually pixel-matching a sample, finding a smoking-gun pricing-sheet mislabel by triangulating across 4 documents, solving for combinations that close a PO at exactly $65,565, iterating on a diagonal signature stroke until it looked right.

The honest read: **Claude.ai beat Travis at the one task Travis is supposed to be ahead at**. Not because the model is smarter — same model — but because Claude.ai has three capabilities Travis lacks:

1. **Python code execution.** Claude wrote `reportlab`/`openpyxl`/`pypdf` code in the moment to generate PDFs that match any layout. Travis ships hardcoded `printpdf` Rust templates. The moment a customer's letterhead differs by one font or color, our template can't help.
2. **Multimodal visual styling analysis.** Claude *opened the sample PDF as an image*, sampled colors (#5B3F86 purple header), identified the zebra striping, traced the diagonal signature stroke — then rebuilt with those exact properties. Travis uses vision only for OCR.
3. **Visible step-by-step execution.** Every Claude turn showed named substeps ("Inspecting PDF size and embedded images", "Cropping header and signature column for inspection", "Sampling header and row colors"). Taylor could *see* what Claude was doing. Travis says "thinking…" and emits an answer, or hangs.

What Travis still has that Claude.ai doesn't: persistent memory of her coaches, schools, contracts, hours; local-first data substrate; background observers; tight DB integration with `coach_hours`; the workflow/dialogue framework. **Those advantages only matter if the moment-of-truth interaction succeeds.** Today it doesn't. v0.14.0 fixes that.

The shape of the fix: keep the vertical pack and workflow recipes as the **fast path** for common tasks (a standard LTE invoice still ships through `propose_invoice_draft`), and add **code execution + multimodal analysis + visible execution** as the **escape hatch** for anything custom. The LLM picks the path each turn.

---

## 2. The architecture shift

### 2.1 Today's shape

```
User → Overlay/AskTab → journal_ingest → LLM → structured extraction →
                                                  ├─ proposedActions[] → ActionHandler (Rust) → PDF (printpdf)
                                                  ├─ tool calls (Rust functions)
                                                  └─ workflow ops
```

Every output artifact comes from a Rust handler that calls a hardcoded `printpdf` template. There's exactly one invoice layout shape, one sign-in sheet shape, one WO shape. Any deviation requires a new Rust function and a release.

### 2.2 v0.14 shape

```
User → Overlay/AskTab → journal_ingest → LLM → structured extraction →
                                                  ├─ proposedActions[] → ActionHandler (Rust)     ← fast path
                                                  ├─ tool calls (Rust functions, including run_python)
                                                  ├─ workflow ops
                                                  └─ thinking/plan/steps → visible to user

                                                LLM can also call:
                                                  • run_python(code, files?, libs?) — Pyodide worker
                                                  • analyze_document_styling(document_id) — vision
                                                  • render_pdf_page(document_id, page) — to image
```

The Rust action handlers stay. They are the fast path for the 80% case. The escape hatch is `run_python` — the LLM writes Python in the moment, mounts whatever documents it needs, generates output files, and Travis registers those outputs as documents (round-tripping through the existing substrate from v0.12).

### 2.3 Decision rule for the LLM

The system prompt teaches Travis when to use which path:

> **Use the fast path** (an existing `propose_*` or `lte_*` action) when:
> - The user's request matches a known workflow recipe AND the output target is a known template (the standard LTE letterhead invoice, the standard LTE work order, the standard LTE sign-in sheet matching the PS 19 sample).
> - No sample document has been provided.
> - The data shape fits the recipe's slot definition exactly.
>
> **Use `run_python`** (the escape hatch) when:
> - The user supplied a sample and asks Travis to "match it" or "look like this".
> - The layout, fonts, colors, or fields differ from the hardcoded template.
> - The task requires constraint solving ("find quantities that close to $65,565 exactly").
> - The task requires reading a format Travis doesn't ingest yet (.docx, .pptx, arbitrary CSV layouts).
> - Cross-document reconciliation goes beyond `reconcile_documents` (auditor-style walking back to a source).
> - The user explicitly asks Travis to "write code" or "do it like Claude does."

The fast path stays fast — milliseconds, no LLM token cost for execution. The escape path is slower (Pyodide warm-start ~5–10s the first time, then ~100ms per call) and pays the cost in token quality and execution time, but is unbounded in capability.

---

## 3. The eight slices

### Slice 1 — Code Interpreter Substrate (Pyodide)

**Goal:** Travis can execute LLM-written Python code locally with mounted documents and collected outputs.

**Approach:** Pyodide (CPython compiled to WASM) running in a hidden Tauri webview window, communicating with the main process over Tauri's event channel. No native Python install required on the user's machine. True sandbox (WASM-isolated; only files we explicitly mount are accessible).

**New files:**
- `src-tauri/src/interpreter/mod.rs` — Rust-side worker manager
- `src-tauri/src/interpreter/session.rs` — per-conversation Pyodide session (state survives across `run_python` calls within one workflow)
- `src-tauri/src/interpreter/io.rs` — mount documents into Pyodide's virtual FS, extract outputs
- `src-tauri/src/interpreter/cmd.rs` — Tauri command `run_python(...)`
- `src-tauri/tauri.conf.json` — register a second hidden window labelled `interpreter`
- `src/interpreter/PyodideWorker.tsx` — the hidden React component that loads Pyodide and listens for execution requests
- `src/interpreter/pyodide-bootstrap.ts` — Pyodide loader, library preinstaller

**Tauri command shape:**
```rust
#[tauri::command]
pub async fn run_python(
    app: AppHandle,
    state: State<'_, AppState>,
    params: RunPythonParams,
) -> Result<RunPythonResult, String>;

struct RunPythonParams {
    code: String,
    conversation_id: i64,            // session is per-conversation
    document_ids: Vec<i64>,          // documents to mount into /inputs/
    libraries: Option<Vec<String>>,  // extra micropip-installable libs to load (e.g., "reportlab")
    timeout_secs: Option<u64>,       // default 60s
    workflow_state_id: Option<i64>,  // for attributing generated docs
}

struct RunPythonResult {
    stdout: String,
    stderr: String,
    generated_documents: Vec<Document>,  // anything written to /outputs/ becomes a Document
    display_data: Vec<DisplayItem>,      // matplotlib images, etc.
    execution_ms: u64,
    error: Option<String>,
}
```

**File I/O bridge:**
- On call: documents in `params.document_ids` get their bytes mounted at `/inputs/<safe_name>.pdf` in Pyodide's virtual FS.
- Pyodide code can write to `/outputs/`. After execution, every file in `/outputs/` is extracted to managed storage and registered as a `Document` with `source = 'code_generated'`.
- A separate `/tmp/` is scratch space, discarded after execution.

**Library policy:**
- Preinstalled at startup: `numpy`, `pandas`, `openpyxl`, `pillow`, `pypdf`, `reportlab`, `python-docx`, `lxml`. These cover ~95% of LTE-style document work.
- Optional: `matplotlib`, `scikit-learn`, `sympy`, others via `micropip.install()` on demand. Slower first call.
- Disallowed: anything requiring native extensions Pyodide can't compile.

**LLM tool definition:**
```rust
ToolDef {
    name: "run_python".into(),
    description: "Execute Python code in a sandboxed interpreter with access to any documents \
        the user has attached. Generated files (PDFs, Excel, images) are automatically registered \
        as Travis documents. Available libraries: pandas, openpyxl, pypdf, reportlab, pillow, \
        python-docx, lxml, numpy, matplotlib. Use this for: custom PDF generation, document \
        reading in formats Travis doesn't natively ingest, constraint solving, multi-document \
        reconciliation with auditable code, and any task that benefits from imperative reasoning \
        over the documents.",
    input_schema: json!({
        "type": "object",
        "properties": {
            "code": { "type": "string", "description": "Python code to execute." },
            "documentIds": {
                "type": "array",
                "items": { "type": "integer" },
                "description": "Travis document ids to mount at /inputs/. Use find_documents first to get ids."
            },
            "libraries": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Extra libraries to install via micropip (only beyond the preinstalled set)."
            },
            "purpose": {
                "type": "string",
                "description": "One-line description of what this code is doing — shown to the user as a step name."
            }
        },
        "required": ["code", "purpose"]
    }),
}
```

**Build cost:** 8–10 days. Pyodide integration is well-trodden but the file-bridge and library preinstall work is real.

---

### Slice 2 — Step-Streaming Backend

**Goal:** Every tool call, action, and code execution emits structured progress events the frontend can render in real time.

**The shape of a step event:**
```rust
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum StepEvent {
    Started {
        step_id: String,        // uuid per step
        parent_step_id: Option<String>,  // for sub-steps
        conversation_id: i64,
        name: String,           // "Reading PO doc"
        detail: Option<String>, // "doc#42 (PS 498 PO)"
        kind: String,           // "tool_call" | "code_execution" | "action" | "thinking"
        started_at: String,
    },
    Note {
        step_id: String,
        text: String,           // "found 22 PS 217 entries; 9 fall in window"
    },
    Result {
        step_id: String,
        ok: bool,
        summary: String,        // one-line outcome
        error: Option<String>,
    },
    Completed {
        step_id: String,
        duration_ms: u64,
    },
}
```

**Where events fire from:**
- Every existing tool's `execute()` wraps in a step (started + result + completed)
- Action handlers fire steps from their `apply()` method
- The Pyodide interpreter fires sub-steps for its phases (mount inputs, install libs, run user code, collect outputs)
- The LLM's "thinking" sections fire `Note` events when the model emits intermediate reasoning

**Frontend listens via:** `tauri://event/step-event`, dedup'd by step_id.

**Persistence:** Steps land in a new `step` table:
```sql
CREATE TABLE step (
    id              TEXT PRIMARY KEY,        -- uuid
    conversation_id INTEGER NOT NULL,
    parent_step_id  TEXT,
    name            TEXT NOT NULL,
    detail          TEXT,
    kind            TEXT NOT NULL,
    status          TEXT NOT NULL,           -- 'running' | 'ok' | 'failed'
    summary         TEXT,
    notes_json      TEXT NOT NULL DEFAULT '[]',
    started_at      TEXT NOT NULL,
    completed_at    TEXT,
    duration_ms     INTEGER
);
```

So reloading the conversation re-renders the full step history.

**Build cost:** 3–4 days (instrumentation across existing tools + new table + event plumbing).

---

### Slice 3 — Chat Interface v2

**Goal:** Match Claude.ai's chat surface — collapsible thinking sections, named tool steps with checkmarks, syntax-highlighted code blocks, inline file cards, markdown rendering, streaming tokens.

**Component redesign:**

```
<ChatTurn>
  <UserMessage>             ← right-aligned, dark bubble
  </UserMessage>
  
  <AssistantMessage>
    <ThinkingSection collapsed-by-default />        ← gray italic, "▸ thinking…" → expand
    <StepList>
      <Step name="Reading the PO" detail="doc#42" status="ok" />
      <Step name="Filtering hours for engagement" status="ok">
        <SubStep name="parsing master sheet" status="ok" />
        <SubStep name="matching school+period" status="ok" />
      </Step>
      <Step name="Generating PDF" status="ok">
        <SubStep name="running run_python" status="ok">
          <CodeBlock lang="python" collapsible />     ← shown collapsed; expand to see code
        </SubStep>
      </Step>
    </StepList>
    <MarkdownBody>
      Travis's actual reply text, with **bold**, *italic*, tables, headers
    </MarkdownBody>
    <GeneratedFiles>
      <FileCard
        name="LTE_Invoice_IS217.pdf"
        size="42 KB"
        documentId={123}
        thumbnail="..."        ← rendered first page
        onPreview onSave />
    </GeneratedFiles>
    <ActionCards>             ← existing confirmation cards stay
    </ActionCards>
  </AssistantMessage>
</ChatTurn>
```

**Dependencies to add:**
- `react-markdown` + `remark-gfm` (tables, strikethrough, task lists) + `rehype-raw` (allow embedded HTML)
- `shiki` (syntax highlighting; loads grammars on demand) — or `prism-react-renderer` (lighter, simpler)
- `react-pdf` or our existing `preview_document` Tauri command for inline PDF thumbnails

**Streaming:**
- Backend already streams via Anthropic's SSE; we forward token-by-token to the frontend via a new event `assistant-token` (conversation_id, delta, message_id)
- The chat surface renders the partial text in real time, with a caret cursor blinking at the end while streaming

**Toasts:**
- Existing toast system kept for ephemeral status (saved, error)
- Adds a per-step "step toast" — small inline status that appears while a long step runs ("Generating PDF…") and dismisses on completion. Matches Claude.ai's "Reading PDF skill before editing" style.

**Visual style:**
- Keep Travis colors (ink/bone/pulse). Don't ape Claude.ai's beige; we're our own brand.
- Steps use the existing pulse-violet → cyan accent.
- Code blocks use a slightly raised `--color-ink-2` background.
- Generated file cards use the `◈` glyph that's already in our doc chip strip, with a thumbnail when available.

**Build cost:** 8–10 days. Most of this is component work + streaming wiring; the chat surface is the most visible piece of v0.14 so it deserves real polish time.

---

### Slice 4 — Multimodal Visual Styling Analysis

**Goal:** Travis can look at a sample PDF and extract its styling features (colors, fonts, layout, table structure, signature placement) so generated documents match the user's existing template.

**New LLM tool: `analyze_document_styling(document_id)`**

**What it does:**
1. Render the PDF (first 1–3 pages) to images using `pdfium-render` on the Rust side. This is the same pdfium dependency we considered for vision-fallback in v0.12 — adding it here pays for itself across both use cases.
2. Send those images to Claude vision with a structured-output prompt:

   > Analyze the document's visual styling. Return JSON with: header_color (hex), header_text_color (hex), body_font_family (best guess), body_font_size_estimate, table_header_color, table_alt_row_color, border_color, border_weight_estimate, font_weight_for_header ("bold"|"normal"), signature_column_present (bool), signature_stroke_type ("diagonal"|"horizontal"|"none"), key_layout_features (list of short observations), column_widths_relative (rough proportional widths), brand_logo_position ("top_left"|"top_right"|"top_center"|"none").

3. Cache the result on the document row (new `styling_json` column) so subsequent code generations don't re-pay the vision call.

**Output shape:**
```json
{
  "header_color": "#5B3F86",
  "header_text_color": "#FFFFFF",
  "body_font_family": "Arial",
  "body_font_size_estimate": 9,
  "table_header_color": "#5B3F86",
  "table_alt_row_color": "#F8F4FF",
  "border_color": "#000000",
  "border_weight_estimate": 1,
  "font_weight_for_header": "bold",
  "signature_column_present": true,
  "signature_stroke_type": "diagonal",
  "key_layout_features": [
    "portrait letter orientation",
    "tight 0.3in margins",
    "7-column table",
    "totals row spans 5 columns"
  ],
  "column_widths_relative": [0.07, 0.07, 0.18, 0.18, 0.30, 0.08, 0.12],
  "brand_logo_position": "top_right"
}
```

**Integration with code interpreter:**
The LLM uses this output as input to its `run_python` calls. Concrete pattern:

```python
# LLM-generated code (illustrative)
from reportlab.lib.colors import HexColor
from reportlab.platypus import Table, TableStyle

styling = json.load(open('/inputs/styling.json'))  # we mount the styling alongside the sample
header_bg = HexColor(styling['header_color'])
header_fg = HexColor(styling['header_text_color'])
# ... rest of the layout code uses these tokens
```

**Build cost:** 5–7 days. Most of the work is in the rendering pipeline (`pdfium-render` + Rust binding) and the prompt iteration for getting the vision output reliably structured.

---

### Slice 5 — Fast Path / Escape Path Dispatcher

**Goal:** The LLM consistently picks the right path. The fast path stays fast; the escape path absorbs everything custom.

**Prompt engineering:**

A new top-level section in the system prompt:

> ## CHOOSING PATH: structured action vs. code
>
> You have two ways to produce document outputs and complex computations:
>
> 1. **Structured actions** (`propose_invoice_draft`, `lte_create_contract_from_doc`, `lte_derive_sign_in_sheet`, etc.) — these are fast, deterministic, and produce documents in the canonical LTE format. Use them when:
>    - The request matches a known workflow recipe
>    - The output target is a known template
>    - No sample document has been supplied that you need to match
>
> 2. **`run_python`** — full Python interpreter with reportlab/openpyxl/pypdf. Slower, but unbounded. Use it when:
>    - The user supplied a sample and wants Travis to "match it"
>    - The layout, fonts, colors, or fields differ from the canonical template
>    - The task involves constraint solving (find quantities that close at $X exactly)
>    - The task requires cross-document reconciliation deeper than `reconcile_documents`
>    - The task requires reading a format like .docx, .pptx, or an unusual CSV layout
>    - The user explicitly asks for code or imperative reasoning
>
> When using `run_python`, narrate what you're about to do as the `purpose` parameter so the user sees a meaningful step name ("Generating PS217 invoice matching the supplied sample") rather than just "running code".

**Workflow recipe addition:**

`WorkflowDef` gets an optional escape-hatch finalize:

```rust
pub struct WorkflowDef {
    pub name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub slots: &'static [Slot],
    pub finalize_action: &'static str,           // existing: fast-path action kind
    pub allow_code_escape: bool,                 // NEW: LLM may instead dispatch to run_python
    pub code_escape_hint: Option<&'static str>,  // NEW: hint shown to LLM when escape is appropriate
}
```

For `lte_generate_invoice`:
```rust
allow_code_escape: true,
code_escape_hint: Some(
    "Use run_python if the user has dropped a sample invoice and asked to match it, \
     or if the customer's letterhead/fields differ from the canonical LTE template. \
     The fast-path propose_invoice_draft only produces canonical LTE letterhead invoices."
),
```

**Build cost:** 3 days. Mostly prompt + recipe-field plumbing + LLM behavior tuning.

---

### Slice 6 — Long-Running Cases (Cross-Session State)

**Goal:** A multi-day, multi-correction workflow like Taylor's PS 89 reconciliation stays coherent. Travis remembers "we were working on the PS 89 invoice #3, here's the reconciliation we agreed to, the COO came back with a correction yesterday."

**The case abstraction:**

```sql
CREATE TABLE travis_case (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id    INTEGER NOT NULL,
    name            TEXT NOT NULL,              -- "PS 89 invoice #3"
    summary         TEXT,
    status          TEXT NOT NULL DEFAULT 'open',  -- 'open' | 'paused' | 'closed'
    parent_case_id  INTEGER,                    -- cases can decompose
    started_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_activity_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    closed_at       TEXT,
    UNIQUE (workspace_id, name, status) -- can't have two open cases with same name
);

CREATE TABLE case_artifact (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    case_id     INTEGER NOT NULL REFERENCES travis_case(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,    -- 'document' | 'decision' | 'note' | 'reconciliation_table'
    payload     TEXT NOT NULL,    -- JSON
    created_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

**How it integrates:**
- Workflows can be wrapped in a case. When a workflow completes, the case stays open (with a summary). Conversations can resume work on the case via natural language ("back to the PS 89 invoice").
- Generated documents get tagged with the case so they're retrievable as a set.
- A case has a rolling summary the LLM maintains — the equivalent of Claude.ai's conversation memory but pinned to a named long-running unit.

**New LLM tools:**
- `open_case(name, summary?)` — start tracking
- `note_case(case_id, kind, payload)` — record a decision or artifact link
- `close_case(case_id)` — done
- `find_case(query)` — look up cases (substring + recency)

**Prompt integration:** When a case is active in the conversation, its summary + the last 5 case_artifact entries are injected into the prompt, similar to how `initiatives` is today.

**Build cost:** 4–5 days. Cases are like initiatives v2 — more structured, with artifact tracking. Could even subsume initiatives in a future cleanup.

---

### Slice 7 — First Real Use Case: LTE Invoice Code Path

**Goal:** Convert `lte_generate_invoice` to support both paths and produce a working end-to-end demo of code-driven generation. Test against Taylor's actual workflow.

**What ships:**

1. **Sample-driven invoice generation.** Taylor drops a sample invoice (any layout). The dialogue manager detects this is a sample (via doc kind or LLM intent). The recipe routes to the code path:
   - `analyze_document_styling(sample_doc_id)` → styling JSON
   - LLM uses styling JSON + Taylor's PO/WO/sheet data to generate Python
   - `run_python(...)` produces the new invoice matching the sample
   - Result registered as a document, surfaced as a file card

2. **Template memory.** A new `pack_template` table:
   ```sql
   CREATE TABLE pack_template (
       id              INTEGER PRIMARY KEY AUTOINCREMENT,
       workspace_id    INTEGER NOT NULL,
       pack_slug       TEXT NOT NULL,
       kind            TEXT NOT NULL,        -- 'invoice' | 'sign_in_sheet' | 'work_order'
       label           TEXT NOT NULL,        -- user-given name, e.g. "PS 19 sheet format"
       styling_json    TEXT NOT NULL,
       sample_document_id INTEGER,
       generation_code TEXT,                 -- saved Python that worked; reusable next time
       counterparty_hint TEXT,               -- e.g. "PS 217" if the template is specific to one school
       created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
       used_count      INTEGER NOT NULL DEFAULT 0
   );
   ```

3. **Save-as-template flow.** After a successful sample-driven generation, Travis offers: "Want me to save this as a template for [counterparty]? Next time you invoice them I'll use the same format automatically." If yes → row in `pack_template` with the working Python + styling.

4. **Template-aware fast path.** The fast path becomes: "if `pack_template` exists for this counterparty + kind, use its saved code; otherwise use the hardcoded printpdf template; otherwise (sample provided) generate fresh."

**Concrete chat flow:**

```
Taylor: I need to invoice IS 217 for the work that just wrapped. Use this format. [drops sample PDF]

Travis: ▸ Analyzing the sample's styling
        ▸ Reading the PO (doc#42)
        ▸ Reading the master coach-hours sheet (doc#43)
        ▸ Filtering 11 sessions for IS 217 between 02/27 and 05/26
        ▸ Reconciling against PO total ($15,000 = 10 × $1,500)
        ⚠ Note: 11 days delivered but PO caps at 10 — billing 10, listing 11 (per your earlier rule)
        ▸ Generating PDF
          ▸ running run_python
            Purpose: Build IS 217 invoice matching supplied sample,
                     11 dates listed, 10 days billed at $1,500
            [code block — collapsed; expand to see]
        ▸ Registering generated document
        
        Done. The invoice matches the sample's purple header, zebra rows, and
        signature stroke. 11 dates listed, 10 days × $1,500 = $15,000 billed.
        
        [📎 LTE_Invoice_IS217.pdf · 42 KB · open · save as]
        
        Want me to save this layout as a template for IS 217? Next time you
        invoice them I'll use the same Python without re-analyzing the sample.
```

**Build cost:** 5 days. Composes Slices 1, 2, 3, 4, 5, and 6 against a real workflow.

---

### Slice 8 — Verification & Polish

**Goal:** Travis can do every task in the Claude.ai conversation. Verified end-to-end. No regressions.

**Concrete acceptance tests (Taylor's real flow):**

1. **Generate IS 217 invoice from sample + PO + master sheet** — matches the sample's styling, lists 11 dates / bills 10 days, takes ≤ 60 seconds end-to-end.
2. **Generate IS 217 sign-in sheet matching the PS 19 sample** — purple header, black borders, zebra striping, diagonal signature stroke corner-to-corner, 11 rows + total row, fills 2/27 hours when supplied.
3. **PS 89 invoice #3 reconciliation** — Travis ingests PO + invoice #1 + invoice #2 + Appendix F + PS 89 pricing sheet, finds the $4,769/$3,461 School Assessment mislabel, surfaces it as a data-integrity warning, proposes a corrected invoice #3 that closes the PO at exactly $150,000.
4. **Constraint solving** — given "I need invoice #3 to close out the remaining $65,565," Travis can find combinations of services × catalog rates that sum exactly, surface multiple options, let user choose.
5. **Mid-conversation correction** — "actually, transformational 1 and 2 were already billed" → Travis updates the case, rebalances the math, regenerates the invoice. Same conversation thread.

**Polish items:**
- Toasts during long steps ("Generating PDF…" with a spinner)
- Step durations rendered inline so the user can see what's expensive
- Generated file cards with thumbnails for PDFs
- Error rendering when Python crashes — show traceback in expandable section
- "Re-run with edit" button on generated files (modify the code that produced it)
- Token cost surface (optional, behind a setting) — show running cost per turn so the user can manage budget

**Build cost:** 3–5 days, mostly bug-fix and polish.

---

## 4. Build sequence + timeline

The slices have weak dependencies. A reasonable schedule for one engineer:

| Week | Slice | Status |
|---|---|---|
| 1–2 | Slice 1: Code interpreter substrate | Foundation; nothing else works without it |
| 3 | Slice 2: Step-streaming backend | Unblocks Slice 3 |
| 3–4 | Slice 4: Multimodal visual styling | Parallel to Slice 2 — unlocks Slice 7 |
| 4–5 | Slice 3: Chat UI v2 | Visible to user; biggest perception impact |
| 5 | Slice 5: Fast/escape dispatcher | Small slice, mostly prompt work |
| 5–6 | Slice 6: Long-running cases | Independent; can land alongside |
| 6 | Slice 7: First real use case | Composes everything, validates end-to-end |
| 6 | Slice 8: Verification + polish | Acceptance tests + bug fixes |

**Total:** ~6 weeks for one focused engineer. Realistic, not aggressive.

---

## 5. Honest tradeoffs

- **Bundle size.** Pyodide adds ~10MB compressed to the binary (~30MB uncompressed when loaded). Travis goes from a ~50MB install to ~60MB. Acceptable for what it unlocks.
- **First-run latency.** Pyodide warm-start is 5–10 seconds. We preload at app start (in a hidden window) so it's hot by the time the user needs it. Subsequent code runs are sub-second.
- **Library coverage.** Pyodide can't run native-extension libraries. Most LTE document work uses pure-Python (reportlab, openpyxl, pypdf) which works fine. PIL/Pillow works. pdfium does not — but we use pdfium server-side via Rust for vision-rendering needs.
- **Security surface.** Pyodide is WASM-sandboxed; it cannot reach the host filesystem outside what we mount. Network access is gated (no `fetch` by default; we expose a Travis-mediated `travis.fetch_url` that the LLM can call if needed, with policy controls).
- **Cost.** Vision-styling analysis adds a Claude API call per sample. Caching the styling JSON on the document row means it's a one-time cost per sample.
- **LLM determinism.** The LLM writing fresh Python each time means runs aren't reproducible. Mitigated by Slice 7's template memory — successful generations get saved and reused. Over time, the LLM writes less and less code because more templates are saved.
- **"Wait, are we just rebuilding Claude.ai?"** Partly. The capability is the same. The difference is everything around it — persistent vertical memory, local-first data, background observers, OS integration, tight DB connection to the user's actual operational tables. We're not competing with Claude.ai on raw flexibility; we're matching its flexibility while keeping the rest.

---

## 6. Open questions

1. **Pyodide window architecture.** Hidden Tauri webview vs. Web Worker in the existing main webview? The hidden webview approach gives stronger isolation; the worker approach is simpler to wire. Initial pick: hidden webview (better isolation, easier to swap for a cloud sandbox later).

2. **Where do generated documents save?** Same content-addressed storage as ingested documents (probably under `documents/<hash>/`). Should the user be able to specify "save to Downloads"? Yes — we already have `app.path().download_dir()`. Add a preference: "Generated files copy to Downloads automatically" (off by default; on for desktop heavy users).

3. **Cost gating.** Should there be a per-conversation token-cost ceiling so a runaway loop doesn't burn $50? Yes — soft cap with prompt confirmation when crossed. Settings-configurable.

4. **`run_python` in untrusted contexts.** Today's LLM is Anthropic/OpenAI which we trust. If we ever support local models or third-party gateways, code execution becomes higher-risk. Defer to that future; v0.14 assumes Anthropic/OpenAI.

5. **Should the workflow recipe be allowed to *require* the code path?** E.g., a future recipe `generate_arbitrary_invoice` that has no fast path — only Python. Yes. The field is `allow_code_escape: bool`, but recipes can also set `fast_path_action: None` to force the code path.

6. **Replace `printpdf` entirely with code-generation?** Not in v0.14. The fast path stays as the deterministic + tested route for canonical LTE outputs. Long-term (v0.17+), as more templates accumulate in `pack_template`, the hardcoded handlers can be deprecated in favor of saved code.

7. **Cross-platform Pyodide.** Works identically on macOS / Windows / Linux because it runs in WebView. ✓.

8. **Does this open the door to skills like Claude.ai's?** Yes. Each saved `pack_template` + recipe + escape code IS a skill in our system. A future v0.15 could let users (or LTE peers) share templates via a registry. Out of scope for v0.14 but worth noting.

---

## 7. What this release is NOT

- **Not a multi-user / sync release.** That's Phase 6 (cloud relay).
- **Not a model upgrade.** Same Claude Sonnet, Claude Haiku, OpenAI providers.
- **Not a marketplace.** No template sharing yet.
- **Not a rewrite of the LTE pack.** All existing actions, tools, workflows stay working. Code path is additive.
- **Not the EDU vertical fork.** Per `project_edu_vertical_first` direction, we're still horizontal-first.
- **Not full session continuity yet.** Slice 6 introduces cases; full multi-user-collaborative case handoff is later.

---

## 8. Success criteria (binary)

v0.14.0 ships when:

- [ ] Pyodide loads in ≤ 8s on a mid-range laptop and stays warm
- [ ] `run_python` executes 95th-percentile in ≤ 2s for reportlab-style PDF generation
- [ ] Every tool / action / code execution surfaces named steps in the chat UI
- [ ] Markdown / code blocks / file cards render correctly in the chat UI
- [ ] All 5 acceptance tests in Slice 8 pass against Taylor's real documents
- [ ] No regressions in existing v0.13 workflows (`lte_generate_invoice` via fast path, `lte_derive_sign_in_sheet`, `lte_create_contract_from_doc`)
- [ ] Pack templates can be saved and re-used on next invoice for the same counterparty
- [ ] Multi-day case persistence verified across an app restart

---

*This spec is the result of reading the full Claude.ai conversation Taylor and Michael had on 2026-06-05 → 2026-06-07. The capabilities listed here are exactly what Claude.ai demonstrated; v0.14.0 brings them into Travis without losing what Travis already has.*
