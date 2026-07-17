# Travis Changelog

## v0.28.76 — Voice tick-loop log spam → DEBUG (2026-07-17)

`voice::capture.rs` fires a per-tick VAD diagnostic every 500ms
forever while the capture thread runs — from app boot onward. At
`tracing::info!` level it flooded dev terminals with
`[voice] rms=... state=silent utterance_len=0` even when both
ambient mode + Hey Jarvis wake were off (the capture thread is
always up to be ready for mic press; only wake INFERENCE is
gated by the toggle).

Dropped to `tracing::debug!`. Available via `RUST_LOG=debug` when
voice-debugging; silent by default. Other voice INFO logs are
event-based (state transitions, arm/disarm, speech-end) and stay
at INFO — those are useful.

## v0.28.75 — Prompt fix: code must be inline in the response field (2026-07-17)

The user's second Bezier-code test shows Claude reasoning
*"Let me provide it now as a proper code_snippet part"* and then
never writing the code. There is no "code_snippet part" output
channel — Claude was hallucinating a separate mechanism.

Fix: expanded the `report_extraction` tool's `response` field
description in `journal.rs::build_extraction_tool` to be explicit:

> CODE: if the user asks for code, PUT THE CODE HERE in this
> response field as a Markdown fenced code block: triple-backtick +
> language + newline + code + newline + triple-backtick. NEVER
> promise code and omit it. NEVER say 'here's the code' or 'let me
> provide it as a code_snippet part' without including the actual
> code inline in this field. There is NO separate code_snippet
> output channel — Markdown-fenced code blocks embedded in this
> response field ARE the way code renders. If a snippet is long,
> still include it in full — do not truncate or promise a
> continuation.

The two specific hallucinated patterns Claude used (`code_snippet
part`, "here's the code" without code) are now called out by name
so the model can't slip into them again.

## v0.28.74 — Tool marker strip + preserve streamed code (2026-07-17)

Both remaining items from the v0.28.73 "still investigating" list.

### 1. Tool call name leaking into visible content

Users saw `report_extraction` as the entire visible assistant response.
Cause: Claude sometimes prefixes a tool call with a plain-text mention
of the tool name; when the extraction path fell through, that
prefix persisted as the message content.

Two-sided fix:
- **Rust:** `journal.rs` `strip_leaked_tool_markers()` runs on the
  assistant response text before it's written to the DB. Removes
  lines that are exactly a known tool name (`report_extraction`,
  `extract_report`, `run_python`, `edit_python_artifact`,
  `read_document`, `search_docs`) OR that match the
  "Calling <tool>..." / "Using tool: <tool>" preambles Claude
  sometimes emits.
- **Frontend:** `ChatCanvas.sanitizeAssistantContent()` re-runs the
  same strip on the display path — belt-and-suspenders for messages
  already persisted before the Rust fix.

### 2. Code missing on second code request

Users' first code snippet (polygon clipping) rendered perfectly; the
second (Bezier) showed only the intro sentence. Root cause: the
`journal://assistant-done` event carries `content: assistant_visible`
from Rust, which is the extraction tool's `response` field. If the
extraction returned a summary shorter than what streamed (which
happens when the tool loop's synthesis prose is briefer than the raw
streamed answer with code), my `swapTmpId` handler REPLACED the
streamed content — dropping the code.

Fix: in `useConversationStream`'s `assistant-done` handler, compare
the streamed content length to the done event's content. Take the
LONGER of the two (with a 40-char slack to allow for whitespace
differences). Now the streaming path acts as insurance against the
extraction path stripping content.

## v0.28.73 — Doc viewer overlay + voice static import (2026-07-17)

Fixes v0.28.72 gaps found by the user:

### 1. Clicking a doc card / doc list item did nothing

`viewerDocumentId` was ONLY consumed by the legacy `Manage.tsx` view
(`Manage.tsx:200-213`). The v2 workspace never mounted anything that
watched the store field, so `setViewerDocumentId(id)` was a silent
no-op. Setting the ID both from `DocCardClickable` (chat message) and
from `DocumentsOverlay` (documents list) went nowhere.

New: `src/workspace/v2/DocumentViewerOverlay.tsx` — modal-style
overlay that watches `viewerDocumentId` and renders `<DocumentViewer/>`
inside a backdrop-blurred sheet. Esc + backdrop-click close.
Mounted alongside the other overlays in WorkspaceV2.

### 2. Voice audio still not instant

The v0.28.72 optimistic-insert helper (`insertOptimisticUserMessage`)
was dynamically imported (`await import(...)`) inside the speech-end
handler. On first voice turn the module hadn't been loaded yet, so
Vite fetched it right when we needed it — ~150-300ms of latency.
Cached after that, but the first press was slow.

Static imports in both `useNativeVoice.ts` and `Composer.tsx`. First
voice turn now inserts as fast as subsequent ones.

### Deferred (still investigating)

- **Missing code on second Bezier response.** First code snippet
  (polygon clipping) rendered perfectly via the same MarkdownBody /
  CodeBlock path. Second response shows only the intro sentence.
  Same rendering path — this is almost certainly an LLM output issue
  (the model stopped early), not a UI bug. Reproducing to confirm.
- **`report_extraction` marker leaking into visible content.** Tool
  call name leaked into the persisted assistant message; needs a
  Rust-side content sanitizer in `journal_ingest`. Tracking
  separately.

## v0.28.72 — Infinite loop + voice optimistic + code + doc cards + new chat (2026-07-17)

Five real bugs from user feedback on v0.28.71. Every one is a
regression from the v0.28.70 chat rewrite.

### 1. Infinite render loop (the "app crashes" symptom)

`useChatStore((s) => s.messagesMap[id] ?? [])` created a NEW empty
array on every render when the conversation had no messages yet.
useSyncExternalStore diffs by reference — it sees a "changed" value,
triggers re-render, selector runs again, new array, infinite loop.
CanvasErrorBoundary catches it as "Maximum update depth exceeded".

Fix: module-level `EMPTY_MESSAGES: UIMessage[] = []` reused as the
stable fallback. Same reference every render, no diff spike.

### 2. Audio card takes ~9s to appear on voice submit

Voice submits flow through AskTab via `pendingComposerSubmit`, not
through Composer.handleSubmit. AskTab wasn't updated to insert into
chatStore, so ChatCanvas stayed empty until `journal://user-inserted`
landed — usually right before the assistant streams. From the user's
POV: mic release → thinking → nothing → nothing → user + assistant
both drop 9 seconds later.

Fix: `useNativeVoice` speech-end handler inserts the optimistic user
message into chatStore directly (with the current `pendingVoiceAudio`
attached) BEFORE setting `pendingComposerSubmit`. The audio card + text
render the instant whisper returns; journal://user-inserted still
does the tmpId → realId swap when Rust persists.

### 3. Code blocks + doc cards missing from assistant messages

Two root causes:
- **MarkdownBody** overrode `<p>` as `<p>`. Claude often outputs a
  fenced code block INSIDE a paragraph — the resulting
  `<p><CodeBlock/></p>` has `<div>` + `<pre>` nested in `<p>`, which
  is invalid HTML. React 19 refuses to render the tree; the whole
  message shows as headers with an empty content slot below.
- **`doc#N` markers** rendered as plain text instead of a doc card.
  Old ChatTurn had the marker-parse + FileCard render in
  `ChatTurn.tsx:132-138`; the ChatCanvas rewrite dropped it.

Fixes:
- `MarkdownBody` renders `<p>` as `<div className="my-2">` — same
  visual, but block content nests without complaint.
- `ChatCanvas` now scans assistant content for `doc#\d+` markers,
  strips them from displayed text, and renders `FileCard`s in a
  chip-row below the message.
- New `DocCardClickable` wrapper opens the document viewer overlay
  on click by setting `viewerDocumentId` on the app store (which
  App.tsx already routes through `DocumentViewer`).

### 4. No way to start a new conversation

Cmd/Ctrl+N is now bound at the WorkspaceV2 level — sets
`activeConversationId = null` which pops ChatCanvas into empty state.
Empty state now shows a hint: `⌘/Ctrl · N — new conversation`.

## v0.28.71 — v0.28.70 fix + smooth-drain (2026-07-17)

Two fixes on top of v0.28.70 that materially change how streaming
reads:

1. **Compile error fix.** `assistant_tmp_id` was declared inside the
   manager iteration loop but the `journal://assistant-done` emit
   reads it far outside that scope. `error[E0425]: cannot find value
   'assistant_tmp_id' in this scope`. Moved the declaration to the
   outer journal_ingest scope so the tmpId lives for the whole call
   (which is what "one assistant turn = one tmpId" already assumes).

2. **Smooth-drain (lobe-chat parity).** Direct port of lobe-chat's
   `createSmoothMessage` (packages/fetch-sse/src/fetchSSE.ts:127-217).
   `src/chat/smoothMessage.ts` — adaptive character-per-frame RAF
   drain. Queue depth drives the drain rate:
   `currentSpeed += (max(startSpeed, queueLen) - currentSpeed)
                   * (|Δqueue| * 0.0008 + 0.005)`.
   When Claude dumps 100+ chars in one SSE event, speed ramps up
   toward the queue depth. As the queue drains, speed decays back
   toward 12 chars/sec. Feels like fluid typing regardless of
   upstream bursty cadence.

   `useConversationStream` routes `journal://assistant-chunk`
   through `getSmoother(conversationId, tmpId).push(delta)` instead
   of calling `appendContent` directly. `.done()` fires on
   `assistant-done`.

### Still deferred to v0.28.72

- **Abort controller / Stop button.** Attempted here; needs an
  `abort_flags` field on `AppState` which requires cascading changes
  to every `AppState` construction site. Doing it half-way would
  ship broken code — pulled the change, deferring properly.
- **Mid-stream error footer.** UIMessage has the `error` and
  `aborted` fields. Population from the journal_ingest error path is
  a small next step; deferred with abort so they ship together.

## v0.28.70 — Chat rearchitecture: messagesMap + reducer + tmpId (2026-07-17)

The real lobe-chat-shaped rewrite. Streaming pipeline from v0.28.66 is
kept; the FRONTEND state model is replaced.

### What changed structurally

**Before (v0.28.66 → v0.28.68 hybrid):**
- ChatCanvas rendered from `useFocalContent` (DB polling)
- Streaming state lived in a parallel `streamingAssistant` store slot
- Optimistic user bubble lived in `useOptimisticSubmit` refs inside ChatCanvas
- Voice audio lived in `pendingVoiceAudio` + `voiceAudioLinkedMessageId`
- Three separate state models, all trying to describe the same message

**After (v0.28.70):**
- Single `chatStore.messagesMap[conversationId]` — the source of truth
- Reducer-style actions: `createMessage`, `updateMessage`, `appendContent`,
  `appendReasoning`, `addToolCall`, `swapTmpId`, `removeMessage`,
  `hydrateConversation`, `clear`
- UIMessage carries ALL its state: content, reasoning, toolCalls, audio,
  streaming flag, error, aborted flag, tmpId, real DB id
- Every event mutates ONE list by (tmpId or realId). No parallel stores.

### New wire protocol

Rust `journal_ingest` now emits one event per lifecycle stage:

1. `journal://user-inserted` — user message row landed with real DB id.
2. `journal://assistant-message-created` — LLM turn started; carries a
   `tmpId` for the frontend to create the assistant row optimistically
   (before ANY token arrives).
3. `journal://assistant-chunk` — text delta, carries `tmpId`.
4. `journal://reasoning-chunk` — thinking delta, carries `tmpId`.
5. `journal://assistant-tool-start` — tool call started, carries `tmpId`.
6. `journal://assistant-done` — final content + real DB id, carries `tmpId`
   for the swap.

Every message goes through the same tmpId → realId swap at the boundary
between "in-flight" and "persisted". The renderer never sees a state
transition — it just sees `id` change from string to number.

### New frontend pieces

- **`src/stores/chatStore.ts`** — Zustand store, single messagesMap,
  reducer actions. Immer-lite manual patches (Zustand's set()
  semantics compose fine at this scale).
- **`src/chat/useConversationStream.ts`** — the ONE hook that listens
  to every journal event and mutates chatStore. Replaces v0.28.66's
  `useAssistantStream`. Mounts once at WorkspaceV2 root.
- **`src/chat/useHydrateChatStore.ts`** — on conversation switch,
  fetches the DB thread and populates chatStore. Idempotent per
  conversation.
- **ChatCanvas rewrite** — reads exclusively from
  `chatStore.messagesMap[activeConversationId]`. No polling. No
  parallel refs. No optimistic-slot juggling. Renders lobe-chat
  message anatomy (bubbleless assistant, avatar, hover actions,
  streaming cursor, tool chips, reasoning block, error footer) with
  full v0.28.66 visual continuity.
- **Composer** — inserts optimistic user message into chatStore on
  submit; the tmpId is auto-swapped for the real DB id when Rust
  emits `journal://user-inserted`.

### Explicit non-goals for this release

- **Abort controller / Stop button** — not wired yet. UI has no way
  to interrupt a stream. Deferred to v0.28.71.
- **Smooth-message RAF drain** — deferred; deltas still land as they
  arrive (React 19 handles it fine but not the "silky typing" feel).
- **Mid-stream error footer** — UIMessage has the `error` and
  `aborted` fields wired; the actual error-catching + population is
  deferred to v0.28.71.

## v0.28.68 — Early audio card + live reasoning/tool render (2026-07-17)

Two blockers on the streaming story that shipped in v0.28.66. Both were
real gaps in the per-part emit architecture; addressing them now.

### 1. Audio card renders BEFORE whisper finishes

v0.28.66 shipped streaming but the audio card still waited for whisper
to complete (~700ms of "Travis thinking" before ANYTHING appeared).
User called it out: card should appear the instant the mic is released.

Fix: `voice_finalize_transcript` now saves the WAV first, emits
`voice://audio-ready { audioPath, durationMs }` on that spot, THEN
runs whisper. Frontend `useNativeVoice` listens for the new event
and sets `pendingVoiceAudio` immediately with an empty transcript.
The `__voice_transcribing__` bubble in ChatCanvas already renders
when `voiceTranscribing && pendingVoiceAudio`, so the audio player
appears instantly; the transcript fills in when finalize returns.

Same treatment applied to the prewarm-fast-path — emits
`voice://audio-ready` with the cached prewarm audio_path.

### 2. Reasoning + tool calls render live during the stream

v0.28.66 wired `ReasoningDelta` and `ToolCallStart` events all the way
from Anthropic SSE → `StreamCallback` → `journal://reasoning-chunk` /
`journal://assistant-tool-start` → `streamingAssistant.reasoning` /
`.toolCalls` in the Zustand store. But ChatCanvas ignored those slots
— everything crammed into one text bubble.

Fixed by rendering ABOVE the response text, during the stream, in
lobe-chat's per-part shape:

- **ToolCallsInline**: horizontal chip strip. Each chip shows the
  tool name in Travis's mono-uppercase font with a pulsing green
  status dot ("running"). Chips appear the instant `tool_call_start`
  fires and vanish when `assistant-done` clears the streaming slot.

- **ReasoningBlock**: subtle muted-panel showing the extended-thinking
  text as it streams (Anthropic `<thinking>` blocks). Bronze
  "THINKING" label on top, italic body text. Reads as thought, not
  as answer.

- **Streaming bubble activation**: the bubble now renders when there's
  ANY signal (text OR reasoning OR tool calls), not just text. That
  means the user sees Travis moving the instant the LLM starts
  reasoning or calling a tool — before the first response token
  arrives.

This is the "emit-per-part → render-per-part" flow lobe-chat / claude.ai
use: user turn shows up → reasoning streams → tool cards flip to
running → response text builds → follow-ups repeat. No more monolithic
"Travis is thinking" pause.

## v0.28.67 — v0.28.66 hotfix: dev-mode DLL glob + prewarm poison (2026-07-17)

Two defensive fixes on top of v0.28.66 while I chase a runtime crash a
user reported on the installed build.

1. **Dev-mode `resources/vc/*.dll` glob failure.** The `tauri.windows.
   conf.json` overlay (v0.28.64) requires the VC++ runtime DLLs to exist
   in `src-tauri/resources/vc/` — CI stages them from the Windows
   runner's `C:\Windows\System32` before build. In local `npm run tauri
   dev` those files don't exist, and Tauri's config resolver bails at
   startup on the unresolved glob. Added `resources/vc/placeholder.dll`
   (empty, committed) so the glob always resolves to at least one file;
   CI still overwrites with the real 3 DLLs on release builds.

2. **Prewarm mutex poison-recovery.** v0.28.65 added `prewarm_in_flight:
   Mutex<Option<usize>>` for the dedup guard, using `.lock().unwrap()`
   throughout. If a prewarm task ever panicked while holding one of
   those locks, the mutex was poisoned and every subsequent mic press
   panicked at the same call site — presenting as "app crashes on mic".
   Now recovers from `PoisonError` via `p.into_inner()` for both
   `prewarm` and `prewarm_in_flight` locks.

Not fixed here: the installed-build crash on voice submit. Needs the
actual panic trace before I can pinpoint. If you can reproduce and
capture stderr, please share the `thread '...' panicked at ..., file:
line` line.

## v0.28.66 — Streaming + lobe-chat message anatomy + CPU pass (2026-07-17)

Three big changes in one release, all threaded through Travis's canvas
HUD frame so nothing about the app's identity changes — but the chat
mode now streams, breathes, and stays out of your CPU's way.

### 1. Streaming LLM responses (Anthropic SSE, end-to-end)

The single biggest perceived-latency win. Tokens now stream into the
assistant bubble as Anthropic produces them, replacing the 5–15s wait
for a whole response to land at once.

New `LlmProvider::chat_with_tools_streaming` trait method with a
`StreamCallback` callback modeled on lobehub/lobe-chat's chunk union
(see `packages/fetch-sse/src/fetchSSE.ts:36-92`). Six event types —
`TextDelta`, `ReasoningDelta`, `ToolCallStart`, `ToolCallInputDelta`,
`ToolCallComplete`, `Done` — same shape across every provider.

Wire path:
- `ClaudeProvider` implements SSE parsing directly against
  `api.anthropic.com/v1/messages` with `stream: true`. Custom
  byte-buffer parser handles `message_start` / `content_block_start` /
  `content_block_delta` / `content_block_stop` / `message_delta` /
  `message_stop` / `error` / `ping` — no external SSE crate.
- `TravisCloudProvider` reuses the same parser — passes `stream: true`
  to the cloud, which already tees the upstream SSE for metering and
  forwards to the client verbatim.
- `journal_ingest` calls `chat_with_tools_streaming` with a callback
  that emits `journal://assistant-chunk` per text delta,
  `journal://reasoning-chunk` per thinking delta,
  `journal://assistant-tool-start` per tool call, and
  `journal://assistant-done` on completion.
- Frontend `useAssistantStream` hook (mounted once at WorkspaceV2 root)
  listens for those events and accumulates into store's
  `streamingAssistant` slot.
- `ChatCanvas` renders a live bubble from the store slot, auto-scrolls
  during streaming (unless the user has scrolled up to read history),
  clears the bubble when `assistant-done` fires and the persisted
  row appears in the polled thread.

### 2. Lobe-chat message anatomy (chat canvas rewrite)

The chat mode canvas now uses lobe-chat's message anatomy verbatim,
themed to Travis's palette. What changed:
- **Assistant messages are bubbleless.** Silvery-bronze-lavender avatar
  + `TRAVIS` label + timestamp header + free-flowing markdown flush
  left. Author label and timestamp fade in on row hover at 200ms
  `motionEaseOut` — same pattern as lobe-chat's
  `ChatItem/style.ts:5-39`.
- **Hover-reveal action strip** on completed assistant messages —
  Copy / Regenerate / Fork. Same fade-in timing.
- **Streaming cursor** is a lavender-tinted blinking rectangle with
  soft glow at the tail of the in-flight bubble. 1.4s blink on
  `cubic-bezier(0.22, 1, 0.36, 1)`.
- **Avatar shimmer while streaming** — Travis's spheroid avatar
  breathes at 2.2s intervals during token stream.
- **User bubble kept.** Travis's dashed-purple identity is retained
  for optimistic bubbles, solid border for persisted. Voice audio
  card renders inline via the v0.28.63+ persistent-store path.
- **No model chip.** Travis Cloud is the abstraction. Users see
  "Travis", never "Claude Sonnet 4.6". The model tag from the concept
  mock is removed from every user-facing surface.

### 3. CPU efficiency pass

Users reported terminals becoming unresponsive while Travis was running.
Two concrete fixes:
- **Wake worker fully idles when paused.** Previously the tick loop
  still drained + `try_send`'d 80ms chunks to the wake worker even
  when `SetWakePaused(true)` had been received (the worker discarded
  them but the batching cost per tick was real). Added
  `wake_paused_local` mirror in the tick loop; the whole
  chunk-batching path is now skipped while paused.
- **Whisper thread cap lowered** from `min(cores, 4)` to
  `min(cores/2, 3).max(1)`. tiny.en runs fine on 2–3 threads
  (~800ms per 5s utterance on M1); leaving cores free for the
  user's terminal / browser matters more than shaving 200ms off
  transcription.

### Not in this release
Session sidebar redesign (lobe-chat NavPanel Accordion), cmdk command
palette, full antd token-mirror theme, OpenAI/Ollama streaming
(both fall through the default trait impl and emit one big
TextDelta + Done — same event shape, just no incremental UI feedback).
Coming in follow-up releases.

## v0.28.65 — Audio card actually appears + CPU steal fix (2026-07-17)

The v0.28.63 "persistent pendingVoiceAudio in the store" fix didn't
actually work in the app. Fork-agent audit found three concrete
causes, each fixed here.

### 1. AskTab was racing to clear pendingVoiceAudio

`AskTab.tsx:531-550` still had the pre-v0.28.63 code that cleared
`pendingVoiceAudio` and re-invoked `linkUtterance` the moment
`journalIngest` returned. v0.28.63 removed the eager clear from
`useNativeVoice.ts` but left this second copy in AskTab intact. So
the store field held for ~50ms into the journal turn (long enough
for `useNativeVoice`'s `journal://user-inserted` listener to link
audio), then AskTab wiped it as the LLM turn returned — the fresh
ChatCanvas mount had already lost its useRef snapshot in v0.28.61,
and the store-based fallback shipped in v0.28.63 was killed by
AskTab's redundant clear.

Fix: **delete the AskTab clear+link block entirely**. `useNativeVoice`
already links the utterance on `journal://user-inserted` (fires within
~50ms of the user message insert, long before the LLM finishes), and
ChatCanvas clears the persistent field when the real linked message
appears in the thread. The AskTab code was pure redundancy — and the
redundancy IS the bug. Do not put it back.

### 2. Empty placeholder bubble during the 700ms finalize window

`ChatCanvas.tsx` rendered a `__voice_transcribing__` bubble whenever
`voiceTranscribing===true`. That flag flips true BEFORE whisper
returns audio+transcript, so during the ~700ms wait the bubble
rendered with `content=""` and `audio=undefined` — an empty box
users reported as "empty chat bubble". Skip the placeholder entirely
when there's nothing yet to show; the composer's own thinking
indicator already covers the gap.

### 3. Prewarm was spawning concurrent whisper tasks

`voice_prewarm_transcript` (Rust) spawned a full whisper inference
every time VAD emitted a `voice://speech-pausing` event. VAD can
bounce Speech↔ProbablySilence several times during a single pause,
so one utterance dispatched N prewarm tasks, each pinning
`min(cores, 4)` threads concurrently. On a 4-core machine that's
~16 whisper threads running at once when the user pauses mid-sentence
— saturating CPU and making other apps unresponsive (users reported
terminals hanging while Travis was running).

Fix: added `prewarm_in_flight: Mutex<Option<usize>>` to `VoiceState`.
The command guards on:
- A ready prewarm within tolerance of the current sample count →
  skip (finalize would just reuse it anyway).
- An in-flight prewarm within tolerance → skip (already computing
  essentially the same thing).
Marker is set before spawn, cleared at task end on any path (wrapper
async block guarantees this).

## v0.28.64 — Windows VC runtime + persistent voice card (2026-07-17)

Combines two fixes because both were needed to make Windows 11 users
whole again in the same release. Retags v0.28.63 whose CI failed
before any release published (VS 2025 Preview shifted the CRT dir
layout — this release copies DLLs from `System32` instead of walking
the VS install).

Bundles TWO fixes because both were needed to make Windows 11 users
whole again in the same release.

### 1. VC++ runtime DLLs bundled alongside app.exe

Fresh Windows 11 users hit "MSVCP140.dll was not found" on first
launch. Tried static-linking the CRT (v0.28.62 attempt) — didn't
work because `fastembed` pulls in `ort` → `ort-sys`, which ships
pre-built ONNX Runtime binaries linked with /MD. Can't mix /MT and
/MD in a single binary.

Fallback: ship msvcp140.dll, vcruntime140.dll, and vcruntime140_1.dll
alongside Travis.exe. Windows' DLL search order looks in the
executable's directory first, so it finds them without any system
install. No admin prompt, no vc_redist download, adds ~620KB to
the installer.

Implementation:
- Release CI (Windows only) copies the DLLs from the runner's
  Visual Studio install into `src-tauri/resources/vc/`.
- `tauri.windows.conf.json` overlay adds `resources/vc/*.dll` to
  `bundle.resources` and wires `installerHooks` to
  `windows/installer.nsh`.
- `src-tauri/windows/installer.nsh` defines `NSIS_HOOK_POSTINSTALL`
  that `CopyFiles` from `$INSTDIR\resources\vc\` up to `$INSTDIR`.
  `NSIS_HOOK_POSTUNINSTALL` cleans them out on uninstall.

### 2. Persistent voice audio card (survives canvas mount race)

The v0.28.61 InlineVoiceBubble fix was structurally correct code but
never got mounted for the case it was written for: when the user
speaks into VoiceCanvas, speech-end sets `activity="thinking"` which
flips `useCanvasMode` from voice → chat, which unmounts VoiceCanvas
and mounts a **fresh** ChatCanvas. The fresh mount's useRef audio
snapshot starts null; by then `pendingVoiceAudio` had already been
cleared by the `journal://user-inserted` listener. Result: no audio
card, exactly what the user kept reporting.

Fix: stop relying on a component-local ref. Keep `pendingVoiceAudio`
alive in the store until the real DB message it was linked to shows
up in the current thread, then clear both. Any fresh ChatCanvas
mount just reads the live store value.

Also added: `voiceAudioLinkedMessageId` store field, set by
`useNativeVoice` when the journal user-insert event carries the DB
row id. ChatCanvas watches for that id in `allMessages` and clears
the persistent state at the handoff — the real message's
VoiceMessageCard takes over rendering from the in-flight card so
they don't double-render.

**Not fixed here:** the "several minutes" wait for the assistant
reply. That's the LLM turn itself, and the real fix is streaming +
split-emit — where the user message emits as its own event
immediately, then assistant tokens stream in progressively. Coming
in v0.28.64.

## v0.28.61 — Voice UX honesty (2026-07-16)

Three user-reported bugs, each with a real root cause that patches from
v0.28.57–.60 had missed. Not incremental polish — actual state-machine
mistakes.

### 1. Spheroid popping mid-thinking / audio recording again mid-flight

Root cause: `setArmed(false)` and `intentArmedRef=false` sat in the
speech-end handler's `finally` block, which only fires AFTER whisper
finalize awaits. That kept Rust armed and the intent-flag true for
~700ms during transcription. Any ambient noise in that window tripped
VAD, Rust emitted `speech-start` with `armed=true`, the frontend
handler saw `intentArmedRef=true`, and flipped `activity="listening"`
— which is exactly the "spheroid randomly pops mid-thinking" symptom
users kept reporting.

Fix: disarm IMMEDIATELY at the top of the speech-end handler, before
we await whisper. `setArmed(false)` doesn't drain the utterance
buffer (that's `TakeUtterance`'s job), so it's safe to disarm first
and then finalize. Race window closes to zero.

### 2. Audio card doesn't appear until the LLM finishes

Root cause: the optimistic user bubble in ChatCanvas rendered
text-only. The audio player only appeared when the real message row
came back from `journalIngest`, which meant users waited the full
5–15s LLM turn before seeing the audio bubble.

Fix: `useOptimisticSubmit` now snapshots `pendingVoiceAudio` the
moment the composer submit fires and keeps that snapshot until the
real message replaces the bubble. A new `InlineVoiceBubble` component
renders the same play button + waveform + duration + transcript as
`VoiceMessageCard`, but takes props directly (no DB fetch). Result:
audio player appears the instant recording ends, transcript renders
inline, both persist until the DB round-trip completes.

### 3. The "..." thinking bubble

Removed. Users called it out as a permanent eyesore — the composer
already has its own thinking-state indicator (glow border + spinner,
shipped in v0.28.44), so the assistant-side dots were redundant.
When streaming lands (v0.28.62), tokens will fill the assistant
bubble progressively, so there's no "waiting" state to visualize.

## v0.28.60 — Speculative whisper prewarm (2026-07-15)

Speculative-transcription for voice: while VAD is still counting down
its hangover after you stop speaking, whisper runs in parallel on the
audio it already has. By the time `speech-end` fires 1500ms later,
the transcript is already computed and finalize just hands it back.

Saves 500-1000ms per voice turn — roughly the full whisper inference
cost is now hidden behind the VAD hangover instead of stacking on top.
Combined with prior v0.28.59 (tiny.en model + threads capped + Metal
on macOS + hangover 2500→1500ms), post-utterance latency drops from
the original ~2900ms to ~700ms on typical hardware.

**How it works.** New `voice://speech-pausing` event fires from
Rust at the VAD Speech→ProbablySilence edge (i.e. the instant you
start pausing). Frontend `useNativeVoice` listens and invokes a new
`voice_prewarm_transcript` command, which peeks (doesn't drain) the
current utterance and spawns a background whisper inference. The
result is stashed in `VoiceState.prewarm`. When `speech-end` lands
and `voice_finalize_transcript` runs, it checks the prewarm cache:
if the cached transcript is <3s old and was computed on essentially
the current utterance (within 0.5s of samples), reuse it. Otherwise
fall back to a fresh inference. Stale-prewarm case (user resumed
speaking) costs one wasted whisper run per turn, which the tick
loop absorbs since prewarm runs on a separate tokio task.

**Refactor along the way:** extracted `run_whisper_blocking` so
finalize + prewarm share exactly one code path. Zero duplication;
easier to add a real streaming variant later.

**Not in this release (deferred to v0.28.61):** streaming LLM
responses. That's the biggest remaining perceived-wait fix — the
LLM turn (5-15s for a full Claude response) still lands as one
block at the end. Streaming needs its own dedicated pass because it
changes the trait signature, all four provider impls, `journal_ingest`,
and the frontend chat renderer. Rushing it here would break more
than it fixes.

## v0.28.59 — Voice: real architectural fixes (not patches) (2026-07-15)

Prior releases patched around symptoms; this one addresses the
actual structural issues driving the "glitchy voice" report. Users
described (a) enabling "Hey Jarvis" broke normal mic capture, (b)
ambient TV/phone-video audio kept hijacking in-flight turns, (c) TTS
played over the splash while the user couldn't see the conversation.
Each maps to a real architecture problem that patches couldn't fix:

**Wake inference moved off the tick loop.** Previously the openWakeWord
three-stage ONNX chain ran inline in the same 30ms tick that owned
VAD + amplitude + utterance capture. Every 80ms of audio triggered
three inferences; on slower machines those ate the tick budget, cpal
kept filling the buffer, and our "wake buffer cap" started dropping
user audio to prevent unbounded growth — which is exactly why "Hey
Jarvis on = voice stops working". New architecture: dedicated
`travis-wake` worker thread with a bounded 32-frame channel. Tick
loop does NO inference. If the worker stalls it drops queued frames
silently; VAD + utterance capture stay real-time.

**Wake pauses during in-flight turns, not just gated on the
frontend.** Previously we only skipped the frontend `arm-voice`
dispatch when chatBusy — but the Rust wake worker still ran
inference on every ambient frame, spending CPU + being one race
away from false-positive interference. New: a `SetWakePaused` cmd
freezes the worker while `chatBusy || activity==="thinking" ||
activity==="speaking"`. Wake worker stays loaded (resume is <1ms),
just skips inference. Frontend wake listener + `onArm` also both
hard-reject in busy states as belt-and-braces.

**Fresh reply dismisses the splash.** Splash was showing over
newly-arrived assistant replies when the inactivity clock had ticked
past 5 min during the LLM turn. Fix: `useNativeVoice` bumps
`activityBeat` on `chatBusy` true→false transitions — any completing
turn (voice or text) exits idle mode so the user actually sees what
Travis said.

**Whisper optimizations (per Gemini's suggestions):**
- Default model switched from `ggml-base.en.bin` (74 MB) to
  `ggml-tiny.en.bin` (39 MB). Whisper is not the bottleneck of the
  overall wait — the LLM turn is — but 2-3x faster STT still shaves
  ~500-1000ms off every voice submit. For command-length utterances
  tiny.en is within a rounding error of base.
- `set_n_threads(min(cores, 4))` — beyond 4 threads whisper hits
  diminishing returns and starts stealing CPU from the UI thread.
- Metal backend enabled on macOS via a target-conditional feature
  merge on `whisper-rs`. Windows/Linux stay CPU-only for now
  (CUDA/Vulkan need toolkits our CI doesn't have).

**VAD hangover 2500ms → 1500ms.** Post-utterance dead time was
"glacial." 1500ms still covers a natural end-of-sentence beat
(median end-of-utterance pause ~800ms in dictation studies) while
shaving a whole second off every voice turn. Combined with tiny.en
+ threads: time from "user stops speaking" to "LLM turn starts"
drops from ~2900ms to ~1700ms.

**Not in this release (dedicated follow-up, task #385):** streaming
whisper (transcript builds during speech, final flush ~100ms) and
streaming LLM responses (progressive token render as the model
generates). Together those two are the biggest remaining perceived-
wait fixes — they need their own careful pass because both change
fundamental Rust↔frontend event contracts.

## v0.28.58 — Wake word + 3 voice-flow bugs + history-splash-dismiss (2026-07-15)

Three specific voice bugs the user hit, plus the openWakeWord shipment
that was already in flight. All in one release because they all touch
the same pipeline and shipping them separately would just churn CI.

### Voice-flow bugs (root-cause fixes)

**1. Spheroid pops randomly after a previous mic click.** The
speech-end handler in `useNativeVoice` only reset `intentArmedRef`
and called `nativeVoice.setArmed(false)` on the "non-empty transcript
succeeded" branch. Empty transcript or whisper failure left Rust
armed AND `intentArmedRef=true` — so any later VAD spike (someone
coughs, HVAC starts) fired `voice://speech-start` with
armed_for_submit=true, which flipped `activity="listening"` and
popped the spheroid. Fix: disarm on every exit path via `finally`.

**2. Splash appears whilst the LLM turn is still running.** The
inactivity clock in `useCanvasMode` didn't check whether a turn was
in flight — after ~5 min it just decided the user was idle and
promoted the mode to `idle`, painting the greeting splash over the
live spinner. Fix: `busy = chatBusy || activity==="thinking" ||
voiceTranscribing` blocks the idle transition.

**3. TTS plays into another app when you alt-tab.** ChatTurn called
`mod.speak()` unconditionally when `speakNextResponse` was set.
User alt-tabbed during a slow turn; Travis then read the reply out
loud into whatever they'd switched to. Fix: `document.hasFocus()`
short-circuits the speak path — the reply still renders + the audio
card affordance means it's replayable on return if wanted.

### Wake word ("Hey Jarvis") via openWakeWord

Closes the wake-word gap the v0.28.57 explicit-only policy created.
Say **"Hey Jarvis"** and the mic opens — same downstream flow as a
mic click. Off by default (Settings → Wake word). CPU cost is ~1%
running the ONNX chain continuously vs the ~10-15% ambient-whisper
path we killed in v0.28.57. "Hey Jarvis" is the placeholder wake
phrase because openWakeWord doesn't ship a pre-trained "Hey Travis"
model; a custom-trained one is task #384 and swaps in as a single
`.onnx` file with no other code changes.

- `voice::wake` — three-stage pipeline via tract-onnx (pure Rust,
  no native ORT binary):
    raw f32 mono 16kHz → `melspectrogram.onnx` → mel features
    → `embedding_model.onnx` → 96-d embeddings
    → `hey_jarvis_v0.1.onnx` → confidence
  Rolling buffers keep the last 10 mel chunks + 16 embeddings; a
  12-slot smoothed detection buffer + 2s refractory prevents a
  single utterance from retriggering.
- Ported the pipeline from the MIT-licensed `oww-rs` crate — we
  couldn't drop-in `oww-rs` because it owns its own cpal stream and
  our `voice::capture` already does.
- New deps: `tract-onnx 0.21`, `circular-buffer 1`.
- `capture::tick_loop` feeds the wake detector 1280-sample (80ms)
  chunks alongside VAD/amplitude. Wake runs independently of the
  intent-arm state.
- New commands: `voice_set_wake_enabled(on)` (persists to meta),
  `voice_wake_enabled()`. `voice_start` restores the persisted
  wake state on launch.
- Frontend: `useNativeVoice` listens for `voice://wake-detected`
  and dispatches `travis:arm-voice` — same event the mic button
  fires. New `WakeWordSection` in Settings (opt-in toggle).
- `scripts/fetch-openwakeword.mjs` downloads the three ONNX files
  from openWakeWord v0.5.1 GitHub Releases (~3.7 MB total) into
  `src-tauri/resources/wake/` on `predev` + `prebuild`.

### Splash dismisses on history-item click

`useInactivityTick`'s local `lastActiveRef` only reset on `keydown`.
Non-keyboard activity paths — history-item click, resume chip, mic
press — called the store's `noteUserActivity()` but that never
propagated to the hook. Added an `activityBeat: number` counter to
the store, bumped on every `noteUserActivity()`; the hook subscribes
to it and mirrors any bump into the local ref. Splash now dismisses
on every real activity path.

## v0.28.57 — Voice: explicit-only capture + instant convo/audio (2026-07-15)

Closes the wake-word gap the v0.28.57 explicit-only policy created.
Say **"Hey Jarvis"** and the mic opens — same downstream flow as a
mic click. Off by default (Settings → Wake word). CPU cost is ~1%
running the ONNX chain continuously vs the ~10-15% ambient-whisper
path we killed in v0.28.57.

Wake phrase is "Hey Jarvis" as a placeholder — openWakeWord doesn't
ship a pre-trained "Hey Travis" model. Custom-training one needs
a real dataset + training run and belongs in a future release; the
runtime here is identical, only the wake ONNX file swaps.

### Rust

- `voice::wake` — three-stage openWakeWord pipeline via tract-onnx
  (pure Rust, no native ORT bundle):
    raw f32 mono 16kHz → `melspectrogram.onnx` → mel frames
    → `embedding_model.onnx` → 96-d embeddings
    → `hey_jarvis_v0.1.onnx` → confidence
  Rolling buffers keep the last 10 mel chunks + 16 embeddings; a
  12-slot smoothed detection buffer + 2s refractory prevents a
  single utterance from retriggering.
- Ported the pipeline from the MIT-licensed `oww-rs` crate — we
  couldn't drop-in `oww-rs` because it owns its own cpal stream and
  Travis's `voice::capture` already owns the mic.
- New deps: `tract-onnx 0.21` (pure-Rust ONNX runtime),
  `circular-buffer 1`.
- `capture::tick_loop` now feeds the wake detector 1280-sample
  (80ms) chunks alongside the existing VAD/amplitude pipeline. Wake
  runs independently of the intent-arm state so the user can wake
  the mic even between armed captures.
- New commands: `voice_set_wake_enabled(on)` (persists to
  `meta['voice.wake.enabled']`), `voice_wake_enabled()`.
  `voice_start` restores the persisted wake state on launch.

### Frontend

- `useNativeVoice` listens for `voice://wake-detected` and
  dispatches `travis:arm-voice` — the exact same event the mic
  button fires. Downstream flow is identical: no separate wake
  code path.
- New `WakeWordSection` in Settings (opt-in toggle, clear phrase
  copy). Explains the CPU cost + the "Hey Jarvis" placeholder.

### Build

- `scripts/fetch-openwakeword.mjs` downloads the three ONNX files
  from openWakeWord v0.5.1 GitHub Releases into
  `src-tauri/resources/wake/` on `predev` + `prebuild`. Total ~3.7 MB
  bundled per installer. Skipped if already present.

## v0.28.57 — Voice: explicit-only capture + instant convo/audio (2026-07-15)

Reported: after voice submit, users waited "many seconds" before the
conversation and audio card appeared, and the mic was capturing
background sounds when the user hadn't asked it to.

New contract: **voice capture ONLY starts on explicit user intent** —
mic button click OR the wake shortcut (Ctrl+Alt+Space). Nothing else
opens the pipeline. Ambient continuous transcription and the
post-TTS auto-arm window are both removed — those were the sources
of "spheroid appears when nobody's talking" behaviour, because they
kept Rust armed and whisper'd every VAD-bounded utterance.

**1. Explicit-only capture.**
- `speech-end` handler is intent-only. If the user didn't press the
  mic (or hit the wake shortcut), the utterance is not saved, not
  transcribed, not submitted. Same guarantee applies to the wake
  shortcut path — it fires an `travis:arm-voice` alongside surfacing
  the window so the mic actually opens.
- `travis:auto-arm-mic` (the 3-6s "reply within a beat" window after
  Travis speaks) is removed entirely. `ChatTurn` no longer dispatches
  it; the useNativeVoice listener is gone.
- Ambient-listening-arms-Rust effect is removed. The `ambientListening`
  toggle stays as store state (other consumers still read it), but it
  no longer streams every VAD segment through whisper. A follow-up
  release will layer a real audio wake-word engine (openWakeWord) so
  "Hey Travis" works without continuous transcription.

**2. Instant convo + audio card after submit.** `journal_ingest`
already inserts the user's row before the (slow) LLM turn begins —
we just weren't telling the frontend until the LLM finished. New
`journal://user-inserted` Tauri event fires the moment the user
row lands (`{ conversationId, userMessageId, content }`). The voice
hook picks it up and immediately: sets `activeConversationId` (so
the canvas mounts, history switcher updates), and calls
`voice_utterance_link` with the real message id (so the audio card
renders on the user bubble). Assistant reply still streams in when
the LLM completes; the user just doesn't stare at an empty screen
in the meantime.

**3. VAD threshold raised.** `VAD_SPEECH_RMS` 0.008 → 0.014,
`VAD_SILENCE_RMS` 0.004 → 0.007. Even inside an intent capture the
old floor overlapped the office-ambient band (HVAC startup,
next-room conversation) and was tripping speech-start on non-speech.
Desk-distance speech tests at 0.03-0.08 RMS, well above the new
threshold.

## v0.28.56 — Hotfix: libgbm-dev for Linux linker (2026-07-14)

v0.28.55 got past PipeWire's struct mismatch by moving to
ubuntu-24.04, but the linker then errored `unable to find library
-lgbm`. Mesa's Generic Buffer Manager gets pulled by the newer
xcap/PipeWire path. Both workflows now install `libgbm-dev`
alongside the other deps. macOS + Windows already succeeded on
v0.28.55, so this is the last Linux hurdle. No product changes.

## v0.28.55 — Hotfix: Linux runner → ubuntu-24.04 (2026-07-14)

v0.28.54 installed libpipewire-0.3-dev but ubuntu-22.04 ships
PipeWire 0.3.48 which is older than what `libspa-sys 0.9.2`
expects — `spa_video_info_raw.flags` doesn't exist on the older
struct, so xcap fails to build. Both workflows now use
ubuntu-24.04 (PipeWire 1.0.5, everything else already available).
No product changes vs. v0.28.54.

## v0.28.54 — Hotfix: PipeWire deps for xcap 0.9 on Linux CI (2026-07-14)

v0.28.53 shipped with xcap 0.9 which transitively pulls PipeWire on
Linux for Wayland screen capture. CI ran into `libspa-sys` failing
at `pkg-config --libs --cflags libpipewire-0.3`. Both workflows now
install `libpipewire-0.3-dev` + `libclang-dev` + `clang` alongside
the existing deps so the build reaches the Rust toolchain unblocked.
No product changes vs. v0.28.53.

## v0.28.53 — Sentry cloud sync + T2T secure file transfer (2026-07-14)

Two things ship together:

**1. Sentry snapshots reach the cloud.** The v0.28.52 local rolling
window now uploads each fresh JPEG to `POST /me/telemetry/snapshot`.
Server writes ciphertext (well, plain JPEG — encryption of user's own
data-at-rest lives in R2's server-side crypto) to R2 at
`sentry/<userId>/<snapId>.jpg`, tracks it in the new `sentry_snapshot`
D1 table, and prunes each user's rolling window to 20 on every upload.
Orbit gains a per-user gallery card (`OrbitUserDetail` →
`UserSentrySnapshots`) so the founder can see what a consented
pre-commercialization beta user is actually working on — the whole
reason Sentry exists.

**2. Travis-to-Travis file transfer, end-to-end encrypted.** Every
desktop mints a persistent X25519 identity keypair (private half in
the OS keyring, public half published to `POST /t2t/me/keys/x25519`).
When you send a file to a paired peer:

- We fetch their static pubkey (only permitted if there's an active
  relationship), mint a fresh ephemeral X25519 keypair, ECDH, derive
  a session key via HKDF-SHA256 with the transfer id as salt,
  ChaCha20-Poly1305 the whole file, upload the ciphertext + our
  ephemeral pubkey. The Worker never sees the plaintext or the key.
- Recipient polls `GET /t2t/files/inbox`, downloads the ciphertext,
  mirrors the ECDH with their static privkey + the sender's ephemeral
  pubkey, decrypts. On success they POST `/t2t/files/:id/ack` and the
  Worker deletes the R2 blob.
- 25 MB per transfer, 7-day server-side TTL, tamper detection via
  the Poly1305 tag.

### New Rust crates

- `xcap` bumped to `0.9` — 0.0.14 had a type-inference error
  (E0282) on aarch64-apple-darwin that killed the v0.28.52 macOS
  build. 0.9 also has the current maintained API.
- `x25519-dalek` (with `static_secrets`), `hkdf`, `chacha20poly1305`,
  `rand_core`, `zeroize`, `hex` — production AEAD primitives.

### Rust

- `crypto::` — X25519 keypair generation + keyring persistence,
  static-ephemeral ECDH, HKDF-SHA256 session-key derivation, a
  ChaCha20-Poly1305 seal/open pair keyed by the transfer id. Two
  unit tests cover a full roundtrip and tamper detection.
- `t2t_transfer::` — thin async layer over the crypto module +
  cloud endpoints: `publish_my_pubkey`, `send_file`, `poll_inbox`,
  `download_and_decrypt`. Downloaded plaintext lands in
  `<app_data>/t2t-inbox/`.
- `sentry::try_upload_latest` — after each `capture_and_prune` the
  newest local JPEG is best-effort uploaded to
  `/me/telemetry/snapshot`. 403 (consent revoked server-side) is
  quiet; other failures log a warning.
- `sentry::set_cloud_consent` — turning Sentry on now also grants
  `app_window` + `screen` consents server-side so the ingest
  endpoints accept our writes. Turning it off revokes both.

### Cloud (travis-cloud)

- Migration `0058_sentry_snapshot.sql` — id, user_id, r2_key,
  captured_at, uploaded_at, bytes, width, height.
- `POST /me/telemetry/snapshot?captured_at&width&height` — accepts
  a raw JPEG body up to 3 MB, requires an active `screen` consent,
  writes R2 + D1, then prunes the user's rolling window to 20.
- `POST /me/telemetry/delete-all` also purges `sentry_snapshot`
  rows + R2 blobs so the existing user-facing purge button covers
  the new surface without a second button.
- Migration `0059_t2t_file_transfer.sql` — `t2t_file_transfer` +
  `user_pubkey_x25519` tables.
- `POST /t2t/me/keys/x25519` — upsert the sender's static pubkey.
- `GET /t2t/users/:id/pubkey/x25519` — relationship-gated read of a
  peer's pubkey.
- `POST /t2t/files/send` — enqueue a ciphertext transfer with the
  sender's ephemeral pubkey attached. Relationship-gated.
- `GET /t2t/files/inbox`, `GET /t2t/files/:id/download` (streams
  ciphertext with `x-travis-ephem-pub` header), `POST
  /t2t/files/:id/ack` (marks delivered + deletes the R2 blob).
- `GET /admin/users/:id/sentry-snapshots` + `GET
  /admin/sentry-snapshot/:sid` — orbit-side inspection endpoints
  (admin-role gated).

### Frontend

- `ContactsOverlay` — Send-file buttons now open the OS file
  picker + call the real T2T encrypted transfer, showing progress
  in the flash strip. A background inbox poll (4s cadence) picks
  up incoming files, decrypts them, and toasts the recipient.
- `orbit-api.ts` gains `fetchUserSentrySnapshots` (returns
  same-origin URLs pointing at the admin JPEG streamer).
- New `UserSentrySnapshots` orbit card — three-column gallery of
  the target user's recent screenshots.

### On BLE peripheral advertise

btleplug 0.11 is central-role only across all three OSes. True
peripheral needs per-platform work (`objc2-core-bluetooth` on macOS,
`windows` crate GATT publisher on Windows, `bluer` on Linux) —
each is its own multi-day implementation. Rather than ship a
half-working per-OS stub, this release commits fully to the T2T
cloud transport (encrypted end-to-end, works today, cross-platform
by construction). BLE peripheral becomes a dedicated future
release once each OS's server-role crate is wired.

## v0.28.52 — Sentry screenshots (local rolling window) (2026-07-14)

Sentry graduates from window-metadata-only to actual visual context.
Every ~5 minutes while enabled, Travis captures a JPEG of the primary
monitor, resizes to 1600px long-side, and drops it in the app data
directory. Rolling window of 20 keeps the on-disk footprint bounded.
Nothing leaves the machine in this release — cloud upload is a
separate opt-in shipping in v0.28.5x.

### Rust

- New deps `xcap = "0.0.14"` + `image = "0.25"` (JPEG feature only) —
  cross-platform monitor capture (GDI on Windows, CoreGraphics on
  macOS, xcb on Linux X11) plus a slim JPEG encoder.
- `sentry::capture_and_prune(dir)` grabs the primary monitor, resizes
  proportionally to fit within 1600×1600, encodes JPEG @ quality 80,
  writes `sentry-YYYYMMDD-HHMMSS.jpg`, then prunes older files back
  to the 20-most-recent budget.
- Sample loop tracks a screenshot counter and fires `capture_and_prune`
  every 10 ticks (`5 min` at the existing 30s cadence). Capture runs
  inside `tokio::task::spawn_blocking` so xcap's synchronous OS calls
  never stall the window-sampling driver.
- `SentryState::new` now takes a `snapshot_dir` — resolved at startup
  to `<app_data>/sentry-snapshots/`, auto-created on first capture.
- New commands: `sentry_list_snapshots(limit)` returns the newest N
  files as `{ path, filename, captured_at, bytes }`, and
  `sentry_capture_now()` fires a synchronous capture (respects the
  local enabled flag so it can't be used to bypass consent).
- `sentry_status` now returns `snapshot_count` + `snapshot_bytes` so
  the Settings UI can show live storage usage.

### Frontend

- `SentrySection` gains a live snapshot readout (count + total bytes),
  a 3-column gallery of the last 6 thumbnails (asset-protocol paths
  via `convertFileSrc`), and a "capture now" button. Thumbnails link
  out to the full file in the OS default viewer. Storage stats
  refresh every 15s while Settings is open.
- `SentryConsentModal` bumped to `SENTRY_CONSENT_VERSION = 2` — the
  scope changed from "app names only" to "app names + local
  screenshots", so every existing user re-consents before the next
  capture starts. Copy updated: screenshots are described as they
  actually behave (local-only, rolling window of 20, upload requires
  future re-consent).

## v0.28.51 — Real BLE central-role scan via btleplug (2026-07-14)

Cutting the scaffold. Central-role Bluetooth LE scan is now live —
Travis actually looks for peers advertising the Travis service UUID
on every OS it ships to. Peripheral-role (advertise) is deliberately
still a no-op because btleplug doesn't cover it consistently across
platforms; that ships in v0.28.52 layered on top of this.

### Rust

- New dep `btleplug = "0.11"` — cross-platform BLE central via
  BlueZ (Linux) / CoreBluetooth (macOS) / WinRT (Windows).
- `src-tauri/src/ble/mod.rs` rewritten: `ensure_scanner()` spins a
  singleton tokio task on first frontend call. That task opens the
  default adapter, starts a scan filtered to the Travis service
  UUID (`550e8400-e29b-41d4-a716-446655440073`), and listens on
  the central event stream — every `DeviceDiscovered` /
  `DeviceUpdated` for a peripheral whose properties actually list
  the Travis service gets folded into a shared `RwLock<HashMap>`.
  Disconnects prune the entry.
- `scan_peers()` becomes async; reads a snapshot from the shared
  map. First call kicks off the scanner; subsequent calls return
  the current known set (updated every ~2s from the frontend
  poll). Any failure to open the adapter is logged but not
  surfaced — the frontend just sees an empty BLE list, which is
  the correct fallback on machines without Bluetooth.

### Platform bits

- **Linux**: `libdbus-1-dev` added to both `ci.yml` and
  `release.yml` build-deps (btleplug's Linux backend needs BlueZ
  which speaks D-Bus).
- **macOS**: `src-tauri/Info.plist` added with
  `NSBluetoothAlwaysUsageDescription` +
  `NSBluetoothPeripheralUsageDescription`. Without these
  CBCentralManager refuses to start. Tauri 2 auto-merges this
  file into the bundle's `Contents/Info.plist`. Copy is plain and
  honest ("Travis uses Bluetooth to find nearby Travises so you
  can share files…").
- **Windows**: nothing extra — WinRT doesn't require a manifest
  entry for a desktop app's BLE central use.

### What still hurts

Peripheral-role. btleplug won't advertise cross-platform (Linux
would need `bluer`, macOS partial via CoreBluetooth peripheral APIs,
Windows very limited). Until v0.28.52 layers a peripheral crate on
top, a Travis running v0.28.51 can *see* peers that advertise the
Travis service (e.g. same-network Travises using a companion
script) but can't be seen back over BLE. mDNS + circles + QR pair
still cover discovery in the meantime.

Verified locally with `npx tsc --noEmit`. Watching CI closely on
this push since it's the first native-crate addition since the
CI-blindness episode.


## v0.28.50 — Fix radar glitching (2026-07-14)

The v0.28.49 radar was glitching because of two `% 1` modulo wraps
that produced visible discontinuities every ~3s. Rewriting both
loops without the wrap.

**Rings.** The old loop had 4 fixed rings that "pulsed" using
`(t*0.35 + i*0.25) % 1`. Every 2.86s each ring's radius popped 4%
smaller and its alpha popped up by ~0.06. Replaced with 5
expanding-wave rings, staggered in phase, that grow from center to
edge over 4.2s. Fade-in ramp (0→1 over the first 6% of a ring's
life) + fade-out ramp (1→0 over the remainder) keeps both endpoints
at alpha 0 — the modulo wrap is no longer visible because there's
nothing to see when a wave restarts. HSL lightness now grows from
65% (near center) to 92% (near edge), so outer rings read as softer
echoes as they fan out.

**Peer blips.** The old afterglow was `rel < 0.5 ? 1 - rel/0.5 : 0`
— a hard cliff at 0.5 rad past the sweep. Every peer went from lit
to dark instantly. Replaced with `exp(-sinceSweep * 1.8)`, a smooth
exponential decay that lets peers glow brighter as the sweep
passes and fade continuously through the rest of the rotation. No
cliff, no pop.

**BLE (task #379) held for v0.28.51.** btleplug integration
(cross-platform advertise + scan) needs its own focused session
with libdbus-1-dev added to the CI workflow, macOS
NSBluetoothAlwaysUsageDescription in tauri.conf.json, and
Windows peripheral-role research. Not landing it half-baked given
the CI-blindness that ate v0.28.44 through v0.28.46.


## v0.28.49 — Radar-driven contacts + BLE scaffold (2026-07-14)

Full redesign of the Contacts overlay so the network reads as one
coherent surface instead of a button-per-thing menu. Plus scaffolding
for Bluetooth LE + secure file transfer that lands next release.

### The overlay is now four tabs, not five buttons + five modals

- **Discover** — animated radar screen. Concentric brand-purple
  rings, rotating sweep line (3.2s/rev), gradient center orb for
  "you". Every discovered peer sits at a hashed angle at a radius
  derived from signal strength (BLE) or a stable id hash (mDNS).
  Hover a blip → tooltip with name + relationship state. Click →
  invite. A plain list below the radar mirrors the blips so peers
  stay accessible without hunting the dots.
- **Circles** — inline "create" (brand purple) + "join" (teal)
  cards at the top of the tab replace the old modal chrome. Circle
  rows below with expand-for-members + copy-code + leave/delete.
- **Contacts** — invite-by-email form inline at the top; three
  buckets (Waiting for you / Connected / Sent invites) below.
  Connected rows gain a "send file" affordance (see BLE scaffold).
- **Pair** — QR + 8-char code on the left, redemption input on
  the right. Deep link `travis://pair?tok=…` auto-switches to this
  tab and auto-redeems. Issue a new code on demand.

Tab bar uses a `layoutId` motion pill so switching cross-fades
smoothly and the selection feels physical, not clicky.

### Unified peer feed across discovery sources

New `UnifiedPeer` shape merges mDNS (`discovery_peers`) and BLE
(`ble_scan_peers`) into a single feed. Every peer carries its
`source` + `rssi` + `relationship` state so the radar can pick a
sensible radius (closer = higher signal), color connections in
brand-green instead of purple, and label the "connected · click to
send file" state instead of "invite".

### BLE + file-transfer scaffold (real impl in v0.28.50)

New Rust module `src-tauri/src/ble/` declares the Travis GATT
service the frontend will hit once `btleplug` is added in the next
release:

- Service UUID: `550e8400-e29b-41d4-a716-446655440073`
- Identity characteristic (read): JSON `{ user_id, display_name,
  public_key }` — Curve25519 identity
- Handshake characteristic (write + notify): raw X25519 ephemeral
  keys either direction → HKDF → ChaCha20-Poly1305 session key
- File chunk characteristic (write + notify): framed
  `{ seq, len, ciphertext, tag }` — same framing used by the T2T
  cloud-relayed transfer path so a Travis can send a file to
  another Travis regardless of which transport is available

Tauri commands `ble_scan_peers` / `ble_start_advertise` /
`ble_send_file` are wired end-to-end today; scan returns `[]`,
advertise no-ops, `send_file` returns the friendly "coming in
v0.28.50" message that the UI toasts as a flash. Adding btleplug +
the real GATT impl in v0.28.50 will not require another
architectural pass on the module or the UI.

### Verified locally with `npx tsc --noEmit` before push

The CI-blindness episode that ate v0.28.44 through v0.28.46 taught
me to actually run the check that the failing pipeline was running.
Doing that now every release.


## v0.28.48 — Circles: named groups for beyond-LAN discovery (2026-07-14)

Everyone in the same circle is auto-discoverable to each other — no
separate T2T invite needed. Works over any network since it's cloud-
brokered. Ships alongside the v0.28.46 QR pair flow as the second
beyond-LAN discovery mechanism.

### Cloud

- Migration `0057_circles.sql` — new `circle` + `circle_member`
  tables. Circle rows carry a unique 8-character join code (alphabet
  excludes 0/O/1/I for readability).
- Routes at `/circles`:
  - `POST /circles` — create (creator becomes owner + first member)
  - `GET /circles` — list mine with member counts and my role
  - `POST /circles/join` — join by code (idempotent)
  - `POST /circles/:id/leave` — leave; last member auto-deletes the
    circle
  - `GET /circles/:id/members` — members of a circle I'm in
  - `GET /circles/contacts` — dedup'd list of everyone in every
    circle I'm in (feeds the "In your circles" row)
  - `DELETE /circles/:id` — owner-only teardown

### Desktop

- `src-tauri/src/cloud/circles.rs` + `circles_cmd.rs`: HTTP clients
  and Tauri commands (`circles_create` / `_list` / `_join` / `_leave`
  / `_members` / `_contacts` / `_delete`).
- `ContactsOverlay` gets two new sections:
  - **Your circles** — collapsible rows with name, description,
    role, member count, join code (click to copy), and a leave (or
    delete for owners) button. Expanding a row loads and lists
    members.
  - **In your circles** — flat list of every distinct person you
    share at least one circle with, avatars + name.
- Two new modals reached from the action row:
  - **Create circle** — name + optional description → gets you back
    a shareable join code.
  - **Join circle** — 8-char code input, auto-uppercase, Enter to
    submit.
- Older cloud deployments (without `/circles`) fall through
  silently — the UI just doesn't render the two new sections.


## v0.28.47 — Unblock the CI pipeline (2026-07-14)

**None of v0.28.44 / v0.28.45 / v0.28.46 actually shipped to users.**
The Release CI has been silently failing on the same TypeScript
error since v0.28.44 was tagged — the `PathSourceBadge` component
was orphaned when its usage moved into the top-of-map overlay chip
stack (v0.28.44), but the function stayed in `MapCanvas.tsx`.
`tsc --noEmit` failed on TS6133 ("declared but never read"), which
runs both in the Lightweight CI workflow and inside the Release
workflow's `npm run build` step (`tsc && vite build`), so all
three platform installers refused to build for three consecutive
releases. Last shipped installer is v0.28.43.

Fix: delete the dead `PathSourceBadge` function. Verified locally
with `npx tsc --noEmit` before pushing this time.

Once this build passes, users receive **the accumulated changes
from v0.28.44 + v0.28.45 + v0.28.46 + this fix**, all at once.


## v0.28.46 — QR / deep-link pair (beyond-LAN Travis discovery) (2026-07-14)

Pair with another Travis over any channel, not just the LAN. First
of the "beyond-LAN discovery" tranche; cloud rendezvous, circles/org
auto-discovery, and Bluetooth LE proximity follow in v0.28.47+.

### Cloud (travis-cloud)

Two new endpoints on `/t2t/pair/*`:

- `POST /t2t/pair/token` — issue a fresh 8-character pair code for
  the current user (alphabet excludes ambiguous 0/O/1/I). Stored in
  KV with a 24-hour TTL. Response includes the code, expiry, and a
  deep-link URL (`travis://pair?tok=…`).
- `POST /t2t/pair/redeem` — consume a token, create two active
  relationship rows in both directions so the two Travises can
  immediately message each other. Idempotent for the redeemer. Skips
  the pending step since both sides have explicitly consented
  (issuer by creating the token, redeemer by entering it).

### Desktop

- `src-tauri/src/cloud/t2t.rs` + `t2t_cmd.rs`: `create_pair_token` +
  `redeem_pair_token` HTTP clients and their Tauri commands
  (`t2t_pair_create_token`, `t2t_pair_redeem`).
- `src-tauri/src/lib.rs`: new `travis://pair?tok=…` deep-link
  handler. Extracts + sanitizes the token to A-Z/2-9, brings the main
  window forward, dispatches a `travis://pair` window event with the
  token.
- `WorkspaceV2`: window-level listener for the deep-link event. On
  fire, stashes the token in the new `pendingPairToken` store field
  and opens the contacts overlay. This means a `travis://pair` URL
  works whether Travis is already running, minimized, or cold-booting
  from a browser click.
- `ContactsOverlay`: three actions row in the footer — **Share pair
  code**, **Enter pair code**, and **Invite by email**. Both pair
  actions open sleek modals.
  - **Share** issues a token immediately, renders a QR (encoding the
    deep link), shows the plain-text 8-char code below in a clickable
    button that copies it, and a secondary button to copy the deep
    link. Auto-refreshes on next open.
  - **Enter** shows a single centered 8-char input; auto-uppercase,
    focus + Enter submits. If a `pendingPairToken` shows up from the
    store, the modal pre-fills + auto-redeems.
- Success flashes surface "Paired with $name" once relationships
  refresh.

### Dependencies

- `qrcode` + `@types/qrcode` — client-side QR rendering into a
  data-URL PNG. No image round-trip.

### What's still coming

- Cloud rendezvous UI (surface "beyond your network" matches) —
  v0.28.47
- Circles + Org auto-discovery — v0.28.47
- Bluetooth LE proximity — v0.28.48
- Sentry Rust screenshot capture + cloud upload — v0.28.49


## v0.28.45 — Settings cleanup, switches, Travis contacts as its own flow, Sentry consent gate (2026-07-14)

Six-item settings + T2T + Sentry pass.

### 1. Shared Switch component replaces every checkbox

New `src/ui/Switch.tsx` — brand-purple gradient track when on, dark
track when off, framer-motion spring on the thumb (stiffness 480,
damping 34). Two sizes (`sm` / `md`), optional label + description,
`role="switch"` + keyboard toggle. All 10 checkbox sites swapped:

- Settings.tsx: Travis Cloud toggle, Speak replies, "Hey Travis"
  wake word, Actions permission, Proactive nudges, Cross-workspace
  visibility, Pack enable card, Include-sensitive on export
- lib/autoCRUD/FieldInput.tsx: bool field renderer
- chat/cards/SlotFormCard.tsx: bool slot input

No more `type="checkbox"` in the codebase.

### 2. Default packs hidden from Settings

Compiled-in packs (`lead-to-empower`, `tutoring`, `everyday`) are
always-on scaffolding, not user-tunable UI. Frontend hardcoded slug
set filters them out of `PacksSection` — only custom / add-on packs
remain toggleable. Backend `PackInfo` API unchanged.

### 3. Travis contacts extracted into its own canvas overlay

`ContactsOverlay.tsx` — discovery-first layout. mDNS-discovered peers
show up as AirDrop-style tiles (initials avatar + brand-purple pulse
ring) polled every 2s, plus your existing contacts (accepted +
incoming-pending with accept + outgoing-pending). "Invite by email"
is a secondary button that expands an inline form.

- Tap a nearby peer → sends a T2T invite using the email from their
  mDNS TXT record. If they didn't broadcast one, the email form
  opens with a hint.
- Peers who are already contacts render dimmed with a "connected"
  label instead of "invite".
- New store field `contactsOverlayOpen`, keyboard shortcut ⌘⇧C,
  dock row between Documents and Settings with a Contacts icon.
- Old `T2tSection` removed from Settings entirely (import dropped).

### 4. UI switcher button removed from canvas

The "Classic view" DockRow + its `SwapIcon` are gone from
`QuickAccessDock`. Surface switching still lives in Settings →
Interface — the canvas is not the place for a button that swaps out
the whole app.

### 5. Sentry consent gate + honest copy

`SentryConsentModal.tsx` — sleek dark card that opens when a user
tries to turn Sentry on for the first time. Structured blocks:

- What Travis captures (current + coming: screen snapshots)
- Why we're asking (Travis becomes useful in workflow, not just
  answers)
- Where it lives (local first, rolling window to your account)
- What Travis will NEVER capture (keystrokes, clipboard, password
  fields, Sensitive workspaces)
- Your controls (pause, purge, orb indicator)

The "I agree — turn Sentry on" button stays disabled until the user
scrolls to the bottom (soft attention gate). Accepted version stamped
in localStorage; a future scope expansion will re-prompt. Turning
Sentry OFF never asks.

`SentrySection` copy rewritten to remove the "no screen content"
promise since that scope is expanding. Actual Rust screenshot
capture + cloud upload lands in v0.28.46.

### 6. LTE-specific placeholder text removed

`T2tSection.tsx` invite note placeholder now reads "for planning our
trip / for the campaign" instead of the LTE-specific default. Only
user-facing LTE placeholder in the codebase — internal comments and
pack-scoped files are correctly named and untouched.


## v0.28.44 — Composer thinking state + map controls + backdrop polish + idle-orb fix (2026-07-13)

Seven coordinated UI upgrades tightening up the composer, map,
backdrop, and idle-orb behavior. (A three.js "digital universe"
backdrop was prototyped in this cycle but pulled before shipping —
didn't meet the bar. Coming back in a future release.)

### 1. Idle orb dismisses on keyboard only

The idle/opening screen was dismissing on any mouse motion — a
regression from the spec at `WorkspaceV2.tsx:144-147` which reads "mouse
events no longer dismiss the splash. Only keyboard input… and mic arm."
`useInactivityTick` (`useCanvasMode.ts:89`) was still listening on
`mousemove` + `mousedown`. Removed both listeners. Wake-word paths
already call `noteUserActivity()` directly, so voice engagement still
dismisses idle without needing a pointer listener.

### 2. Composer thinking state

While a turn is in flight the composer now sports a rotating
conic-gradient halo behind the input (2.6s per revolution, brand
purple → mint → amber → purple), disabled input, and a real rotating
spinner in the submit button (replacing the placeholder `...`). New
`<ThinkingGlow>` sub-component; sits under the composer surface with
`inset: -4` + `filter: blur(10px)` so what shows is color spilling
around the border, not a solid ring. Respects
`prefers-reduced-motion`.

### 3. Persistent input on immersive views

When Travis is thinking on the map / voice / idle canvas, the user's
submitted text now stays on screen as a floating chip above the
composer with a small spinner + a mode-aware status verb
(`consulting maps`, `on the call`, `on it`). Fades after chatBusy
flips false + a 1200ms dwell. Not rendered on chat view since the
turn already shows in the message stream. New `<PendingRequestChip>`
sub-component. Backed by a new `lastSubmittedText` field on the app
store (`stores/app.ts`) which the composer writes on submit.

### 4. More map controls

Right rail on the map canvas gets a control stack: zoom in/out, reset
compass to north, toggle pitch (flat ↔ 55° tilt), and fit-route (only
when a route is present). Each control uses `easeTo` with a soft cubic
so camera changes read as controlled motion rather than snaps.

### 5. Overlay chip stack at top of map

Every overlay currently on the map now surfaces as a chip in one row
pinned to the top-center of the canvas. Route source (real/llm/
straight/fetching), 3D terrain indicator, and one chip per
mapPart.overlay (heatmap / regions / circles / reach). Route chip
click flies to the route bounds. `PathSourceBadge` moved out of the
info card into this stack so all overlay identity lives in one place.

### 6. Nudge conversations hidden from history

Proactive nudges were creating regular `conversation` rows with
`kind='nudge'` and surfacing in the history switcher — clutter, and
the wrong lane for a one-shot OS notification. `list_for_switcher`
now filters `AND c.kind != 'nudge'` so nudges stay in the OS
notification / attention-strip lane. Existing nudge rows disappear
from the switcher without any migration.

### 7. Grid stands out + cloth deformation under the cursor

The mesh grid was so faint it barely read as a HUD reference. Now
it's canvas-drawn instead of SVG, with two densities: minor cells
every 80px at ~10% brand-purple opacity, plus major cells every
320px at ~22% + a small dot at each major intersection so the
surface has scale + structure.

The grid IS the cloth: vertices near the cursor are pulled toward
it with a smoothstep Gaussian falloff (radius 240px, max
displacement 34px), and lines are rasterized as polylines through
sample points every 20px so straight grid lines visibly bend — not
concentric ripples, actual local deformation of the mesh, like
stretched fabric magnetizing around a finger. The smoothed cursor
lerps at 0.16/frame and relaxes gently when the pointer leaves the
window instead of snapping back.

Reduced-motion suppresses the deformation (grid renders static).
No pointer events consumed — the canvas is `pointer-events: none`,
so composer / HUD / map controls keep working normally.


## v0.28.43 — Fix map marker displacement + surface path-fetch failure reason (2026-07-13)

Two coordinated map fixes carried by the same screenshot: the endpoint
markers were nowhere near their coordinates on a Lagos route, and the
badge kept reading `STRAIGHT` with no explanation.

### 1. Marker positioning (the "displaced markers" report)

Markers were pinned to the map container's origin, not their
geographic position. Root cause: `.travis-map-marker` carried the
`travis-marker-drop` CSS animation with `animation-fill-mode: both`.
MapLibre positions each marker by writing `element.style.transform =
translate(x, y)` inline every frame, but a CSS animation's declared
values override inline styles. When the drop-in animation ended, the
100% keyframe (`transform: translateY(0) scale(1)`) stuck via
`fill-mode: forwards`, and every subsequent MapLibre position update
got clobbered. Result: after 0.65s all markers snapped to translate(0,
0) of their offset parent, then hid behind the top-left overlay card.

Fix: introduce a `.travis-marker-inner` wrapper and move the drop-in
animation onto that. The marker root now has no `transform`
declaration at all, so MapLibre's inline transform is the only writer.

### 2. Badge no longer flashes STRAIGHT during a live fetch

When a route rendered, the `geometrySig` effect ran on the same tick
as the fetch effect and immediately committed the straight-line
fallback source, overwriting `loading` before the cloud fetch could
resolve. Users saw the badge jump straight to `STRAIGHT` even on
runs where the fetch would ultimately succeed — actively misleading.

Root cause was subtle: the sibling effect read its `isFetchingPath`
guard from the pre-update render's closure, where the flag was still
`false`. Fix uses a `useRef` (`isFetchingRef`) that's mutated
synchronously in the fetch effect body, so the sibling effect sees
the true in-flight state the same tick.

### 3. Path-fetch failures surface their reason in the card

When the fetch fails, the card now shows a small `path fetch: <reason>`
line under the badge (e.g. `not signed in`, `cloud maps returned 502`,
`no geometry in response`). No more silent STRAIGHT with no clue where
the pipeline broke.

Also directly commits `pathSource: "straight"` on fail/empty response
(previously the badge could get stuck on `loading` forever, since
`fetchedGeometry` staying null meant `geometrySig` never changed).

## v0.20.18 — Visual fidelity loop: sample-as-vision + verify_replication_match (2026-06-13)

Three coordinated changes that close the "Claude.ai's invoice is 95%
perfect, Travis's is 70%" gap. Side-by-side comparison the user
provided showed Travis matching "what an invoice looks like" instead
of "what THIS sample looks like" — specifically missing the double
underline rules, the solid blue "INVOICE" header text, "To:" vs
"Bill To:" phrasing, and empty-row preservation in the table. All
the things JSON descriptions from `analyze_document_styling` can't
capture.

### 1. Sample renders auto-attached as Claude vision

The `Message` struct grows an `images: Vec<MessageImage>` field
honored by the Claude provider as multimodal content blocks (`type:
image`, base64-encoded PNG). In `journal.rs`, the user message
builder now scans `inbound_doc_ids` for any doc classified as a
sample/template/po/wo/invoice/signed_sheet, looks up its
`page_render` template asset from Tier 4, and attaches the PNG bytes
as a vision block. Cap of 3 images per turn (~4500 tokens total —
sane budget).

Now the LLM SEES the sample while writing HTML instead of reading
JSON descriptions of it. That's the lever.

### 2. Stronger Path 1 bias + "replicate, don't normalize" prompt

The doc-generation hierarchy section in `journal.rs` now opens with
an explicit decision rule: "when a sample is attached, DEFAULT TO
PATH 1 (`replicate_from_sample`). The user has SHOWN you exactly
what they want — your job is to reproduce that geometry, not to
invent a layout that 'looks like an invoice.'"

Path 2 (`render_html_to_pdf`) gets a "replicate the sample's exact
decorative language" instruction — same label phrasing, same color
bands, same empty rows, same watermark style. No normalizing to a
generic template.

### 3. New tool: `verify_replication_match`

After Travis generates a doc, it can call:

```
verify_replication_match({generatedDocumentId, sampleDocumentId})
```

The tool renders both PDFs to 200-DPI PNGs via the bundled
pypdfium2, dispatches a multimodal chat call to the user's
configured provider with both images attached + a structured
comparison prompt, and returns a mismatch report. Each mismatch
includes WHAT differs, WHERE, and a concrete CSS fix. Ends with
`CLOSE_ENOUGH` (ship) or `NEEDS_REFINEMENT` (top 3 fixes).

Pattern: generate → verify → if NEEDS_REFINEMENT, edit the HTML
applying the fixes → re-render → verify again. 2-3 iterations
should converge.

### Combined effect

For the IS 217 invoice case from the user's side-by-side:
- Sample image is in the LLM's vision context throughout generation
  (was: only JSON descriptions). The double underlines, "INVOICE"
  font, "To:" phrasing, empty rows are all visible.
- Prompt biases toward `replicate_from_sample` rather than
  `render_html_to_pdf` because a sample exists.
- If the LLM uses `render_html_to_pdf` anyway and the result drifts,
  `verify_replication_match` surfaces the exact mismatches with CSS
  fixes — refinement loop becomes self-correcting.

Honest expectation: invoice fidelity goes from 70% to 90%+ on first
try, and convergence to 95%+ within 2-3 verify/edit cycles.

## v0.20.17 — render_html_to_pdf path placeholder hotfix (2026-06-13)

User dropped a transcript showing v0.20.16's `render_html_to_pdf`
firing but no `doc#N` card appearing. Travis kept reporting "the
file writes to /outputs but doesn't get a doc ID" — file existed
but the cmd's post-processing wasn't seeing it.

Root cause: brace-count mismatch in the driver script.
`format!("{{OUTPUTS_DIR}}/{}", output_name)` escapes to
`{OUTPUTS_DIR}/...` (SINGLE braces) in the JSON spec. But the
driver had:

```rust
SPEC['outputPath'].replace('{{{{OUTPUTS_DIR}}}}', OUTPUTS_DIR)
```

Four Rust braces escape to TWO output braces. So Python looked for
literal `{{OUTPUTS_DIR}}` (double) in a string containing
`{OUTPUTS_DIR}` (single). No match → output path stayed as
`{OUTPUTS_DIR}/LTE2026217002.pdf` → weasyprint wrote to nowhere
useful → the cmd never found a file in OUTPUTS_DIR to register.

Fix: collapse to two Rust braces (`{{OUTPUTS_DIR}}`) so the Python
source becomes `.replace('{OUTPUTS_DIR}', ...)`. Now single brace
matches single brace, path resolves to the per-call outputs dir,
file gets registered, FileCard renders.

Verified `replicate_from_sample` uses the correct 2-brace pattern
already — only `render_html_to_pdf` had the bug.

## v0.20.16 — HTML+CSS → PDF rendering (the path Claude.ai uses) (2026-06-13)

User feedback: "Asked Claude.ai to replicate an invoice — 95% perfect.
Travis's reportlab output is 60-70%." The gap isn't model capability;
the SAME model family powers both. The difference is the rendering
substrate. Reportlab forces the model to hand-position every element
and guess font metrics. HTML+CSS gives proper text flow, table
layout, alignment, padding — all the things the model is trained on
deeply.

This release adds the HTML+CSS path.

### New tool: `render_html_to_pdf`

LLM writes the doc as a self-contained HTML template; weasyprint
renders to PDF. Input: `html`, optional `css`, `outputName`,
optional `assetDocumentIds` (logos/fonts mounted under INPUTS_DIR
and referenced via `file://...` URLs in the HTML), `pageSize`
(default Letter), `margins` (default 0.5in). Output is registered
as a Travis Document and returned in `generatedDocumentIds` like
every other PDF tool.

weasyprint added to `scripts/fetch-python.mjs`: pure-Python, ~15MB,
no headless browser dep. All three platform builds pick it up on
next CI fetch.

### Journal prompt rewritten with the three-path hierarchy

```
1. EXACT REPLICA with new data   → replicate_from_sample   (95-100%)
2. FRESH DOC / "match this style" → render_html_to_pdf      (90-95%)
3. COMPLEX PROGRAMMATIC LOGIC     → run_python + reportlab  (60-70%)
```

Travis defaults to Path 2 for any new doc generation. Path 1 wins
when there's a sample to overlay onto. Path 3 is the last resort —
constraint solving, dynamic line counts HTML can't express,
custom-layout edge cases.

Honest expectation: invoice fidelity goes from current ~70% (when
the LLM picks run_python despite earlier prompt nudges) to ~95% on
first try once the LLM defaults to render_html_to_pdf.

### Why this beats reportlab specifically

- **Text flow**: HTML wraps, breaks, hyphenates the right way. Reportlab
  doesn't unless you do it yourself.
- **Tables**: `<table>` + CSS layout handles column widths,
  row striping, header repetition. Reportlab Table requires
  per-cell positioning.
- **Fonts**: HTML resolves system + brand fonts via `@font-face`.
  Reportlab requires embedding TTF/OTF binaries explicitly.
- **Margins / page boxes**: `@page { size; margin; }` in one line.
  Reportlab requires SimpleDocTemplate + paragraph styles + flowables.
- **Color, padding, borders**: CSS one-liners. Reportlab three calls
  each.

The model emits HTML naturally because that's where its training data
density is. Reportlab output is always one step removed from what the
model "knows."

## v0.20.15 — Pixel-perfect template replication via PDF overlay (2026-06-12)

The Tier 4 asset library got the LLM logos + page renders, but the
underlying problem stayed: anything redrawn with reportlab is an
APPROXIMATION of the sample. The LLM has to guess every coordinate,
font metric, and line stroke. Even with perfect images, the layout
drifts and the user sees "close enough but not right."

This release stops redrawing.

### New tool: `replicate_from_sample`

Opens the sample PDF as the canvas, white-masks each variable
region the LLM supplies, stamps the new text at the same coords,
saves the result as a new Travis document. The structural pixels
(letterhead, table rules, signature lines, footer, watermark) NEVER
MOVE because they're never redrawn. Output is byte-identical to the
sample except where the LLM intentionally changed something.

```
replicate_from_sample({
  sampleDocumentId: 1,
  outputName: "LTE2026217002_IS217_Invoice.pdf",
  overlays: [
    {page:0, bbox:[x0,top,x1,bottom], value:"LTE2026217002"},
    {page:0, bbox:[...], value:"IS 217 School of Performing Arts..."},
    {page:0, bbox:[...], value:"$15,000.00"},
    ...
  ]
})
```

Defaults: `bboxOrigin: "top-left"` (matches pdfplumber +
`analyze_document_styling` output), `font: "Helvetica"`, `fontSize:
10`, `color: [0,0,0]`, `align: "left"`, `maskOriginal: true`.
Color accepts both 0-1 floats and 0-255 ints; the tool normalizes.

Under the hood: embedded Python script (uses already-bundled `pypdf`
+ `reportlab`) builds an overlay PDF with the same page count and
dimensions as the source, draws masks and text per overlay entry,
then merges via `PageMerge`. Runs in the same sandbox as
`run_python` — same cache integration, same plan support, same
`generatedDocumentIds` payload so the FileCard renders.

### Journal prompt rewrite for "match this sample" requests

The "user gave you a sample" section now leads with:

> FIRST: if the sample is a PDF AND the user wants pixel-perfect
> visual fidelity, USE replicate_from_sample — DO NOT redraw with
> reportlab.

Fallback to `run_python` + reportlab only when the sample isn't a
PDF or when structural changes (added columns, different line
counts) can't be expressed as bbox stamps.

### Why this beats redrawing

- Logo: stays exactly where it is, bit-for-bit identical, because
  it's part of the sample's content stream. We don't re-encode it.
- Fonts: the sample's text uses whatever font the original PDF
  embedded. We overlay with the closest reportlab built-in — fine
  for variable values, never touches the fixed text.
- Table rules + signature lines: same vector strokes, no
  reapproximation.
- Margins + padding: never recomputed.

### Two modes — Travis picks based on the workflow

- `mode: 'overlay'` (default): vector overlay. White-mask + stamp. Tiny
  file, instant render. Original text stays in the content stream under
  the mask — a select-copy of the output PDF would reveal it. Right for
  invoices and forms going to humans (printed, viewed, filed). Almost no
  one select-copies an invoice.
- `mode: 'raster'`: rasterize each page at `dpi` (default 300) first, so
  the background is now pixels with no text layer at all. New text drawn
  as vector on top. Original text is GONE, not just hidden. Right for
  regulated documents, contract diffs, anything going to legal where the
  scrub has to be real. Tradeoff: larger file, background's crisp vector
  graphics become 300-DPI raster (invisible to the eye but present in
  metadata).

Prompt teaches Travis to default to overlay and switch to raster when
the user says "make sure the old values are completely gone" / "scrub
the old data" / "this is going public."

Implementation note: I initially proposed bundling pikepdf to do
content-stream-level text removal. Walking the PDF content stream to
identify and surgically delete the right Tj/TJ operators turns out to
be genuinely hard for arbitrary regions. Rasterize-then-overlay is
simpler, uses libraries we already bundle (pypdfium2 + PIL + reportlab
+ pypdf), and delivers the same "no underlying text" guarantee. Cleaner
path even though it's not what I named earlier.

### Planned UI follow-up (v0.21)

The chat-side previewer gets a canvas mode that lets the user click
any field, edit the value, drag the bbox. The corrected coordinates
flow back via `record_step_result` so the next overlay stamp is
already calibrated. Implementation is non-trivial (PDF.js render +
hit testing) but composes cleanly with what's shipping today.

## v0.20.14 — DAG-style step pipes + v0.20.13 build hotfix (2026-06-12)

Two things in one release: the build-breaking format-string bug that
killed both CI and release for v0.20.13, and the next planner
feature — DAG-style data flow between steps.

### Hotfix — v0.20.13 didn't compile

`format!(r#"..."#)` interpreted unescaped `{key:'read_log'}` as a
format placeholder ("expected `}` in format string"). All three
release builds and CI failed at the same step. The plans prompt
section I added in v0.20.12 had literal curly braces inside an
example payload. Escaped to `{{` `}}`. Same bug class as the earlier
`{kind:'logo'}` fix — adding a CI guard against it is queued.

### DAG-style step pipes

v0.20.13 made step output retrievable via `get_step_result`, but the
data still flowed through the LLM's context: Step B asked for Step A's
result, the LLM saw it, the LLM inlined it into Step B's code. That
inflates context cost and loses fidelity on big data.

v0.20.14 adds direct pipes:

- `run_python` and `edit_python_artifact` accept `stepInputs:
  [{fromStepKey, asFile}]`.
- Before invoking Python, the tool fetches each referenced step's
  cached `result_json` and mounts it at `INPUTS_DIR/<asFile>` (with
  the convention `_step_<key>.json` when `asFile` is omitted).
- The Python script reads it with `json.load(open(os.path.join(
  INPUTS_DIR, 'dates.json')))`. The data NEVER goes through the LLM.
- `extra_input_files: HashMap<String, Vec<u8>>` plumbed through
  `interpreter::cmd::RunPythonParams` so the bundled CPython wrapper
  drops the files into the per-call inputs directory alongside
  user-supplied docs.

**Upstream invalidation cascades.** The downstream step's
`input_hash` folds in each referenced step's `result_hash` via
`extend_hash_with_step_inputs`. If `read_signin_log` re-runs (new
spreadsheet uploaded, code edited), its hash changes; every
downstream step that depends on it via `stepInputs` sees its own
hash flip too, so caches invalidate top-down automatically. No
manual invalidation, no TTL.

### Net effect

A multi-step LTE workflow now has:
- O(1) data flow between steps (mounted files, not LLM context)
- O(1) re-execution after edits (only changed steps + their
  descendants re-run, everyone else is cache-hit)
- Zero "remember to check cache first" discipline required

Journal prompt updated with the DAG pattern.

## v0.20.13 — Plans become enforced, not optional (2026-06-11)

v0.20.12 shipped the plan substrate but the discipline was LLM-driven
("remember to call `get_step_result` before running"). v0.20.13 moves
the discipline INTO the tools — the LLM doesn't have to remember,
the cache check happens automatically.

### Auto-caching on `run_python` + `edit_python_artifact`

Both tools now accept optional `planId` + `planStepKey`. When set:

1. **Cache check first.** The tool computes an input hash over
   (script source + sorted documentIds with their `content_hash` from
   the `document` table + sorted libraries), then asks the plan if
   there's a `done` step with a matching `result_hash`. Hit → return
   the cached `result_json` and `documentIds` in milliseconds without
   spawning Python. No subprocess. No agent-loop wall time.
2. **Auto-record on miss.** On a cache miss the script runs, and the
   result is recorded against the step with the new input hash. Next
   call with the same inputs hits the cache.
3. **Auto-invalidate on input change.** Edit the code, change the
   document set, or upload a fresh version of a doc (the doc's
   `content_hash` changes), and the input hash flips. Cache miss,
   fresh run. There's no manual "invalidate" step — input change is
   the invalidation.

The LLM no longer needs the discipline of "check cache first." The
tool does it. Forgetting to pass `planStepKey` just means you opt out
of the cache — falls back to the prior behavior.

### Active plan auto-injection into the prompt

Every turn, before the LLM sees the user message, the system pulls
the conversation's active plan (if any) and prepends a block:

```
== ACTIVE PLAN (planId=7) ==
Goal: Generate IS 217 invoice LTE2026217002
Steps:
  - key='read_signin_log' status=done purpose='Parse the sign-in log' [cached]
  - key='filter_dates' status=done purpose='Filter for IS 217' [cached]
  - key='generate_pdf' status=pending purpose='Render the invoice'
Pass planId + planStepKey on run_python / edit_python_artifact calls — cached steps return in milliseconds without re-running.
```

So a follow-up like "regenerate with Feb 27 added" sees: planId=7 is
live, two steps are cached, only `generate_pdf` needs to re-run. The
LLM doesn't have to call `plan_active` to discover this — it's right
there in the context.

### Recap of the planner's full shape now

- `create_plan(goal, steps)` — declare at the top of a complex turn.
  Reuses an active plan with the same goal instead of fanning out.
- `run_python(code, ..., planId, planStepKey)` — cache-aware execute.
  Hit returns cached result; miss runs + auto-records.
- `edit_python_artifact(...)` — same cache semantics.
- `get_step_result` / `record_step_result` — still available for
  steps that aren't a Python invocation (e.g. caching the result of
  `read_document` for a 50MB sheet so the next turn doesn't re-parse).
- `list_plan_steps(planId)` — review state.
- Active plan auto-injected into the user message so follow-up turns
  resume without explicit lookup.

The 50-call IS 217 trace from yesterday should now look like 6 calls
on the first turn and 1-2 calls on every follow-up.

## v0.20.12 — Plans, file cards, payload aggregation, Windows asset paths (2026-06-11)

A grab-bag with one substantive new feature (plans) and three fixes
addressing pain points from the IS 217 invoice trace.

### Plans + step caching — the real fix for "50 run_python calls per turn"

`run_python` re-reading the same sign-in log five times in one turn,
then doing it AGAIN on the next turn when the user asked for a small
tweak. The agent loop had no concept of "this step was completed; use
the cached output."

New substrate:
- Migration `0042_plans.sql` introduces `plan` (goal-scoped) and
  `plan_step` (keyed, with cached `result_json` + `document_ids`).
- New module `src/plans.rs` with the typed API: `create_plan`,
  `record_step`, `get_step`, `list_steps`, `active_plan_for_conversation`.
- Four new LLM tools — `create_plan`, `record_step_result`,
  `get_step_result`, `list_plan_steps`.

Pattern: at the start of any multi-step turn the LLM calls
`create_plan({goal, steps:[{key, purpose}, ...]})`. Before doing each
step it calls `get_step_result(planId, key)` — if cached, skip the
work. After completing a step it calls `record_step_result(planId,
key, {result, documentIds})` to cache for next time. On follow-up
turns the same plan resumes; unchanged steps return instantly.

Journal prompt updated: "ALWAYS open a plan for any task with ≥3
logical steps." That's the #1 lever against the 50-call failure mode.

### File cards for `doc#N` in assistant text

The chat surface was supposed to render a clickable FileCard whenever
Travis wrote `doc#11` after generating a PDF. It didn't — `doc#11`
appeared as plain text. Two layers fixed:

- **Backend**: every `run_python` and `edit_python_artifact` result
  is now scanned for `generatedDocumentIds` and the union is folded
  into the assistant payload. The chat reads it and renders cards.
- **Frontend**: `ChatTurn` also scans assistant prose for `doc#N`
  markers, unions with the payload field, strips the marker from
  displayed text, and renders FileCards underneath. Works whether
  the doc was generated this turn or referenced from a prior turn.

Belt + suspenders so a missing marker in the LLM's prose doesn't lose
the card AND a missing payload field doesn't lose it either.

### Windows asset paths

User report from a successful regeneration: "the logo asset from the
PS 556 sample couldn't be re-embedded (path issue on Windows)."
`absPath` strings come back with backslashes; the LLM was wrapping
them in f-strings or hardcoding them with single backslashes that
Python parsed as escape sequences.

Fix in `find_template_assets` description: explicit guidance — pass
`absPath` to PIL / pathlib as-is, never re-escape, never write
literal paths with single backslashes. Concrete `pathlib.Path(...)`
example included.

## v0.20.11 — Successful turns were being reported as errors (2026-06-11)

Travis writes "here it is — doc#11" after generating a PDF. The
chat UI was supposed to render a clickable FileCard in place of the
marker. It didn't — the user saw "doc#11" as plain text.

Why: ChatTurn looked at `message.payloadJson.generatedDocumentIds`
to decide which cards to render. The `Extraction` struct doesn't
carry that field, so `assistant_payload` never had it populated. The
field was set on every `run_python` tool RESULT but discarded as
soon as the tool result was fed back to the LLM.

Fix (frontend, immediate): assistant messages now ALSO get scanned
for `doc#N` markers, unioned with `generatedDocumentIds` from the
payload, and rendered as FileCards. The literal markers get
stripped from the displayed text so the card replaces them visually,
mirroring how user-side attachments already render. Works the same
whether the doc was just generated this turn or referenced from a
prior turn.

A more thorough backend fix — aggregate every run_python's
`generatedDocumentIds` into the assistant payload — is still worth
doing for the case where the LLM forgets the marker. Frontend scan
catches it in the common case where the prompt instruction lands.

## v0.20.11 — Successful turns were being reported as errors (2026-06-11)

User showed a trace where Travis generated `LTE2026217002 → doc#9`,
emitted a clean prose summary referencing the doc marker, and the
chat surface still showed:

> `Travis hit an error while thinking through that turn.`
> `err_msg: model didn't call any tool: expected value at line 1 column 1`
> `content: "Invoice LTE2026217002 generated — doc#9 ..."`

That's a successful completion being reported as failure. Two places
needed fixing:

### Worker agent loop — text-only completion is not an error

In `journal.rs`, when the LLM returned text with no tool calls, the
code tried `parse_extraction(&content)` (expects structured JSON),
failed because the response is natural language, and returned
`ok=false` with "model didn't call any tool" — even though `doc#N`
was right there in the text. The manager loop then "forced progress"
for 60-200s of wasted iterations before giving up.

Fix: when text-only AND content is substantive (≥20 chars) or
contains a `doc#N` marker, accept it as a Delivered turn. The
content becomes the `response` field; intent is `deliver`.

### Manager — `doc#N` is a strong delivery signal

`evaluate_progress` already short-circuits to `Delivered` when
`generated_doc_ids` is non-empty or `proposed_actions` is non-empty.
Added a third short-circuit: when `response` contains `doc#`, treat
as Delivered regardless of length or handoff-phrase count. The
chat UI uses the same marker to render the file card — if it's good
enough for the UI, it's good enough for the manager.

Combined effect: a successful run_python → "here's your file:
doc#9" turn now lands as Delivered on the first manager pass
instead of triggering 60+ seconds of forcing and an error toast.

(Separately, the user's second turn in that session failed with
"error sending request for https://api.anthropic.com/v1/messages" —
that's a network/API error, not a Travis bug. Transient.)

## v0.20.10 — Onboarding-loop root-cause hunt (2026-06-11)

User report: "Update Travis, restart, end up on onboarding even though
the DB wasn't cleared." Reproduced in code review even though I
couldn't reproduce locally. Two paths could falsely route a returning
user to onboarding:

### app_status — full-row SELECT was too sensitive

`profile_exists` was computed via `state.db.user_profile().await?.is_some()`
which fetches all 10 user_profile columns. If ANY of those columns
deserialized unexpectedly (a type mismatch, an unexpected NULL where
sqlx expected `String`, a migration that left a column in a weird
state), the SELECT failed, the Result was Err, and the `?` propagated
the error all the way out — `app_status` returned 500.

That triggered the frontend catch which set fake `onboarded: false`
and rendered Onboarding.

Fix: probe with `SELECT COUNT(*) FROM user_profile WHERE id = 1`
instead. We only care whether the row exists; column shape doesn't
matter.

### App.tsx refresh() conflated two failures

```ts
try {
  const s = await getAppStatus();
  if (s.onboarded) {
    const p = await getUserProfile();  // ← if THIS throws...
  }
} catch {
  setStatus({ onboarded: false, ... });  // ← ...you land here
}
```

If `getAppStatus` succeeded with `onboarded: true` but
`getUserProfile` then failed, the unified catch set
`onboarded: false` and Onboarding rendered. The profile fetch was
gating the entire decision.

Fix: split the two `try`s. `getAppStatus` decides `onboarded`.
Profile fetch is a separate best-effort pass; failing it leaves the
user at Splash with no name displayed but they're not bounced back
to onboarding.

### Diagnostic logging

The `tracing::debug!` line in `app_status` was promoted to
`tracing::info!` so it shows in default logs without bumping log
level. Now you can grep the log for
`app_status: flag=... profile_exists=... -> onboarded=...` to see
exactly what the backend decided.

## v0.20.9 — CI hotfix: protocol-asset feature in Cargo.toml (2026-06-11)

The `ci.yml` workflow's `cargo check` step has been failing since
v0.20.5 with:

> The `tauri` dependency features on the `Cargo.toml` file does not
> match the allowlist defined under `tauri.conf.json`.
> Please run `tauri dev` or `tauri build` or add the `protocol-asset`
> feature.

v0.20.5 enabled `assetProtocol` in `tauri.conf.json` to fix the
PDF previewer's "asset.localhost refused" error, but the matching
`protocol-asset` feature on the `tauri` crate dependency was never
added. `tauri-action` adds it implicitly so the release workflow kept
publishing — but plain `cargo check` (and any local `cargo build`)
hit the allowlist check.

Fix: `tauri` features now includes `protocol-asset`. Both CI workflows
green.

## v0.20.8 — Onboarding is cloud-only + persistence fix (2026-06-11)

Two related fixes for the "every launch re-fires onboarding" report.

### Root cause: completeOnboarding was never called for cloud users

`completeOnboarding` (which writes the `user_profile` row + sets
`meta.onboarded = 'true'`) was bound to step 7's submit handler — the
api-key page. v0.20.2's cloud skip jumped users from step 5 directly
to step 8 (pack picker), bypassing step 7 entirely. So cloud users
walked through pack-picker → workspace → done visually, but the DB
stayed empty. Next launch, `app_status.onboarded` returned false and
Travis routed back to onboarding. Forever.

Fix: when `next()` crosses the 5 → 8 boundary, fire
`completeOnboarding` with the current draft and provider='travis_cloud'.
Best-effort — errors don't block step progression.

### Onboarding is now cloud-only

Per direction: "every user should default to travis cloud — remove the
model selection and api-key steps from onboarding entirely."

- Steps 6 (provider picker) and 7 (api key) no longer render to any
  user, regardless of build. The step indices stay reserved so we
  don't have to renumber every transition.
- Progress bar shows 9 dots instead of 11 — the skipped steps don't
  get rendered as ghost dots.
- `initialDraft.provider` defaults to `"travis_cloud"`.
- `platform_info` probe + `cloudAvailable` state removed from the
  onboarding component — every user is on Travis Cloud, no branching.

Advanced users wanting to bring their own LLM still have the full
toggle in Settings → Model (Travis Cloud / Use my own LLM).

## v0.20.7 — Speed discipline + onboarding cloud-skip render guard (2026-06-11)

### Hard rules against python flailing

The IS 217 invoice test took ~15 minutes when it should have been
under 5. Looking at the trace: 50+ `run_python` calls iterating on
the same data — re-reading the spreadsheet, hunting for paths,
re-parsing the sign-in log. Every call is 10-60s of user wall time.

Prompt changes to enforce discipline (in `journal.rs` and the
`run_python` tool description):
- ONE read pass per turn. Bundle every spreadsheet read, filter, and
  print step into a single script that returns the JSON the rest of
  the turn needs.
- Generate once, edit thereafter. First successful generation should
  produce the final PDF. Tweaks go through `edit_python_artifact`,
  not full regens.
- Trust the wrapper. `INPUTS_DIR` is set; files are there. NEVER
  probe with `os.path.exists` or "find the file" passes. If the file
  is missing, surface once and ask — don't flail.
- No exploratory code. Use `read_document` (instant) before writing
  Python, not after.
- Hard cap: 3 `run_python` calls per turn. Call 4 = "you are flailing,
  stop and rethink."

These don't make individual calls faster — but they cut the call
COUNT, which is where the 15-minute tail came from.

### Onboarding still showed the LLM picker when cloud is available

The v0.20.2 skip-logic moved step pointers around steps 6 and 7 when
`travisCloudAvailable` was true, but the step renderers didn't guard
themselves. If anything (race, history-back, refresh) put step state
at 6 or 7 with cloud available, the LLM picker still showed.

Fix: render-guard steps 6 and 7 with `!skipCloudSteps`. With cloud
available those steps cannot render even if `step === 6` somehow
happens.

(Note: separately, the user reported onboarding firing on every
update. We couldn't reproduce — the DB at `<app_data>/travis.db`
persists across updates by default and the `onboarded` check
falls through to `user_profile IS NOT NULL`. Most likely cause is
the DB getting wiped by manual cleanup or an installer setting. If
it keeps happening, check the DB path on startup logs.)

## v0.20.6 — Tier 4 first-turn fix + stale invoice rows + run_python paths (2026-06-11)

Three follow-ups from the v0.20.5 triage. Each one cost real time
during the IS 217 test and was worth shipping ahead of the bigger
multi-file inline-preview work.

### Tier 4 misses the first-turn benefit

`list_template_assets` returned `status=extracting` synchronously
because background extraction kicks off on classification but doesn't
complete before the next chat turn. Travis correctly fell back to
styling-only, losing the 1:1 fidelity that's the whole point of
Tier 4.

Fix: new `wait_for_extraction(pool, document_id, max_wait_ms)` that
polls the extraction row every 400 ms until status leaves
`pending` / `extracting`. The `list_template_assets` tool now waits
up to 25 s — long enough for most extractions (page raster +
embedded-image crops) on real samples.

### Invoice row went stale after regeneration

The pack policy was: if an existing invoice row exists with the same
`number`, propose-action and skip. That made sense for sent / paid
records but blocked the much more common case — the LLM regenerates
a draft invoice (PDF v2 with corrected dates, v3 with the right day
count) and the typed `invoice` row in Manage stayed pinned to v1's
numbers. School drill-down showed the wrong amount and missing dates.

Fix: split policy by status.
- `sent` / `paid` → unchanged. Propose-action; no silent overwrite.
- `draft` → silently UPDATE with the latest emission. Matches the
  [[feedback-track-everything]] / newer-wins memory: a draft isn't
  sensitive enough to gate.

Paired prompt change in `journal.rs`: the `invoiceDrafts` extraction
field description now says explicitly "RE-EMIT this every time you
regenerate, even with the same number" so the LLM doesn't stop after
the first draft.

### run_python kept hunting for `/inputs/` on Windows

The wrapper exposes `INPUTS_DIR` and `OUTPUTS_DIR` Python constants,
but the tool description still said "documents mounted at /inputs/".
On Windows the `/inputs/` symlinks don't exist (the wrapper only
creates them on POSIX), so Travis spent multiple python iterations
hunting for the right path before falling back.

Fix: tool description rewritten to lead with `INPUTS_DIR` and
`OUTPUTS_DIR`, and explicitly warn off `/inputs/` + hardcoded paths.

## v0.20.5 — Drill-down + previewer + chat-card fixes (2026-06-11)

A grab-bag bundle from the first real LTE workflow test. Six small but
high-impact fixes spotted while generating an IS 217 invoice end-to-end.

### Document previewer "asset.localhost refused to connect"

Tauri 2 disables the `asset://` protocol by default; without it
`convertFileSrc()` URLs return ERR_CONNECTION_REFUSED inside the
iframe. The previewer always showed the cloud-icon failure page on
PDF/image rows.

Fix: `tauri.conf.json` → `app.security.assetProtocol = { enable: true,
scope: ["**"] }`. PDFs/images now render inline as designed.

### Coach detail crashed with REAL vs INTEGER mismatch

`SELECT IFNULL(SUM(hours), 0)` returns INTEGER 0 when there are no
rows; sqlx bound the column to f64 and panicked. One-char fix: `0.0`.

### Engagement drill-down showed 0 everywhere

`engagement.school_id` was always NULL because `capture/mod.rs` had a
TODO-marker `parent_hint` resolver that always returned None. With no
school FK, the engagement detail's invoice / hours / docs queries
couldn't find anything to show.

Fixes:
- The resolver now actually looks up the school by name from the
  current extraction's anchor entities and passes
  `Some(("school", id))` as the parent_hint.
- `engagement::ensure` backfills `school_id` on existing rows whose
  FK was NULL when a hint is now available. Old engagements created
  before this fix get patched on the next chat turn that mentions
  them.

### Generated invoice never showed a file card

The `run_python` / `edit_python_artifact` tool descriptions told the
LLM to skip filename mentions because "the UI auto-renders cards" —
but the chat surface actually parses `doc#N` markers from the
message text to know which cards to render. The LLM, following the
prompt, never wrote `doc#N`, so no card appeared.

Fix: prompt rewrite. The new instruction is explicit — when
`generatedDocumentIds` is returned, the reply MUST include each id as
a `doc#N` marker. UI hides the literal marker and renders the card.

### PO / WO tabs empty after dropping POs and WOs in chat

The typed `purchase_order` / `work_order` tables require engagement
FKs + parsed fields the LLM doesn't yet extract, so they stayed
empty even though doc rows existed with `kind='po'` / `kind='wo'`.
The auto-CRUD list view then read "no rows".

Fix: two new pack UI overrides (`PurchaseOrdersTab`, `WorkOrdersTab`)
render the underlying documents directly until typed extraction lands.

## v0.20.4 — Migration hotfix: duplicate ceiling_cents (2026-06-10)

Startup crash fix. v0.20.0 introduced migration
`0006_engagement_terms.sql` which added `ceiling_cents` to the
engagement table — but the column was already added by
`0005_collapse_contract_engagement.sql` (with `NOT NULL DEFAULT 0`).
SQLite hard-errored with `duplicate column name ceiling_cents` and
Travis refused to start on any DB built since v0.20.0.

Fix: drop the redundant ALTER from 0006; 0005's column survives. The
Rust code already used `COALESCE(ceiling_cents, 0)` and
`p.ceiling_cents.unwrap_or(0)`, so the typed-column promotion goal of
0006 still holds via 0005's column.

No data loss possible — failed migrations roll back atomically; users
whose Travis was wedged at startup never had period_start /
period_end columns added either. v0.20.4 adds them cleanly.

## v0.20.3 — Doc-only mode + asset rename tool (2026-06-10)

Two follow-ups on top of v0.20.2's split-window previewer and template
asset library.

### Tier 3: doc-only mode with floating chat overlay

The DocumentViewer header gains a "Hide chat" toggle (corner-brackets
icon). When on:
- The split layout collapses; the doc fills the entire content area.
- A floating "Chat with Travis" pill appears bottom-right, indicator
  dot pulses when Travis is thinking.
- Clicking expands a draggable 420×640 floating panel with the full
  AskTab inside — same conversation, same attachments, same input.
- "−" collapses back to the pill; corner-brackets returns to the split
  layout.

State persists across launches via `travis.docFullscreen` in
localStorage.

### Polish: user-/LLM-editable asset display_name

- `set_template_asset_label` Rust command + matching `Tool` for the LLM.
  When extraction's heuristic produces a generic name like
  "L2E_Sample_Invoice – embedded image (page 1)" and Travis can tell
  what the asset actually is (logo, signature, header banner), it can
  rename it without a vision call. Future `find_template_assets`
  searches grep against the better name.
- Journal prompt teaches when to call it.

### Deferred for next slice

- Vision-based classification refinement (a Claude vision call per
  freshly-extracted asset that overrides the heuristic `kind` +
  `display_name`). Cost/latency wasn't worth bundling into this slice;
  the heuristic + LLM-driven rename covers the common case.

## v0.20.2 — Travis Cloud + forced-upgrade gate + 1:1 template replication (2026-06-10)

### 1:1 template replication via binary asset extraction (Tier 4)

The pre-existing `analyze_document_styling` tool returns a JSON
description of a sample's visuals — "Arial 12pt, navy header, logo at
top-left." Useful, but not enough to reproduce: a script following that
description has to redraw the logo from text, and the result is an
approximation, not a 1:1 replica. The user flagged this directly:
"The invoice it generated was great and accurate but strayed from the
provided template/sample design. The embedded images and headings were
not captured."

This release adds the complementary tool: actual binary extraction.

- **New migration `0041_template_assets.sql`** introduces three tables:
  - `template_extraction` — per-document lifecycle (`pending` →
    `extracting` → `ready` / `failed`) + the per-doc manifest JSON.
  - `template_asset` — the global, deduped asset library.
    Content-addressed by SHA-256 with a UNIQUE constraint, so the
    same L2E logo lifted from twenty sample invoices is ONE row.
    Each asset carries `kind` (`logo` / `header_banner` /
    `signature` / `watermark` / `page_render` / `embedded_image`)
    and a human `display_name` so the LLM can ground "use the L2E
    logo" against an actual row.
  - `template_asset_source` — N:M asset ↔ source doc with page +
    bbox. Answers "where did this logo come from", supports
    "constrain to this sample's assets only" lookups, and lets one
    asset belong to many samples without duplication.
- **`src-tauri/src/template_assets.rs`** owns the extraction. Inline
  Python script (uses already-bundled `pdfplumber` + `pypdfium2` +
  `Pillow`) rasterizes every page at 300 DPI and crops every embedded
  image. Each image is then SHA-hashed, copied into
  `<app_data>/template_assets/<hash[:2]>/<hash>.png` (skip if exists),
  and upserted into `template_asset` by content_hash. Kind inferred
  heuristically from page position + dimensions (top-left small →
  `logo`; wide-thin top → `header_banner`; wide-thin bottom →
  `signature`; centered massive → `watermark`).
- **Capture pipeline hook** in `src-tauri/src/capture/mod.rs`: when the
  background extraction observes a `documentClassifications` entry
  with `kind` starting with `sample_` / `template_` (or exactly
  `sample`), it schedules extraction for that doc. Runs after the
  chat reply is delivered — never blocks a turn.
- **Two new LLM tools**:
  - `list_template_assets(documentId)` — returns the per-document
    manifest. Use when the user JUST attached a sample.
  - `find_template_assets({kind?, query?, sourceDocumentId?})` —
    library-wide search. Use when the user asks for a doc without
    attaching a sample but Travis has seen relevant samples before.
    Grab the L2E logo from any prior sample and embed it in a fresh
    invoice; no need to attach the original sample.
- **Updated journal prompt**: the "make one like this" workflow now
  pairs `analyze_document_styling` (layout JSON) with
  `list_template_assets` (actual pixels). Explicit instruction that
  reuse across docs is the point, via `find_template_assets`.
- **Storage**: assets at `<app_data>/template_assets/<hash[:2]>/<hash>.png`
  (one file per unique image, ever). Per-doc lifecycle is in
  `template_extraction`; binary lifecycle is independent and survives
  doc deletion if other docs still reference it via
  `template_asset_source`.

### Travis Cloud is the default

Provider configuration moves out of the onboarding path. Builds compiled
with `TRAVIS_CLOUD_ANTHROPIC_KEY` ship with Travis Cloud baked in; users
land at "ready to chat" instead of "paste your Anthropic key, pick a
model."

- `build.rs` reads `TRAVIS_CLOUD_ANTHROPIC_KEY` and
  `TRAVIS_CLOUD_MODEL` as `option_env!`. Key never appears in the binary
  as plaintext at rest — it's compiled into the release artifact at CI
  time. CI sets the secret; local dev builds without the secret fall
  back to the existing claude/openai/ollama flow unchanged.
- New `travis_cloud` provider in `llm::build`. Resolves the build-time
  key on demand; if a user somehow lands on the provider in a build
  without the key, they get a clear "switch to your own LLM in
  Settings" error rather than a silent failure.
- `default_model("travis_cloud")` and `cheap_model("travis_cloud")`
  share the claude tiers, so the same Sonnet-4-6 / Haiku-4-5 split
  applies.

### Existing users get migrated, not re-onboarded

- `Db::migrate_to_travis_cloud_if_needed` runs once at startup. If
  cloud is available and the user wasn't on `travis_cloud` already,
  their previous provider/model are stashed into `meta` keys
  (`previous_llm_provider`, `previous_model`) and the profile flips to
  cloud. A `travis_cloud_migrated_v020` flag prevents the migration
  from re-firing.
- Stashed values surface via `platform_info` so Settings can show
  "previously you were on Claude — switch back" if needed.

### Onboarding skip + Settings toggle

- New `platform_info` Tauri command tells the UI whether this build
  has cloud baked in.
- Onboarding skips provider + api-key steps (6, 7) when cloud is
  available. Forward and back navigation both honor the skip.
- Settings grows a "Travis Cloud / Use my own LLM" toggle. With cloud
  selected, the provider grid + API key field disappear; the user just
  has Travis Cloud. Flipping the toggle reveals the existing
  claude/openai/ollama configuration unchanged.

### Forced upgrade enforcement

New mechanism for marking a build too old to keep using. The release
ops can publish a tiny sentinel and every running Travis below that
floor sees a hard gate.

- `force_upgrade::check_force_upgrade` fetches
  `https://github.com/myketheguru/travis-releases/raw/main/min-supported.json`
  (`{"minVersion": "x.y.z", "reason": "...", "latestVersion": "x.y.z"}`)
  and compares the running version. Network failures and missing files
  are treated as "not required" — a transient outage never gates the
  user.
- `ForceUpgradeGate` overlay wraps the app shell. When the sentinel
  says the build is below the minimum, the user gets a non-dismissible
  modal with "Install update" (kicks the existing updater) or "Quit
  Travis" (calls a new `quit_app` command). No back door, no settings
  escape.
- Default behavior is permissive: if the sentinel file doesn't exist
  in the releases repo (today's state), no one is gated. Publishing
  `{"minVersion": "0.20.2"}` is the explicit opt-in to start blocking
  older builds.

### Version bump
`package.json`, `tauri.conf.json`, `src-tauri/Cargo.toml` → 0.20.2.
v0.20.1's tag-only release left these at 0.20.0; cleaned up here.

## v0.20.0 — Engagement schema promotion + Consent cards in chat (2026-06-10)

Two of the three v0.19.x deferred items now land. The third
(relationship-aware drill-down per school) is its own slice in v0.20.1.

### Engagement: period + ceiling promoted to typed columns

Pack migration `0006_engagement_terms.sql`:
- `engagement.period_start TEXT`
- `engagement.period_end TEXT`
- `engagement.ceiling_cents INTEGER`
- Index `idx_engagement_period` on `(period_start, period_end)` for
  the upcoming "show me engagements active between X and Y" filters.

The LTE pack's `apply_extraction_observations` (background) now
writes these columns directly when the LLM extracts an
`engagementEnrichments` entry from a PO/WO doc — replacing the
v0.19.4 stash into the `summary` text column.

### Consent-required changes flow through the chat now

The two action kinds introduced in v0.19.5 —
`lte_engagement_critical_change`, `lte_invoice_critical_change` —
finally render where users can act on them. The pack records the
diff; the chat surface shows a confirm-or-dismiss card.

- New shared `ActionCard.tsx` extracted from the overlay. Same
  visuals: pulse-tinted by default, warn-tinted for high-risk
  kinds (shell, money/identity changes).
- Per-kind details surface the actual diff. For money fields
  (`amount_cents`, `ceiling_cents`): `$X.XX → $Y.YY`. For other
  fields: `oldValue → newValue`.
- AskTab loads `listProposedActions({status: 'proposed'})` for the
  active conversation, polls every 5s while open, and renders the
  cards above the chat input. Confirm/decline trigger the existing
  action handlers and refetch so the card disappears immediately.

### Critical-field tolerance

Engagement ceiling changes within 5% are treated as soft (silent
newer-wins). Larger swings emit `lte_engagement_critical_change`.
This keeps drifting rounding ("$15,000.00" → "$14,997.50") from
spamming consent cards while material changes ($15K → $20K) still
gate on user confirmation.

### Deferred to v0.20.1

- **School / engagement detail drill-down.** Click a row in the LTE
  Schools / Engagements tabs → side panel showing the full
  relationship graph (engagements + their hours + invoices + linked
  docs all on one page). The data's all there; this is a focused
  UI slice.

## v0.19.7 — Hotfix: macro recursion limit for the grown extraction schema (2026-06-10)

CI cargo check failed with `recursion limit reached while expanding
$crate::__private::vec!` on the v0.19.6 build. The
`serde_json::json!` literal in `build_extraction_tool` has grown
past the default 128-step macro limit with the v0.19.x additions
(pack memories, document classifications, coach hours, engagement
enrichments, invoice drafts).

Local cargo (Windows) happened to fit because of incremental
caching; clean Linux CI build hit the wall.

One-line fix: `#![recursion_limit = "512"]` at the top of
`src-tauri/src/lib.rs`. 512 leaves room for the next few additions
before another bump is needed.

Cargo check on my local box (which was on the same code prior to
the limit bump) was green — Linux CI is the source of truth for
this kind of macro-expansion failure.

## v0.19.6 — Core Documents tab + Newer-wins override policy with consent gate (2026-06-10)

### Newer-wins override policy with consent gate

Closes the rule: **"Newer docs and info can override older ones
saved. If sensitive or critical, request permission from user."**

- **Soft fields** (engagement.school_year, doc-kind upgrades from
  uncategorized, summary stash): newer always wins, silent.
- **Critical fields** (engagement.contract_ref change, invoice
  amount_cents change, invoice already sent/paid on re-emission):
  records a `proposed_action` with old/new diff. Two new action
  kinds — `lte_engagement_critical_change`,
  `lte_invoice_critical_change` — registered. The invoice row is
  NOT silently overwritten; confirmation triggers the actual update.
- **Pack memory corrections supersede.** A new `correction` memory
  for a target archives prior non-pinned memories on that target
  (`relevance_score = 0`). Raw rows stay for audit.

UI for the confirm-or-dismiss card lands in v0.20 alongside the
broader Manage relationship-drill-down.

### Documents — first-class core tab

A new **Documents** tab in Manage, alongside Tasks / Reminders /
Threads. Every file Travis has ever seen — uploaded by the user,
generated by Travis, imported from another source — lives here and
can be browsed by category, searched by name, or pulled into the
active chat.

- New `DocumentsTab` component. Backed by the existing
  `list_documents` Tauri command + existing `document` table; no
  schema changes. Filters by:
  - Samples (`sample_invoice`, `sample_pdf`, …)
  - Invoices (incl. `generated_invoice`)
  - POs / WOs
  - Sign-in sheets
  - Contracts
  - Spreadsheets / PDFs (other) / Uncategorized
- Per-row actions: open in default viewer, reveal in file manager,
  retag kind via inline dropdown, **attach to active chat**, jump
  to the conversation the doc came from.
- Search input filters by display name + original filename.

### Library → Chat bridge

The "attach to chat" button dispatches a window-level custom event
`travis:attach-document-from-library`; AskTab listens for it and
adds the doc to the active turn's attachments. So a user can:

1. Find a sample invoice in the Documents tab.
2. Click "attach to chat".
3. Travis sees it in the current conversation and can match its
   style or pull data from it — no re-upload, no path juggling.

This closes the loop on the user's spec: "uploaded samples to
travis/chat can be tracked and found there", and "during
conversations/workflows, those docs can be referenced/pulled into
the chat".

Closes the rule: **"Newer docs and info can override older ones
saved. If it's sensitive or critical, request permission from
user."**

### Soft fields — newer always wins, silent

`engagement.school_year`, `engagement.summary`, document classification
kind upgrades, pack memory dedup (existing behavior). New data
replaces old without asking. The agent loop already saw the user
attach a newer doc; presumption is they want the latest read.

### Critical fields — propose_action, never silent

These would change money, identity, or sent state — too consequential
to overwrite without a nod.

- **`engagement.contract_ref`** changing (different contract instrument)
  → records `lte_engagement_critical_change` with `oldValue` / `newValue`
  in params. UI renders as confirm-or-dismiss card.
- **`invoice.amount_cents`** changing (or status is already `sent`/
  `paid` when re-emitted) → records `lte_invoice_critical_change`
  with the diff and existing status. The invoice row is NOT
  silently rewritten — confirmation triggers the actual update.

### Pack memory — corrections supersede

When the LLM emits a memory of kind `correction` scoped to a target
entity, prior memories for the same target (rule / preference /
fact) get archived (`relevance_score = 0`) so they drop out of
recall. Pinned memories are exempt. The raw row stays on disk so
the audit trail is complete; the user can re-pin via the Manage UI
if a "correction" was misapplied.

### Action kinds registered

Two new entries on the LTE pack's `action_kinds()` so the consent-
required actions aren't dropped by the proposed_action validator:
`lte_engagement_critical_change`, `lte_invoice_critical_change`.

Confirmation UI for these (the "show, confirm, dismiss in one tap"
card) ships next slice alongside the broader Manage UI work.

## v0.19.4 — Engagement enrichment + Invoice draft auto-record (2026-06-10)

The two big deferred items from v0.19.3 now ship via the same
pack-owned background-pipeline pattern.

### Engagement enrichment from PO/WO docs

When the LLM reads a PO or WO, the extraction now carries
`engagementEnrichments`:

```json
{
  "engagementName": "IS 217 Leadership Coaching",
  "contractRef": "QR179CF",
  "periodStart": "2026-03-23",
  "periodEnd": "2026-06-25",
  "ceilingCents": 1500000,
  "schoolYear": "2025-26"
}
```

LTE pack's `apply_extraction_observations` (background) updates the
engagement's `contract_ref` and `school_year` columns directly, and
stashes period + ceiling into the `summary` text until the v0.20
schema promotes them to typed columns. COALESCE pattern means
non-null values never get blanked by future blank emissions.

### Invoice draft auto-record

When the LLM generates an invoice (typically via `run_python`
emitting a PDF), it also emits an `invoiceDrafts` entry:

```json
{
  "number": "LTE2026217002",
  "recipient": "IS 217 School of Performing Arts, 977 Fox St…",
  "schoolName": "IS 217",
  "periodStart": "2026-03-17",
  "periodEnd": "2026-05-26",
  "hoursTotal": 10,
  "rateCents": 150000,
  "amountCents": 1500000,
  "generatedDocId": 7,
  "notes": "Includes 03/17 per CEO permission"
}
```

Pack inserts an `invoice` row with `status = 'draft'` (UNIQUE on
`number` dedups re-emissions), resolves school + coach by name to
populate FKs, and links the generated PDF doc to the school via
`document_link kind='invoice_for'`. So the Invoices tab actually
reflects work-in-progress invoices; `sent` / `paid` status changes
stay user-driven.

### What still needs UI work (v0.20)

The data flows are in. The Manage tabs show typed rows via the
existing auto-CRUD UI. What's missing is relationship-aware drill-
down — clicking a school surfacing its contracts + hours + docs +
generated invoices all on one page. That's a focused UI slice;
queued for v0.20.

## v0.19.3 — Pack-owned auto-population + Document classification + signing-sheet → coach_hours auto-extract + everything backgrounded (2026-06-10)

Closes the document-understanding gap from the v0.19.2 audit:
attached files were being stored as generic `kind = 'file'` and
left disconnected from the entities they belonged to. Now every
attached doc gets classified + linked, signing sheets feed hours
into `coach_hours`, and auto-created engagements bind to the right
school.

### Document classifications

New `documentClassifications` field on the extraction schema. The
LLM must emit one entry per attached doc:

```json
{
  "documentId": 3,
  "kind": "po",
  "linkedEntityKind": "school",
  "linkedEntityName": "IS 217",
  "periodStart": "2026-03-23",
  "periodEnd": "2026-06-25"
}
```

Agent loop applies kind via `documents::db::set_kind` and creates a
`document_link` row via `link_to_entity` once the spine entity is
resolved by name. Result: the Manage > Documents tab can finally
group by real type (po / wo / signed_sheet / invoice / contract)
and per-school drill-down can show every doc tied to a school.

### Signing sheets → coach_hours auto-populate

New `coachHours` extraction field. The LLM emits one row per
(coach, school, date, hours) tuple it pulled from an attached
signing sheet. Agent loop:

1. Calls `coach::ensure(workspace, coach_name)` (auto-creates).
2. Calls `school::ensure(workspace, school_name)` (auto-creates).
3. Dedups by `(coach_id, school_id, session_date)` — re-uploading
   the same signing sheet doesn't double-count.
4. Inserts a `coach_hours` row with description `"from signing
   sheet doc#N"` for audit.

Result: the LTE coach_hours tab actually has data after a sign-in
sheet upload, instead of staying empty.

### Engagement school linkage

`engagement::ensure` was called with `school_id = None` in v0.19.1.
Now the agent loop resolves the first school named in the same
extraction (via `school::find_by_name`) and passes its id as the
hint. Result: auto-created engagements get a proper school FK on
first creation, not an orphan row.

### Architecture: pack-owned, fully backgrounded

Two user-stated rules that this slice honors:

1. **"What to ingest and persist should live under the pack."** Core
   stays generic. Two new optional trait methods on `PackHandle`:
   - `ensure_entity(pool, ws_id, kind, name, parent_hint)` — pack
     decides whether the named entity gets a typed row.
   - `apply_extraction_observations(pool, ws_id, conv_id, extraction)`
     — pack inspects the full extraction JSON and applies whatever
     observation fields it cares about (e.g. LTE's `coachHours`,
     `documentClassifications`).
   Both default to no-op. LTE overrides both. Tutoring pack can
   plug in identically when it wants to auto-populate.

2. **"Everything should run as a separate background process and
   never interrupt chat."** The inline pack-specific code that I
   sketched in earlier v0.19.3 drafts is gone. The capture pipeline
   (`capture::run_background`, already a spawned task) now also
   iterates packs and calls both hooks AFTER tasks/reminders. The
   chat command returns the moment the assistant message is
   appended; pack persistence runs in the spawned background task.

### Deferred to v0.19.4 / v0.20.0

- **Contract / invoice auto-create.** They commit to money so they
  need the "show, confirm, dismiss in one tap" UX before rows can
  be auto-written.
- **Manage UI redesign.** Relationship-aware drill-down (click a
  school → see contracts, hours, docs, generated invoices).

## v0.19.2 — LTE coach + engagement auto-create + user-facing file folder (2026-06-10)

Three pieces — finishes everything queued in the v0.19.1 changelog
except contract/invoice (those need confirmation UX).

### Coach auto-create on mention

Same pattern shipped for schools in v0.19.1, applied to coaches.
- `coach::find_by_name` + `coach::ensure(workspace, name)` —
  idempotent insert keyed on case-insensitive name.
- Agent loop's entity-mention loop now silently calls
  `coach::ensure` whenever the LLM extraction names a coach.
  Notes column flags `"Auto-created from chat mention."`.

### Engagement auto-create on mention

New `domain::engagement` Rust module exposing `find_by_name` and
`ensure(workspace, name, school_id_hint)`. Hooked into the same
entity-mention loop. School_id hint is None for now — the user
edits via the Manage tab when they want to link a freshly-named
engagement to its school. Stage defaults to `'assessment'` (start
of the 3 A's). Spine entity sync fires from the helper so cross-
pack retrieval sees the row.

Contracts (which are the same record as engagements in the schema
since v0.7.0 collapse) and invoices stay behind the proposed-action
confirmation gate — they commit to money and the user-facing
auto-create UX needs a "show, ask, dismiss in one tap" pattern
that's queued for v0.19.3.

### User-facing file folder

Hash-addressed storage stays for dedup; on top of that we now
maintain a browseable mirror at:

  `<user-documents>/Travis/files/<conversation-slug>/<original-name>`

- New `storage::user_facing_root`,
  `storage::conversation_slug`, and
  `storage::mirror_into_user_folder` helpers. Hard link by default
  (zero disk cost, single inode shared with hash storage); copy on
  cross-volume / hardlink-not-supported failure.
- Hooked into both `ingest_document` (user-dropped files) and
  `register_generated_document` (Travis-generated outputs). Best-
  effort — mirror failures log and don't unwind the document row.
- Collision-safe filename suffix (`(2)`, `(3)`, …) when the same
  display name lands in the same conversation.
- The existing `reveal_document_in_folder` action still points at
  the hash storage for now; v0.19.3 can flip it to reveal the
  user-facing mirror once the layout's bedded in.

## v0.19.1 — LTE school auto-population + Entity-scoped memory recall (2026-06-10)

Two pieces queued from v0.19.0.

### LTE school auto-population

User's "radio silence" complaint about pack tabs is now fixed for
the most visible case — schools. When the LLM extraction names a
school (`extraction.entities["schools"] = ["IS 217"]`), the agent
loop calls `lead_to_empower::domain::school::ensure(workspace, name)`
which:

- Checks `school` table for a case-insensitive name match in this
  workspace via the new `find_by_name` helper.
- Inserts a row with `notes = "Auto-created from chat mention."`
  if none exists. Existing row is returned unchanged.
- Spine entity sync fires from inside `school::upsert` so the
  cross-pack retrieval index also picks it up.

So next time the user mentions a school anywhere, the LTE school
tab is no longer empty. Contracts / engagements / invoices stay
behind a confirmation gate (they commit to money), but observational
data (schools, coach mentions) flows in automatically per
`feedback_track_everything`.

### Entity-scoped pack memory recall

v0.19.0 shipped pack memory but the recall only pulled pack-wide
memories. Now `pack_memory` recall also includes memories scoped to
entities currently in conversation — so a constraint about IS 217
("never include March 17 dates") only fires in the system prompt
when IS 217 is in the current thread.

- New `spine::entity::in_conversation_scope(workspace, conv_id)` —
  scans the last 20 conversation messages for substring matches
  against entity display_names; returns up to 20 (kind, id) pairs.
  Crude (substring) but fast and good enough until we wire
  mentions-per-message at write time.
- Agent loop passes those pairs to `recall_for_prompt` so the
  recall query includes entity-scoped memories alongside the
  pack-wide ones.

### Deferred to v0.19.2

- Contract / engagement / invoice auto-record (behind confirmation
  policy — needs UX design first).
- Organised file-folder symlink farm under `Documents/Travis/…`.

## v0.19.0 — Cross-conversation context tool + Reasoning-between-steps (2026-06-10)

### `search_conversations` — pull context from any prior thread

New LLM tool that searches across all conversation threads in the
workspace for a literal phrase. Use case: the user says "the IS 217
work from last week, what rate did we settle on?" and Travis can
find the answer by reaching into the prior thread instead of asking
again.

- New `src-tauri/src/tools/search_conversations.rs` —
  `SearchConversationsTool` registered in the read-only tool registry.
- Substring match (case-insensitive) over message body content,
  scoped to visible workspaces, recency-ordered. Returns up to 30
  hits with `{conversationId, conversationLabel, messageId, role,
  snippet, createdAt}` per hit. The snippet is a ~200-char excerpt
  centred on the matching phrase.
- `excludeActive` defaults to true — searches OTHER threads, not
  the one currently in flight. The LLM can flip it false on the
  rare "search this thread itself" case.
- Distinct from `search_memory` (semantic embeddings hit). Use
  search_memory for "what was said"; search_conversations for
  "where it was said and the exact words".
- Humanised step label: **Looking through past threads**.

### Reasoning-between-steps

The user wanted to see Travis's thinking BETWEEN tool calls, not
buried as collapsed notes under "Working on it". Now: every
substantive thinking block (≥80 chars) the worker emits also
spawns a distinct `Thinking`-kind child step under the manager
step, with a verb-led label (`Reasoning · noticed PO rate differs
from sample...`) and the first 280 chars as the step detail.

The chat surface renders these inline alongside the tool-call
steps, so a long turn now reads as a narration:

```
▸ Working on it
  · Reasoning · Need to find the IS 217 PO and WO first…
  · Reading attachment — IS 217 (1).pdf
  · Reading attachment — LEA991893POPrint…
  · Reasoning · noticed PO rate is $1,500, sample was $2,300…
  · Generating invoice
```

The note-on-manager-step path stays for the collapsible archive of
the full chain-of-thought; the new child step is the narration card.

### Pack memory — Travis learns and remembers, every turn

This is the heavier lift the user asked for: "travis should learn
and constantly update its memory on things". Pack-scoped memory
that Travis writes to PROACTIVELY in every turn, recalls into every
future system prompt, and dedups so re-stated rules just bump
relevance.

- Migration 0040: new `pack_memory` table
  `(workspace, pack_slug, kind, target_kind, target_id, content,
  source, conversation_id, relevance_score, pinned, created_at,
  updated_at)`. Five kinds: `rule`, `preference`, `constraint`,
  `fact`, `correction`. Two scope levels: pack-wide (target NULL)
  and entity-scoped (target_kind + target_id point at a spine
  entity row).
- New `packs::memory` module with `remember`, `recall_for_prompt`,
  and `format_for_prompt`. Dedup on (workspace, pack, target,
  content) — restating a memory bumps relevance instead of
  creating a new row.
- New `remember_constraint` LLM tool for explicit "remember this"
  requests from the user. Humanised step label:
  **Remembering that for next time**.
- New `packMemories` field on the `report_extraction` tool schema —
  the LLM PROACTIVELY picks rules / preferences / constraints /
  facts / corrections out of every turn and emits them in the
  extraction. The agent loop persists each one alongside tasks
  and reminders. So Travis learns whether or not it's asked.
- `build_system_prompt` now takes a `pack_memory_block` argument;
  the agent loop calls `recall_for_prompt` for the active workspace
  + enabled packs and folds the memories into the system prompt as
  a `=== Pack memory ===` block.

v0.19.1 will add entity-scoped recall (memories tied to a school /
contract surface only when that entity is mentioned in the current
turn) — currently the recall pulls pack-wide memories only, which
is strictly additive: no risk of hiding relevant memories.

### Deferred to v0.19.1

- Entity-scoped recall (needs entity-in-conversation tracking).
- LTE pack auto-creation of schools / contracts / engagements from
  the same extraction path (proactive table population, not just
  memory).
- Organised file-folder symlink farm under `Documents/Travis/…`.

## v0.18.3 — Conversation switcher + Reveal-in-folder file trace (2026-06-10)

### Searchable conversation switcher (#)

The chat surface now has a dropdown at the top that lists recent
conversations and lets the user switch between them or start fresh.

- New `conversation::list_for_switcher` (SQL) + Tauri command
  `list_conversations_for_switcher(query, limit)`. Returns
  `ConversationListItem` rows with title, first-user-message
  preview (most threads don't have explicit titles, so the
  preview snippet is what the user identifies the thread by),
  message_count, status, updated_at.
- Query does case-insensitive substring match against the title
  AND against the body of any message. So searching "IS 217" or
  "Wallace Ave" lands on the right thread even when the title is
  empty.
- New `ConversationSwitcher.tsx` — dropdown anchored to the chat
  header, search input with 150ms debounce, "+ New chat" action,
  recent-first list with relative-age timestamps (`just now`,
  `5m`, `2h`, `3d`, `2w`, `4mo`).
- AskTab now reloads its messages when `activeConversationId`
  changes after the initial resume — clicking a row in the
  switcher swaps the chat; selecting "+ New chat" sets the id
  to null and clears the messages.

### Reveal file in folder (#)

Every file Travis ingests or generates is now traceable to its
exact path on disk via a one-click action on the file card.

- New `reveal_document_in_folder` Tauri command using the opener
  plugin's `reveal_item_in_dir` — opens the OS file explorer
  (Finder on macOS, Explorer on Windows, default file manager on
  Linux) with the document selected.
- New folder icon on every `FileCard` next to the "open" label.
  Click reveals the file in your file manager; the surrounding
  card click still opens it in the default viewer (PDF reader,
  Excel, etc.) — different gestures, different intents.

Files continue to live under
`<app_data>/documents/<first-2-of-hash>/<hash>.<ext>` — content-
addressed for dedup. A reorganised, human-browsable layout
(grouped by conversation or by date) is queued for a later slice;
the reveal-in-folder action covers the immediate "where is this
file?" need.

### Deferred to v0.19.0

- Cross-conversation context pulling (`search_conversations` tool)
- LTE pack auto-population from the capture pipeline
- Reasoning-between-steps surfacing in the chat

## v0.18.2 — Step humanisation + chat truncation feel + cleaner file refs (2026-06-10)

Three UX polish fixes from real-world use of v0.18.1.

### Humanised step labels

The chat surface was leaking technical tool names — "Running Python",
"search_memory", "analyze_document_styling", "documentIds=[4]" — and
that's the wrong altitude for a user-facing assistant. New
`humanize_tool_name` in journal.rs maps the registered tool name
(plus the LLM's `purpose` field for run_python) to plain English:

- `read_document` → **Reading attachment**
- `preview_document` → **Skimming attachment**
- `analyze_document_styling` → **Studying the layout**
- `search_memory` → **Checking the records**
- `find_case` → **Looking up the case**
- `reconcile_documents` → **Cross-referencing attachments**
- `web_fetch` → **Fetching from the web**
- `delegate` → **Asking a focused side-question**
- `run_python` (purpose contains "invoice") → **Generating invoice**
- `run_python` (purpose contains "sign…sheet") → **Building sign-in sheet**
- `run_python` (purpose contains "pdf") → **Generating PDF**
- `run_python` (purpose contains "excel/xlsx") → **Working with spreadsheet**
- `run_python` (purpose contains "parse/read/extract") → **Pulling data out of the sheet**
- `run_python` (purpose contains "filter/find") → **Filtering the data**
- `run_python` (other) → **Working on the file**
- `edit_python_artifact` → **Refining the file**

The new `resolve_tool_detail` (replaces the synchronous
`describe_tool_call`) hits the `document` table for any
document-touching tool call and surfaces the actual original
filename, so the step row reads **Reading attachment — IS 217 (1).pdf**
instead of the previous **Reading document · documentId=2**.
For `reconcile_documents`, up to three filenames are joined with
" + " to keep the row compact on multi-doc calls.

### Chunked chat history (proper fix for truncation feel)

Users reported chats looking "truncated to the last few messages"
on tab switch. The right fix isn't a bigger cap — it's pagination:

- Backend `conversation::messages` default returns the MOST RECENT
  50 messages (id DESC, then reversed to ASC for display). No
  artificial upper cap; explicit `limit` overrides for callers
  that need the whole thread.
- New `conversation::messages_before(conv, before_id, limit)` +
  `load_more_messages` Tauri command for paginated older fetches.
- AskTab listens on the scroll container's `onScroll`: when within
  120px of the top AND older history exists, fetches the next 50
  via `loadMoreMessages`, prepends, and preserves the view by
  setting `scrollTop = topBefore + (heightAfter - heightBefore)`.
- "Loading earlier messages…" / "Start of conversation" indicators
  at the top so the user knows the state.

Chat-tab switch now: jumps to the latest message (where you left
off), scroll up to lazy-load earlier turns. No artificial truncation.

### Cleaner file references in agent text

The `run_python` tool description now instructs the worker to
DESCRIBE what it did in plain English without including the
filename inline — the clickable file card the UI renders below the
message already carries the identity. Previously the agent would
write "Generated — doc #7 (IS217_Invoice_LTE2026217002.pdf)" which
duplicated the card and looked technical.

### Deferred (next slices)

- v0.18.3 — searchable conversation switcher + organised file
  folder structure with traceability.
- v0.19.0 — cross-conversation context pulling, LTE pack
  auto-population from the capture pipeline, reasoning-between-steps
  surfacing.

## v0.18.1 — Hotfix: drop AppImage from Linux bundle targets (2026-06-09)

v0.18.0's Linux Release build failed at the AppImage bundling step.
`.deb` and `.rpm` succeeded; only `.AppImage` choked — `linuxdeploy`
exited non-zero after ~14s, almost certainly tripping over the
332MB bundled Python's symlinks and shared libraries. AppImage's
portable-bundle model doesn't compose well with our resource shape.

Fix: `tauri.conf.json` switches `bundle.targets` from `"all"` to
an explicit list `["deb", "rpm", "nsis", "app", "dmg"]`. Linux
gets `.deb` (Debian/Ubuntu) and `.rpm` (Fedora/RHEL) — covers the
vast majority of distros without the AppImage rough edges.

`linuxdeploy`'s stderr isn't captured in the workflow log so the
exact failure mode is opaque; an AppImage-specific fix may be
worth a follow-up if/when we want that format back.

## v0.18.0 — Pyodide → bundled CPython subprocess runtime (2026-06-09)

The Pyodide-in-hidden-window architecture (v0.14 → v0.17.3) is gone.
Python now runs as a real CPython 3.13 subprocess bundled via
python-build-standalone. Every chronic Pyodide failure mode goes
away: warmup-pattern hallucination, "interpreter not ready"
timeouts, ready-race, cold-load latency, WASM compatibility gaps.

### What changed

- **New runtime** `src-tauri/src/python_runtime/mod.rs`:
  - `run(app, params) -> RunPythonResult` spawns the bundled
    Python via `tokio::process::Command`. Per-call temp dir holds
    inputs/ + outputs/ + the wrapped script. Process gets cleaned
    up after every call — no state leak between turns.
  - Cold start: ~150ms (process spawn) vs Pyodide's 3-5s.
  - Stdin nulled, stdout/stderr piped, `CREATE_NO_WINDOW` on
    Windows so no flash of console.
  - Timeout via `tokio::time::timeout`; on fire, the child dies.
- **Build-time fetcher** `scripts/fetch-python.mjs`:
  - Downloads indygreg's python-build-standalone tarball for the
    host platform (Windows x64, macOS arm64/x64, Linux x64).
  - Extracts via system `tar` (Windows 10 1803+, macOS, Linux all
    have it natively).
  - Pre-installs 21 wheels: pandas, openpyxl, xlsxwriter,
    reportlab, pypdf, fpdf2, pdfplumber, python-docx, pillow,
    numpy, lxml (native C-ext), beautifulsoup4, python-dateutil,
    pytz, num2words, markdown, jinja2, pyyaml, qrcode[pil],
    python-barcode, requests.
  - Hooked into `predev` + `prebuild` so `npm run build` always
    has Python ready.
  - Idempotent: skips re-download / re-install if already present.
- **Tauri bundle**: `tauri.conf.json` now declares
  `"resources": ["resources/python/**/*"]` so the installer
  carries the bundled Python. Hidden interpreter window deleted
  from the config (no longer needed).
- **Frontend**: deleted `src/interpreter/main.tsx` and
  `interpreter.html` (the old Pyodide loader entry). Removed
  Pyodide-related vite plugin and the `pyodide` npm dependency.
- **Tool layer**: `src-tauri/src/interpreter/cmd.rs::run_python`
  internals swapped to call `python_runtime::run`. The tool's
  public surface (`RunPythonParams` / `RunPythonOutcome`) is
  unchanged — LLM-facing interface untouched.

### Installer size

+200 MB. The bundled Python tree is 332 MB on disk (~120 MB
compressed in the installer); numpy + pandas + lxml + reportlab
account for the bulk. Worth it — local-first stays intact, and
the reliability gain pays for itself in one chat session free of
"interpreter not ready" retries.

### What stays the same

- The LLM's `run_python` tool description, schema, output format.
- Artifact persistence (`python_artifact` table + lineage via
  `superseded_by`).
- `edit_python_artifact` tool.
- The warmup-pattern short-circuit (v0.16.2) — still applies; new
  runtime would handle warmups fine but spawning a process for
  `print('hello')` is still wasted work.

### Cross-platform

CI matrix builds three installers (Windows, macOS, Linux) each
with its own bundled Python. No fork in the runtime code path —
`python_runtime::resolve_python_bin` picks the right binary via
compile-time `cfg!(target_os)` checks.

## v0.17.3 — Step polling fallback (2026-06-09)

The v0.17.1 mergeStepLists fix addressed the listSteps resync race
but didn't actually surface the real symptom: live events from
subscribeSteps aren't being delivered reliably during long agent
turns. User reported "the steps do not show in real time. But if I
reload the app or remount, the steps will appear" — meaning events
ARE persisted (and visible on resync), but the live Tauri-event
path is dropping them or batching them till after the turn ends.

Rather than chase Tauri event-delivery timing further, ship a
guaranteed polling fallback:

- New `useEffect` keyed on `[busy, activeConversationId]`. When
  busy === true, `setInterval` calls `listSteps` every 1.5s and
  merges with current state via the existing `mergeStepLists`.
- Cheap: a single indexed DB read per tick (~ms), no LLM cost.
- Stops the moment `busy` turns false (turn complete).
- The live subscription path stays — when it works, you see
  events immediately; when it doesn't, the poll catches up within
  ~1.5s. Belt-and-suspenders.

This is a UX-side fix. The underlying Tauri event reliability
question is queued for proper investigation as part of the v0.18
substrate work — alongside the bigger lever: switch from Pyodide
to bundled portable CPython.

## v0.17.2 — Hotfix: migration 0038 table-name collision (2026-06-09)

**Critical.** v0.17.0 and v0.17.1 fail at DB open with "table event
already exists" because migration 0038 tried to create a table
called `event` — but migration 0018 (pack_spine, shipped long ago)
already created a table with that name for entity activity. Anyone
who installed v0.17.0 or v0.17.1 sees Travis refuse to launch.

- Migration 0038 renamed: `event` → `conversation_event`; indexes
  `idx_event_conv` / `idx_event_kind` → `idx_conv_event_conv` /
  `idx_conv_event_kind`.
- `src/events/mod.rs` SQL updated to match.
- Since the failed v0.17.0/.1 migration ran inside a transaction
  that rolled back, `_sqlx_migrations` was never updated. The
  v0.17.2 migration applies cleanly on the same machine — no
  manual cleanup needed. Just install and relaunch.

Caught by myketheguru immediately on installing v0.17.1 ("Travis
could not open its database. Table events already exists"). Should
have been caught pre-ship by running cargo check against a DB with
0018 already applied, OR by grepping for `CREATE TABLE event` before
shipping a new migration. Adding the latter to the BRAIN.md
discipline note for v0.18.

## v0.17.1 — Interpreter-ready race + step-resync race + per-tool visibility (2026-06-09)

Three bugs surfaced during the PS556→IS217 invoice flow on v0.16.7.
The cap bump + thinking tiering let the worker keep going, but the
user couldn't tell why a turn was taking 20+ minutes — and one of
the reasons was a legitimate failure mode invisible until now.

### Interpreter-ready race (fix)

`run_python` was returning "interpreter not ready (Pyodide still
loading)" three times in a row even though Pyodide had finished
loading. Root cause: v0.16.5's bundled wheels made the interpreter
window's bootstrap drop from ~30s to ~3-5s — fast enough that the
frontend's `await emit("interpreter-ready", ...)` now fires BEFORE
Rust's setup() finishes attaching `handle.listen("interpreter-
ready", ...)` (DB migrations + keychain take ~5s). The event lands
in the void; `interp.set_ready(true)` never runs; every subsequent
`wait_ready(90s)` call returns false.

Fix: the interpreter window re-emits `interpreter-ready` every 3
seconds for the first 30 seconds. `set_ready(true)` is idempotent
so repeat firings are harmless; the first emit that lands after
Rust's listener attaches flips the state.

### Step-resync race (fix)

User-reported: "the steps do not show in real time. But if I reload
the app or remount, the steps will appear on the chat." Root cause:
the AskTab effect keyed on `activeConversationId` calls `listSteps()`
to resync from the DB. While that query is in flight, live
step-events arrive via the `step-event` subscription and
`setSteps()` adds them. When `listSteps` resolves, its
`setSteps(dbSteps)` REPLACES the in-memory state with the DB
snapshot from query-start — wiping every live event that arrived in
between.

Fix: new `mergeStepLists()` merges DB rows with in-memory rows by
id, preferring whichever has a terminal status or more notes, and
sorts by `startedAt`. Live events are never overwritten by a stale
resync.

### Per-tool-call step (visibility)

Long agent-loop turns previously showed a single "Working on it"
step for the entire pass — opaque from the user's POV. The agent
loop now opens a `StepKind::ToolCall` step for every tool dispatch,
labelled with a short detail pulled from the input (doc id for
read_document, purpose for run_python, query for search_memory,
url for web_fetch). The step's summary is the first 140 chars of
the tool result, so the user sees what came back at a glance.

## v0.17.0 — Event log substrate + condenser + reasoning UI (2026-06-08)

Three paired pieces of the OpenHands-inspired conversation
architecture finish what was queued in the v0.17.0 plan.

### Event log substrate (#172)

- Migration 0038 adds `event` table: `(conversation_id, kind,
  payload_json, parent_event_id, message_id, created_at)`. Indexed
  on `(conversation_id, id)` for tail-reads.
- `src-tauri/src/events/` module with:
  - `EventKind` enum: UserMessage, AgentResponse, ToolCall,
    ToolResult, Thinking, Condensation, Error.
  - `Event` + `AgentResponsePayload` + `ResponseKind` types.
  - `append`, `append_or_warn`, `list_for_conversation`,
    `list_after` helpers.
- **Dual-write** wired into `journal.rs`: every user turn writes
  `event(UserMessage)` after `conversation_message`; every assistant
  turn writes `event(AgentResponse)` after the message append. The
  existing `conversation_message` table stays the UI read path; the
  event log is ground truth for future branching, time-travel, and
  condenser composition.
- `parent_event_id` is NULL for now — single-thread reads. Future
  v0.18 branching follows the chain back.

### Condenser substrate (#173)

`src-tauri/src/events/condenser.rs`:
- `should_condense(events) -> bool` — heuristic on estimated token
  cost (12k soft budget).
- `condense(pool, provider, conversation_id, older_events)` — calls
  a cheap-tier LLM with a tight summary prompt and appends an
  `EventKind::Condensation` event whose payload carries the summary
  text plus the first/last covered event ids.
- **Not wired into the live agent loop yet.** Substrate only. Future
  slice flips the LLM-visible projection to substitute the
  condensation for its covered span when token budget is tight.

### Reasoning-only UI (#177)

- Migration 0039 adds `conversation_message.response_kind` (TEXT,
  nullable). One of `extraction` / `text_response` / `reasoning_only`.
- `events::classify_response(text, thinking, tool_calls, finalized)`
  classifies each turn:
  - finalized → Extraction
  - 0 tools + ≥1 thinking + >80 chars text → ReasoningOnly
  - else → TextResponse
- Agent loop in journal.rs stamps the column via the new
  `conversation::append_with_kind`. A new `parse_turn_stats` helper
  pulls thinking + tool counts from the agent loop's serialized
  last_dump (best-effort; degrades to TextResponse on parse fail).
- `ChatTurn.tsx`: when `responseKind === "reasoning_only"`, the
  visible content renders inside a distinct left-bordered panel
  with a "Reasoning · not yet acted on" header so the user
  immediately sees the worker's reasoning stage without confusing
  it for a finished answer.

### What's still queued for v0.18

Wiring the condenser into the live agent loop and flipping
conversation reads to project from the event log. Both depend on
this substrate being stable in production first.

## v0.16.7 — Agent-loop iteration cap + tiered thinking budget (2026-06-08)

Two paired fixes for the PS556→IS217 invoice-derivation flow which
hit two walls at once: ran out of iterations AND took too long
before it did.

### Cap raised 8 → 16

The flow burned through all 8 agent-loop iterations on a 5-document
turn (existing invoice for styling + PO + WO + master sign-in xlsx +
service catalog xlsx) and bailed with "Travis ran out of tool-call
iterations on this turn." Worker was doing the right work — read
each new doc, analyze styling, then run_python passes to parse
Excel → find rows → derive line items. 8 covered the 1-2 doc case
(v0.14.3 sizing); not the realistic 5-doc invoice flow.

- `MAX_ITER` in journal.rs raised 8 → 16
- Manager loop unchanged (3 × 16 = 48 max, same backstop)
- Forced-extraction on the last iter still ensures finalisation

### Tiered thinking budget

Extended thinking was burning 4000 tokens of cognition on every
iteration — including the mid-loop "which tool next?" turns that
don't need full re-derivation. Latency scales roughly linearly with
budget, so a 10-iter turn was paying ~40k thinking tokens of wall
clock.

New tiering:
- Iter 0 → 4000 (initial plan + first tool selection)
- Iters 1..N-2 → 1500 (decide next tool given new tool results)
- Iter N-1 → 4000 (forced extraction, real synthesis)

Net: ~50% latency reduction on long turns with no loss of depth
where it matters (start and end). The mid-loop turns are
mechanical dispatching, not novel reasoning.

## v0.16.6 — Valves (typed pack config) + Workspace runtime substrate (2026-06-08)

### Valves — typed plugin config (#175)

Pack authors can now declare typed settings once, and Travis renders
the form in Settings → Packs automatically. Open-WebUI-inspired.

- New `ValveDef` / `ValveType` / `ValveValue` on `PackHandle`:
  ```rust
  fn valves(&self) -> &'static [ValveDef] { &[] }
  ```
- Types supported: Text, LongText, Bool, Integer, Number, Enum.
- Values land in `meta.pack.<slug>.valve.<valve_slug>` as TEXT;
  typed parsing happens on read. Helpers: `packs::get_valve_text`,
  `get_valve_bool`, `get_valve_int`, `set_valve`, `reset_valve`.
- New Tauri commands: `pack_valves`, `set_pack_valve`,
  `reset_pack_valve`.
- L2E pack ships three example valves to validate the surface:
  default invoice terms (Enum), auto-lock signed sheets (Bool),
  default program for DoF-route invoices (Text). The frontend
  Settings → Packs panel can read these via `pack_valves()` and
  render a form using the same `FieldType`-style dispatch the
  auto-CRUD UI already does.

### Workspace runtime substrate (#176)

OpenHands-style execution-environment trait. Substrate only — no
callers migrated. Future Docker/remote modes can drop in without
rewriting every file-touching call site.

- New module `src-tauri/src/workspace_runtime/` with:
  - `Workspace` async trait: `read_file`, `write_file`,
    `remove_file`, `list_dir`, `exists`, plus `kind() -> &'static str`.
  - `LocalWorkspace` impl rooted at a host directory. Rejects
    absolute paths and `..` traversal as a defence against
    pack-supplied paths.
- Three unit tests cover roundtrip, absolute-path rejection, and
  parent-traversal rejection.
- Naming-discipline note: distinct from `crate::workspaces` (DB row,
  organisational scope). This abstraction is *where code runs and
  files live*; the DB row is *what data the user sees*. Documented
  in the module header.

### Why two unrelated slices in one release

Both are pack-ergonomics work, both small, neither blocks the other.
Bundling avoids two near-empty release notes.

## v0.16.5 — Bundled Pyodide wheels (offline + warm-by-default) (2026-06-08)

The dominant cold-start tax in v0.16.2-v0.16.4 was the first
`import pandas` (or openpyxl, reportlab, …) round-tripping to
jsdelivr.net — 8-12 seconds even on a good connection, longer on a
flaky one, and a hard fail offline.

### What changed

- New build-time script `scripts/fetch-pyodide-wheels.mjs` pulls
  every wheel we ship at build time:
  - **In-lock packages** (resolved transitively from Pyodide's
    `pyodide-lock.json`): pandas, numpy, pillow, lxml, beautifulsoup4
    (+ soupsieve, typing-extensions), pyyaml, jinja2 (+ markupsafe),
    python-dateutil, pytz, six, pyodide-http, micropip. Downloaded
    from `cdn.jsdelivr.net/pyodide/v0.29.4/full/` once at build time.
  - **Pure-Python extras NOT in Pyodide's lock**: openpyxl (+
    et_xmlfile), reportlab, pypdf, python-docx, num2words, fpdf2,
    xlsxwriter, qrcode, markdown. Resolved via PyPI's JSON API,
    newest `py3-none-any.whl` per package.
  - All wheels land in `node_modules/pyodide/` so `vite-plugin-
    static-copy`'s existing glob picks them up via one `*.whl`
    addition. A manifest (`pyodide-extras.json`) lists the extras so
    the interpreter knows which local URL to hand micropip.
- `prebuild` + `predev` scripts in package.json hook the fetch
  automatically — `npm run build` and `npm run dev` always have
  wheels ready. Idempotent: re-runs skip wheels already on disk with
  matching size.
- Interpreter (`src/interpreter/main.tsx`):
  - Bootstrap loads the extras manifest after micropip is up.
  - Lazy-import scan inside `runOne` now consults the manifest
    first — `micropip.install("/pyodide-bundle/.../reportlab-X.Y.Z-
    py3-none-any.whl")` instead of `micropip.install("reportlab")`.
    Falls back to PyPI bare-name install if the manifest is missing
    (defence in depth).
  - Background warmup pool expanded: in-lock (pandas, Pillow,
    jinja2) AND extras (openpyxl, reportlab, pypdf, num2words) all
    pre-install while the user is reading. First invoice flow pays
    zero cold-start cost.
- Installer growth: **+14.3 MB** (10.8 MB in-lock + 3.5 MB extras).
  Trade we'd take ten times out of ten — eliminates the dominant
  Pyodide friction point and makes Python work fully offline.

### Why this matters

Even with v0.16.2's lazy-load fix, the *first* `import pandas` in
any session was still an 8-12 second jsdelivr fetch. For users on
spotty wifi or offline (planes, coffee shops, secure sites), that's
the difference between "works" and "hangs". Now: 1-2 seconds disk +
decompress, every time.

### Deferred to v0.16.6

- Workspace abstraction (BaseWorkspace trait + LocalWorkspace impl) —
  shifted from v0.16.5 to make room for wheel bundling. Independent
  track; will land alongside Valves (#175) which is also pack
  ergonomics.

## v0.16.4 — Pyodide-excuse hallucination fix + typed-edge memory graph (2026-06-08)

### Pyodide-excuse fix

Sonnet was hallucinating: claiming "the Pyodide interpreter is still
cold-loading" without ever calling `run_python`. The step log
confirmed the tool was never invoked — the LLM was pre-empting with
a manufactured excuse.

Two-layer fix:
- **Prompt directive** in `run_python`'s tool description:
  "NEVER refuse this tool by claiming 'the interpreter is still
  cold-loading' without actually calling it. The interpreter pre-
  warms in 3-5 seconds and is reliably ready by the time any
  conversation reaches a run_python call. CALL THIS TOOL — don't
  pre-emptively excuse."
- **Manager-level detection** (`manager::is_pyodide_excuse`):
  scans the worker's response for "Pyodide is still cold", "cold-
  loading", "interpreter isn't ready", "as soon as interpreter
  loads", etc. (full list in `PYODIDE_EXCUSE_PHRASES`). If detected,
  forces a `Handoff` verdict so the manager loop re-runs with a
  *targeted* continuation directive: "Your previous reply claimed
  the interpreter isn't ready — you never called the tool. CALL
  run_python NOW with your actual work code."

### Typed-edge memory graph (#174)

Migration 0037 + new `src-tauri/src/memory/edges.rs`:
- `memory_edge` table with `(workspace, from_kind, from_id, to_kind,
  to_id, relation, attributes_json)`.
- 11 canonical AutoMem-style relation types as constants:
  `RELATES_TO`, `LEADS_TO`, `OCCURRED_BEFORE`, `PREFERS_OVER`,
  `EXEMPLIFIES`, `CONTRADICTS`, `REINFORCES`, `INVALIDATED_BY`,
  `EVOLVED_INTO`, `DERIVED_FROM`, `PART_OF`.
- Helpers: `link()`, `edges_from()`, `edges_to()`, `edges_by_relation()`.
- Distinct from the existing `relation` table (entity-graph topology
  in the pack spine). `memory_edge` is for artifact/claim/document
  lineage and temporal reasoning.

Wiring the capture pipeline + the `python_artifact` lineage to
write edges automatically is queued for follow-up. The substrate
exists; the integration is a small follow-up that doesn't block
shipping.

### Deferred to v0.16.6

- #175 Valves typed plugin config — will land alongside the
  workspace abstraction (also deferred). Both are pack-ergonomics
  work and pair naturally.

## v0.16.3 — Sub-agent tool + memory decay + store discipline (2026-06-08)

Low-risk backlog batch. Three of the queued items land together:

### Memory decay (#178)

Migration 0036 adds `relevance_score REAL DEFAULT 1.0` and
`pinned INTEGER DEFAULT 0` to the `claim` table. A daily background
ticker calls `memory::claims::decay_all` (exponential 180-day
half-life — ~0.4%/day) on every unpinned active claim, then
`archive_low_relevance` supersedes claims that have fallen below
the 0.05 floor.

Per the v0.15.x research synthesis, this mitigates AutoMem's known
"store grows → recall quality degrades" failure mode. Travis ships
forgetting *on* by default, not off.

`memory::claims::pin(claim_id)` exposes user-confirmed pinning;
pinned claims skip decay forever. v0.16.4's recall integration
will use `relevance_score` as a ranking weight; v0.16.3 just lays
the substrate.

### Sub-agent-as-tool (#179)

New `delegate` LLM tool (OpenHands pattern). Spawn a focused
sub-agent on a self-contained subtask without burning the parent
manager loop's iteration budget. One LLM call on the cheap tier
(Haiku for Claude); no tools inherited; system prompt explicitly
tells it "you're being asked by Travis, not the user — be terse and
structured." Returns the response string back to the parent agent.

Use cases: bounded summarisations, fresh-eyes decisions, drafting
a small piece while the parent works on the bigger plan.

### Canonical-store-per-entity discipline (#180)

New `BRAIN.md` section codifying the rule. Pick one canonical store
per entity; everything else is a regenerable projection. Avoids
Open WebUI's dual-write drift pain. Applies forward: when v0.17.0
lands the event-log substrate (#172), `conversation_message`
becomes a projection of the log, not a parallel table.

## v0.16.2 — Stop the Pyodide-warmup loop (2026-06-08)

The v0.15.4 error trace finally surfaced the recurring "Travis ran
out of tool-call iterations" pattern: Sonnet was emitting
`run_python({code: "print('hello')", purpose: "Warmup"})` on every
agent-loop iteration before doing real work. Each warmup burned a
manager iteration; by iteration 7 the loop hit MAX_ITER and bailed.

This is a Sonnet training pattern — "test before you run" — that's
actively harmful here because the Pyodide interpreter pre-warms at
app launch and is always ready when the tool runs.

### Two-pronged fix

**Prompt directive** in `run_python`'s tool description:

> CRITICAL: Do NOT call this with no-op warmup code (`print('hello')`,
> `pass`, `1+1`, version checks, etc.). The Pyodide interpreter
> pre-warms at app launch and is always ready when you call this
> tool. […] Each warmup costs a manager-loop iteration. Write your
> actual work code directly.

**Tool-level short-circuit** — `is_warmup_pattern()` heuristic
detects no-op calls and returns a fast synthetic success without
touching the interpreter. Patterns caught: `pass`, empty body,
`print("<short string>")`, `__version__`/`sys.version`, single
arithmetic expression, OR purpose-string contains "warmup" /
"warm-up" / "interpreter check" / "sanity check".

The synthetic response includes a `note` field steering the LLM
toward real code immediately: "Warmup-pattern code detected and
skipped. … Proceed directly to your actual work code; do not retry
warmup."

Conservative heuristic — anything over 80 chars (after stripping
comments) falls through to the real interpreter. Better to run a
legitimate tiny call than skip a real one.

### Pyodide lazy package loading (cold-start 30-60s → 3-5s)

Bundling Pyodide locally (v0.14.1) removed the network download
but the bootstrap was still doing 7 sequential package loads —
`pandas`, `openpyxl`, `Pillow` via `loadPackage` (~15-25s) plus
`reportlab`, `pypdf`, `python-docx` via `micropip.install`
(~15-25s). Total: 30-60s before the interpreter reported ready.
Most of those packages weren't needed for most turns.

**Now:**
- Bootstrap loads Pyodide runtime + `micropip` only (~3-5s).
  Interpreter reports ready immediately after.
- Each `run_python` call invokes `pyodide.loadPackagesFromImports(code)`
  to parse the user's script and lazy-load Pyodide-bundled packages
  (pandas, numpy, openpyxl, Pillow, etc.) on demand. Cached for
  the session.
- For PyPI-only packages (reportlab, pypdf, python-docx) — Pyodide
  can't auto-detect those — we scan the code for known names and
  `micropip.install` them ourselves.
- Background preload of common heavy packages (pandas, openpyxl,
  Pillow) kicks off via `requestIdleCallback` after the
  interpreter reports ready, so first real use is warm without
  blocking the bootstrap.

Trade-off: first `import pandas` of the session adds ~8s; every
subsequent use is free. Net win because pure-conversation turns
(no Python at all) pay zero warmup tax and the user sees a
usable app in 3-5s instead of 60s.

## v0.16.1 — v0.16.0 CI fix (2026-06-08)

The v0.16.0 commit had a TypeScript discriminated-union access bug
in `AskTab.tsx`'s step subscription handler (`event.conversationId`
is only present on the `started` variant of `StepEvent`). CI caught
it; this patch fixes it and re-ships the full v0.16.0 content
under v0.16.1.

All v0.16.0 content (case substrate going live, live step events
fix, Pyodide warmup bump) ships here.

## v0.16.0 — Case substrate goes live (2026-06-08)

v0.14 added the `travis_case` and `case_artifact` tables but they
were never read. v0.16.0 turns them on. Multi-day workflows now
auto-open a case; the LLM sees a continuity context block every
turn for the case's lifetime; the chat surfaces a header strip with
a switcher and close button.

### Auto-detection

When a chat doesn't already have a case linked, `journal_ingest`
evaluates three triggers before the agent loop:
- An active workflow on this conversation
- Multi-doc upload (≥2 `doc#N` markers in the user turn)
- Conversation depth ≥3

When **any two** fire, Travis auto-opens a case (named after the
active workflow's recipe, or the user's first-turn note) and links
the conversation to it. Cases hold across tab switches, restarts,
and multi-day gaps.

### `== ACTIVE CASE ==` context block

The user message every turn now carries a tight case-context block:
case name + id, started/last-activity timestamps, summary if any,
plus a directive: "this conversation is part of a multi-session
case; reference prior decisions, build on past artifacts, don't
restart from scratch." The LLM sees this every turn for the case's
lifetime — the continuity surface that lets Travis resume coherently.

### UI: case header strip + switcher

Above the chat transcript, a slim purple-tinted strip renders when
the conversation has a linked case:

```
case  PS 89 invoice close-out · 3 conversations · started 06-05    [switch] [close case]
```

Click `switch` to see a popover of other open cases — picking one
routes you to that case's most recent conversation. Click
`close case` to mark it closed (still reachable from the switcher,
just no longer "active").

### New backend helpers (cases::db)

- `find_by_conversation(conv_id) -> Option<Case>` — scans
  `conversation_ids_json` to find the case holding a given turn.
- `link_conversation(case_id, conv_id)` — idempotent append.
- `set_summary(case_id, summary)` — background-writer hook.
- `touch(case_id)` — bump `last_activity_at` cheaply each turn.

### New Tauri commands

- `case_for_conversation(conversation_id) -> Option<Case>` —
  what the frontend strip consumes.

### Bundled bug fixes

User-reported during v0.16.0 development — small enough to land in
the same slice.

**Live step events.** The frontend's step-event subscription was
gated on `activeConversationId` being non-null. When the user sent
their first message in a fresh chat, the id was null (backend
assigns it mid-call), so every step event the backend emitted
during that turn was filtered out. Steps only appeared after the
chat reloaded from the DB. Fix: subscribe persistently on mount;
use a ref to filter against the current id without re-subscribing.

**Pyodide warmup timeout.** Cold-start Pyodide load was exceeding
the 30-second warmup wait, causing repeated "interpreter not ready"
errors during the first `run_python` call. Each failure burned a
manager-pass iteration on the retry loop. Bumped to 90 seconds
which leaves comfortable headroom for cold installs.

### Sequencing (v0.16.1 → v0.16.3)

Queued for follow-up slices, independent + non-blocking:

- **v0.16.1** — Typed-edge memory graph (#174) + decay policy (#178).
  New `memory_edge` table with AutoMem's 11 typed edges
  (`LEADS_TO`, `EVOLVED_INTO`, `DERIVED_FROM`, `CONTRADICTS`, etc.).
  Cross-document reconciliation gets a real primitive.
- **v0.16.2** — Event log substrate (#172). `conversation_message`
  becomes a projection of an `event` log. Enables branching /
  time-travel / `reasoning-only MessageEvent` rendering.
- **v0.16.3** — Condenser pattern (#173). Depends on event log.

## v0.15.4 — Error observability + extended-thinking bug fix (2026-06-08)

Two things bundled: the underlying bug behind v0.15.2/v0.15.3's
recurring "Travis hit an error while thinking through that turn"
messages, and the observability infrastructure to track future
errors instead of staring at them blind.

### The bug

v0.15.2 enabled Anthropic's extended-thinking parameter on every
primary agent-loop call (`thinking_budget: 4000`). Anthropic's API
has a constraint we missed: when extended thinking is enabled,
`tool_choice` can be `auto` only — `Specific(...)` and `Required`
both get rejected with a 400. The agent loop forces
`ToolChoice::Specific(extraction_name)` on the LAST iteration as
its safety net, so every conversation with thinking enabled was
one bad turn away from this error.

Fix in `src-tauri/src/llm/claude.rs`: when `thinking_budget` is set,
coerce `Specific` and `Required` tool_choice down to `Auto`. The
system prompt + retry directive still steer the model toward the
right tool; we just don't *force* it at the API layer when the
constraint forbids forcing.

### Error observability

New `error_event` table (migration 0035) capturing every fail-soft
path that fires in journal_ingest. Schema: `kind`
(`llm_api` / `parse` / `iter_cap` / `tool_call` / `capture_bg` /
`other`), `message`, `detail_json` (raw response snippet + err_msg
+ context), `source` (where in the code the error fired), timestamp.

New module `src-tauri/src/diagnostics/mod.rs`:
- `record_error()` — best-effort persistence; never propagates further
  errors.
- `list_recent_errors` + `clear_error_log` Tauri commands for a future
  Diagnostics UI.

The synthesis fallback in `journal_ingest` now calls `record_error`
with the full `err_msg` + a 2000-char snippet of the raw LLM response
each time it fires.

### Expandable error trace in chat

The synthesised "Travis hit an error" assistant message now carries
the underlying error detail in its `payload_json` under `errorDetail`.
The chat surface renders a collapsed `▸ error trace` block under the
error message. Click to expand and see:
- The LLM `err_msg` (parse error, HTTP status, etc.)
- A snippet of the raw response with character count
- A "copy for bug report" button that puts the whole trace on the
  clipboard

Means you can now actually tell us *what* went wrong instead of
opening devtools.

## v0.15.3 — Artifact retention + iterative refinement (2026-06-08)

The Claude.ai iterative-refinement loop now works in Travis. After
`run_python` generates a file, the script + inputs + outputs are
persisted as a `python_artifact` row with an id the LLM sees in the
tool response. When the user asks for a tweak ("remove the note",
"add 7 hours to row 1", "signature line a tiny bit down"), Travis
calls a new `edit_python_artifact` tool with the prior artifact id +
the edited script, re-runs it, and links the lineage via
`superseded_by`. No more from-scratch regeneration.

### New table: `python_artifact` (migration 0034)

- Stores the Python source, input doc IDs, output document IDs,
  stdout / stderr / execution time / error.
- `superseded_by` self-FK so the lineage is diff-able.
- Indexed by conversation, workspace, and supersedes pointer.
- v0.16's typed-edge memory graph will wire `EVOLVED_INTO` edges
  onto these rows once it lands (per the Open WebUI / LangGraph /
  OpenHands / AutoMem research synthesis).

### `run_python` changes

- After a successful run, persists a fresh `python_artifact` row
  with `superseded_by = NULL` (first-of-lineage).
- Tool response now includes `artifactId` so the LLM can reference
  it on a subsequent edit.
- Also: now uses `ctx.conversation_id` instead of hardcoded `None`,
  so artifacts attribute to the right thread.

### New tool: `edit_python_artifact`

- Inputs: `supersedesArtifactId`, `purpose`, `code` (the new full
  script with the edit applied), plus the usual `documentIds` +
  `libraries`.
- The LLM produces the edited script itself from its in-context view
  of the prior one — no internal LLM call inside the tool. Faster,
  cheaper, makes the diff visible to the user.
- Verifies the supersedes-link points at a real artifact before
  running; surfaces a clear error otherwise.
- Tool description signals: "use this for SMALL edits to a script
  you produced earlier — change a field, drop a row, adjust
  styling. For anything substantial, call `run_python` from
  scratch."

### Why this matters

The Claude.ai chat transcript that drove this whole arc shows
repeated patterns of: generate → user nudges → small edit. Each
edit was Claude rewriting the prior reportlab script with one
field changed, then re-executing. Travis can now mirror that exactly
— without paying for a regenerate, without losing the prior
artifact, with diff-able lineage for the eventual v0.16 case
substrate work.

## v0.15.2 — Extended thinking, capture goes background, cross-doc reconciliation, drive-the-process prompts (2026-06-08)

Five bundled architectural + behavioural improvements pulling Travis
toward Claude.ai parity.

### Extended thinking on the Claude provider

- Added Anthropic's extended-thinking parameter (`thinking: { type:
  "enabled", budget_tokens: 4000 }`) to every primary agent-loop LLM
  call. The model now gets dedicated cognitive-budget tokens before
  it produces tool calls or the final response — the same machinery
  Claude.ai uses for the visible "Thinking" boxes.
- Thinking content blocks come back in the response, parsed via a
  new `Thinking` variant on the `ContentBlock` enum, and surfaced
  to `ChatTurn.thinking_blocks`.
- In `journal_ingest`, each thinking block becomes a `Note` on the
  active manager step, so the worker's reasoning streams into the
  chat as it happens — same loop-doesn't-quit texture as Claude.ai.
- Cost: ~$0.06/turn at the 4000-token budget. Worth the depth for
  multi-doc reconciliation, constraint solving, forensic analysis.
- Retry path uses a 2000-token budget for the forcing prompt.

### Background capture (architectural split begins)

- New module `src-tauri/src/capture/mod.rs` with `CaptureSnapshot`
  + `run_background`. Task + reminder persistence (the two most
  visible "captured N new" pain fields) moves into
  `tauri::async_runtime::spawn` so the chat command returns
  immediately and persistence runs in the background.
- Emits a `capture-applied` Tauri event with counts so a future
  UI affordance can surface "tracked N in the background"
  notifications.
- Other capture fields (capability_gaps, entities, entity_facts,
  hypotheses, affect_signals, workspace_routing) still run inline
  for now — they touch more shared state and are higher
  refactoring risk. Queued for v0.15.3.

### Cross-document reconciliation prompts

- New `CROSS-DOCUMENT RECONCILIATION` section in the core system
  prompt: "compare overlapping fields across attached docs, flag
  discrepancies, name the authoritative source. A PO authorising
  payment overrides a sample from a previous engagement. A contract
  appendix overrides a downstream pricing sheet. A sign-in sheet
  overrides recollection."

### Drive-the-process recommendation prompts

- New `WHEN ASKED FOR A RECOMMENDATION` section: "lead with your
  recommendation, then justify it. Option-listing without a
  position is a cop-out. Push back on the user's stated instinct
  when it's wrong."

### Document handling rewritten in the core prompt

- Document editing is universal across professions; the core prompt
  now carries the generic `sample → analyze_styling → run_python →
  iterative refinement` pattern, multi-doc workflow guidance,
  spreadsheet-via-pandas, mid-workflow continuation cues. L2E pack
  fragment trimmed to just the L2E-specific bits: invoice numbering
  formula, default rates, the L2E-specific field enumeration,
  structured-action shortcuts.

### UI polish

- `StepRow` now auto-expands while a step is running (live thinking
  visible) and auto-collapses on success (clean completed
  view). Failed steps stay expanded so errors are surfaced.
- Multi-line `Note` content (thinking blocks especially) renders
  with `whitespace-pre-wrap` and consistent spacing.
- Completed steps show their summary as a muted "→ delivered" /
  "→ asked specific question" trailing line.

## v0.15.1 — Manager loop: the worker no longer gets to bail (2026-06-08)

Five releases of prompt-level enforcement (banned phrases, governing
principle, future-tense prohibitions, drive-the-process directive)
and the worker LLM kept producing handoff replies anyway — "reading
them now", "I'll generate", "give me a moment". The fix is
architectural, not prompt-engineering.

### The manager loop

Following the pattern visible in Claude.ai's chat (multiple
`Thinking` boxes per user turn — manager-driven sub-passes), Travis
now wraps the existing agent loop in an **outer manager loop** that
refuses to return until the worker actually delivered or asked a
real question.

- New module `src-tauri/src/manager/mod.rs`:
  - `ProgressKind { Delivered | AskedBlocker | Handoff }`
  - `evaluate_progress(extraction, generated_doc_ids, tool_calls_made)`
    — deterministic Rust function. Inspects the worker's output
    and decides which bucket. Detects placeholder/handoff phrases
    structurally, not by hoping the prompt holds.
  - `continuation_directive()` — the user-role forcing message
    injected between manager iterations to push the worker.
- `journal_ingest`: agent loop wrapped in `'manager: loop` with cap
  `MAX_MANAGER_ITER = 3` per user turn (PER request/response, NOT
  cumulative across the conversation). Each manager pass also gets
  `MAX_ITER = 8` tool-call rounds in the inner agent loop. Between
  manager iterations:
  - The worker's prior reply is appended as an assistant message.
  - The continuation directive is appended as a new user message.
  - The agent loop runs fresh with the augmented context.
- Each manager pass emits a visible `Thinking` step:
  - Pass 1 → "Working on it"
  - Pass 2 → "Forcing progress (pass 2)"
  - Pass 3 → "Forcing progress (pass 3)"
  Each completes with one of: `delivered` / `asked specific question`
  / `handoff — re-running` — same loop-doesn't-quit texture
  Claude.ai's chat has.
- The worker LLM is unchanged. The manager is just a Rust function
  watching what comes out.

### Why this matters

Worker LLMs are non-deterministic — Claude's training sometimes
overrides even very explicit prompt rules. The manager is a
*deterministic backstop*. If the worker says "reading them now" when
it should have read the docs, the manager detects the placeholder
phrase, injects a forcing directive, and runs the worker again. By
manager pass 2 or 3 the worker has either delivered or named a
specific blocker.

Cost guard: worst case is 24 LLM calls per user turn (3 manager × 8
agent). Typical case lands in 1 manager pass; the manager only
re-runs when the worker actually bailed. The v0.14.4 retry-on-empty
is now structurally redundant (manager catches empty responses too)
but kept in place as an extra inner safety net.

### Deferred to v0.15.2+

- File-level capture refactor (separate `capture::run_background`
  module + `tokio::spawn` second LLM call).
- Keychain file-fallback (only if your diagnostic info shows
  Windows Credential Manager is the issue).
- Opus 4.8 default model A/B against Sonnet 4.6 (only meaningful
  once the manager loop is proven — otherwise we won't know which
  fix did the work).

## v0.15.0 — Claude.ai-parity core + L2E moves to the pack (2026-06-08)

Minor-version bump reflects an architectural shift in the prompt
layering: the core system prompt is now domain-agnostic, and all
L2E-specific guidance (invoices, POs, sign-in sheets, schools,
$rates, services catalog) moves to the L2E pack's prompt fragment.
The chat surface should now feel like Claude.ai for general use,
with vertical depth layered on per enabled pack.

### Core prompt — generalist baseline

- Opening framing: "You are Travis — a personal AI assistant. You
  can help with anything Claude.ai can: writing, analysis, code,
  research, creative work, document handling, scheduling, and
  ops capture."
- Tool catalog organized by capability (writing/code/documents/
  memory/scheduling) instead of a flat alphabetic list.
- Examples cover non-ops use cases — drafting an email, analysing
  a spreadsheet in Python, summarising a project plan, pulling
  details from past memory. No invoice / PO / school references
  anywhere in the core prompt.
- Governing principle ("HOW YOUR TURN ENDS"), future-tense ban,
  and document-handling rules stay — they're universal.

### L2E pack fragment — domain depth

- Extended `src-tauri/src/packs/lead_to_empower/mod.rs::PROMPT_FRAGMENT`
  with the full invoice-generation workflow guidance previously
  embedded in the core:
  - PO + WO + sign-in-sheet → invoice PDF flow.
  - Invoice numbering rule (year + school code + sequence).
  - Default-rate references (Leadership Coaching ~$1,500/day
    school-funded vs $2,300/day DoF-funded).
  - Sample-→-analyze_styling-→-run_python pattern.
  - Spreadsheet handling with pandas.
  - Sample-→-adapt prompt template (the field-by-field
    enumeration pattern with `Bill to`/`Invoice #`/`Service
    dates`).
  - Workflow-continuation cues (mid-invoice doc uploads,
    numbered answers, constraints).
  - run_python vs structured action choice.

### Capture leaves the chat — behavioural split

- Primary LLM is told explicitly: "leave `tasks`, `entities`,
  `reminders`, `capabilityGaps`, `entityFacts`, `hypotheses`,
  `affectSignals`, `completedTaskIds`, `clarifyingQuestions`,
  `workspaceRouting`, `genericEntities` EMPTY. Don't narrate
  captures. A separate pipeline handles them."
- Inline persistence remains for now — anything the LLM does still
  emit gets stored silently. v0.15.1 will land the architectural
  split (separate `capture::run_background` module + `tokio::spawn`
  + dedicated capture-only LLM call).

### Why the bump

This is the first release where the prompt is no longer pack-locked.
If someone disables L2E or ships Travis with a different pack
(tutoring, consulting), the core behaviour stays sensible —
generalist by default, vertical depth on top.

### Keychain diagnostics

User reported "Claude API key not found in your OS keychain"
recurring even after re-entering the key. The generic error gave
no way to tell whether the keychain wasn't being written to, was
returning an empty entry, or the OS itself was misbehaving.

- `secrets::lookup_api_key` returns a `KeyLookup` enum
  (`FromCache` / `FromKeychain` / `NoEntry` / `EmptyEntry` /
  `KeychainError(msg)`).
- The "key not found" error in `llm::build` now names the actual
  failure mode:
  - **NoEntry** → "Open Settings → LLM Provider and enter your key."
  - **EmptyEntry** → "key in your OS keychain is empty, re-enter."
  - **KeychainError** → "OS keychain returned an error: {msg}. The
    key may have been stored under a different OS account, or the
    keychain access is locked."
- INFO-level tracing line on every successful keychain read with
  the character count, so the dev console can confirm what's
  happening.

A file-based fallback (for users hitting Windows Credential Manager
issues) is queued for v0.15.1 once we know whether the problem is
upstream-keyring or environmental.

## v0.14.5 — Drive the process: ban future-tense replies, fix Excel preload, visible doc reading (2026-06-08)

Real-world testing of v0.14.4 surfaced three new behaviours: Travis was
(1) writing future-tense placeholder replies ("Reading the docs now",
"I'll generate the invoice with number…") and ending the turn before
doing the work, (2) erroring out when given Excel master sheets
(spreadsheet content blew up the doc preload), and (3) showing nothing
visible during doc preload so the user perceived dead time.

### Future-tense is banned in `response`

- Hard prompt directive: "If your response contains 'I'll generate',
  'I'll create', 'I'll extract', 'reading them now', 'let me check',
  'working on it', 'give me a moment', 'I'll come back', etc., you
  have FAILED this turn. Go call the relevant tool(s) BEFORE writing
  the response. Then report the result in PAST TENSE."
- The `response` field description in the JSON schema now spells the
  same rule with concrete bad/good examples — e.g. *bad*: "I'll
  generate the invoice with number 2026217002"; *good*: "Generated
  invoice 2026217002 — total $15,000 over 10 days (link below). I
  assumed the IS 217 default rate of $1,500/day from the services
  catalog; let me know if that needs adjustment."
- New "HOW TO DRIVE A MULTI-DOC WORKFLOW" prompt section: "ASSUME you
  have what you need. The user gave you 5 documents — that's not a
  trial balloon, that's the input set. Use them."

### Spreadsheet doc preload — tight summary, not full content

- v0.14.3/.4 preloaded the full extracted_json for every attached doc
  into the user message. For 380KB master sheets, that exploded the
  context and the LLM errored. v0.14.5 detects spreadsheets by mime
  type / extension and replaces the full content with: a 400-char
  structural preview plus the instruction "Spreadsheet — mounted at
  /inputs/<file>. Use run_python with pandas (pd.read_excel) to read
  it. DO NOT request the full content here; query it in Python."
- Mount filename is sanitised to match the interpreter's path-safety
  rules (`src/interpreter/main.tsx`'s safeName regex).

### Visible doc reading

- Doc preload now wraps in a `Step` (the same substrate the tools use)
  so the user sees `Reading attached documents · 3 docs` in the chat
  with per-doc notes streaming as each one is loaded. No more dead
  air between sending and Travis's first tool call.

### Full model power on every turn

- The Haiku tier-down for capture-style turns is **disabled**. Every
  turn now uses the full default model (Sonnet/Opus for Claude). The
  "Travis didn't drive the process" failures are partly a
  model-quality story, and we'd rather pay cents than ship a weaker
  experience. Re-introduces the tier once the background-capture
  split lands and capture truly runs in its own process.

## v0.14.4 — Unblock the empty-response dead-end + user-message visibility (2026-06-07)

v0.14.3 enforced the "finish or ask" rule by deleting the synthesis
fallback, but it traded the polite-placeholder problem for a hard-error
dead-end: when the LLM agent loop couldn't produce a reply, Travis
surfaced "Travis didn't produce a reply on that turn" and the user had
no recovery path. This release fixes that and a couple of related
chat-UX regressions.

### Empty-response retry

- When the primary agent loop returns no usable response, Travis now
  runs **one retry** with a forcing prompt ("Your previous attempt
  returned no `response` value. Re-read the HOW YOUR TURN ENDS rules
  — call report_extraction now with a substantive `response`
  value."). `max_tokens` bumped to 2000 for the retry; system prompt
  is cached so the second call only pays for the forcing tail.
- If the retry also returns empty, Travis now shows a **specific
  error** based on what went wrong — "ran out of tool-call
  iterations, try sending fewer documents at once", "transient
  parse error, please try again", etc. — instead of the bare "open
  the dev console" message.
- `tracing::warn!` lines log `err_msg` + raw-response length on every
  empty-response path so the dev console can show why each retry
  fired.

### User-message visibility

- `flushSync` the optimistic-message state update so the user bubble
  paints to DOM *before* React commits the busy=true / live-turn
  rendering churn. Without it, React's batching could render both
  updates in one frame and the live-turn would push the just-sent
  message above the smart-scroll fold.
- After the optimistic commit, Travis scrolls the new user bubble
  into view at the top of the visible area
  (`scrollIntoView({ block: "start" })`) so it's anchored even when
  the bubble + live-turn together exceed one viewport height.
- Each `ChatTurn` now carries a `data-message-id` attr so the
  scroll-into-view query has a stable target.

### Deferred to v0.14.5

The full background-capture LLM-call split (separate `tokio::spawn`
extraction pipeline) is held for v0.14.5 — it's a larger refactor and
the user is blocked *now*. v0.14.4 is the unblock.

## v0.14.3 — Governing principle: finish or ask, never hand off (2026-06-07)

The "captured 1 new" / "reading them now" / "I'll come back" pattern
was breaking the chat: Travis was handing the conversational turn back
to the user before finishing the work. This release enforces the
governing principle end-to-end and gets capture out of the chat path.

### The governing principle

A new top-of-prompt section drills the rule:

> Your turn ends ONLY when one of these is true:
> 1. You delivered an artifact.
> 2. You asked a SPECIFIC question that the user must answer.
> 3. You hit a real blocker.
>
> "I'll come back with what I found", "reading them now", "give me a
> moment", "working on it", "captured", "noted", "got it" are NOT
> acceptable as a complete reply. Use your tool-call iterations to
> DO the work.

### Capture leaves the chat path

- Primary LLM is told to leave `tasks`, `entities`, `reminders`,
  `capabilityGaps`, etc. empty. Capture is invisible to the chat.
- The synthesis fallback that produced "captured N new" / "Working
  on it" / "Got the document(s) — reading them now" is **deleted**.
  No more polite placeholders standing in for real work.
- Captures the LLM does emit anyway are still persisted to the DB
  silently — the chat just never mentions them. (Architectural
  split into a separate background LLM call ships in v0.14.4.)

### Tool headroom

- **`MAX_ITER` 4 → 8.** With capture extraction off the primary
  pass the model has way more room to call tools — read_document,
  analyze_document_styling, then one or two run_python passes —
  before finalizing.

### Document preload

- When the user's message references attached documents
  (`doc#N` markers), Travis now sees the documents' extracted
  content on iteration 1 — pre-injected into the user message
  under `== ATTACHED DOCUMENTS (pre-extracted summary) ==`. The
  LLM doesn't have to spend a tool-call iteration on
  `read_document` just to see what's there; it can spend that
  iteration on `analyze_document_styling` or `run_python`
  instead. Falls back gracefully (the LLM can still call
  `read_document` for the full body).

### Chat UX

- **Hover jerk fixed.** The copy/delete action row now reserves
  its space and fades opacity in on hover instead of mounting on
  demand. No more bubble jump on hover.

## v0.14.2 — Chat persistence, message actions, workflow continuation (2026-06-07)

Second feedback batch. Three categories of fix:

### Conversation persistence

- **Chat survives tab switches.** Active conversation id lives in
  `localStorage` (`travis.activeConversationId`) and is the authoritative
  restore source; the backend's `most_recent_awaiting_user` heuristic is
  a fallback for first-run only. Switching from Ask to Notes and back
  no longer wipes the transcript.
- **"New chat" is the only reset path.** Travis never clears the chat
  on its own, even when a workflow finishes.

### Per-message actions

- **Copy + delete on every bubble.** Hover (or focus) any message to
  reveal a small action row. Copy puts the message body on the
  clipboard.
- **Delete trims forward.** Deleting a message removes that message
  and every message after it in the thread (Claude.ai-style), keeping
  the surviving transcript coherent. A confirmation prompt always
  shows first; nothing is removed without an explicit click.
- **`delete_message_and_after` Tauri command.** One SQL `DELETE`
  scoped to the conversation; orphaned step rows stay (they belong to
  the conversation, not the turn).

### Chat-input UX

- **No more user-bubble flash.** Optimistic messages now keep their
  React key across the round-trip — the server thread is merged into
  the existing list instead of replacing it, so `AnimatePresence`
  sees no unmount.
- **Instant file-attach feedback.** Dropping or picking a file shows
  a `reading…` placeholder pill the same frame the path comes in;
  the placeholder swaps in-place for the real document card when
  ingest finishes.
- **Smart scroll.** The transcript jumps to bottom on first load and
  follows new content only when the user is already at the bottom.
  Scroll up and Travis stays where you parked. A floating "jump to
  latest" pill appears when there's new content off-screen — click
  it to come back down.

### Workflow continuation — the "captured 1 new" bug

Three reinforcing changes:

- **`response` field is now required** in the LLM JSON schema with
  `minLength: 1` and an explicit prompt directive: "NEVER respond with
  just 'captured' or 'noted' — write a substantive reply that
  advances the work."
- **Workflow-aware fallback.** When the LLM returns an empty
  response AND there's an active workflow OR the user just uploaded
  documents, the synthesised reply is "Got the document(s) — reading
  them now. I'll come back with what I extracted and any open fields"
  instead of "captured N new".
- **Task suppression mid-workflow.** When the user's message contains
  `doc#N` markers AND there's an active workflow on the thread, any
  tasks the LLM tried to extract are dropped before persistence with a
  `tracing::info!` line — preventing the chat from ever showing
  "captured" on a mid-workflow document upload, even if the model
  hallucinates tasks.

## v0.14.1 — Chat-loop and offline polish (2026-06-07)

First feedback batch after v0.14.0. Travis stopped "hanging up" mid-workflow,
the chat panel now actually behaves like a chat, and the Pyodide runtime ships
in the bundle instead of being fetched from a CDN on first run.

### Chat UX

- **Steps land with the message they belong to.** Pre-fix the SQLite step
  timestamps were string-compared against RFC3339 conversation timestamps;
  the encoding difference pushed every step block to the bottom of the
  transcript. Now both are normalised through `Date.parse` with a 5 s
  tolerance, so a "Reading document" or "Run Python" group renders directly
  under the Travis reply that triggered it.
- **Auto-growing textarea + send button.** Replaces the single-line input;
  wraps text up to 8 rows before scrolling, Enter to send, Shift+Enter for
  a newline, explicit click target for touch users.
- **Attached files show up in the user bubble.** PDFs and docs as a
  clickable card, images as an inline preview. Click to open in the
  document viewer.
- **"Your turn" cue.** When the last message is Travis and `busy === false`,
  the input border glows + caret auto-focuses so it's obvious he's done.
- **Scroll history is preserved.** The flex container was missing
  `min-h-0`, which silently capped the transcript at the viewport. Now
  the full conversation scrolls.

### Reasoning + step streaming

- **`thinking` field** on extractions. The LLM emits a short
  one-sentence narration of what it's about to do; the chat surface
  renders it under the assistant bubble as muted text.
- **Action handlers stream steps** like tools do — `ActionRegistry::dispatch`
  now wraps every handler call in a `Step` with a human-readable label
  ("Saving invoice", "Logging activity") instead of the internal kind.

### Workflow continuation prompt

- **No more "captured 1 new" mid-workflow.** The system prompt now
  has a dedicated section explaining that when Travis is mid-workflow
  (he just asked for fields), the user's reply is slot fill, not a
  fresh capture. Mirrors the Claude.ai pattern where a workflow stays
  active until the artifact is generated or the user changes topic.

### Pyodide bundle

- **Local Pyodide bundle via `vite-plugin-static-copy`.** `pyodide.asm.wasm`,
  `pyodide.asm.js`, `pyodide.mjs`, `python_stdlib.zip`, and the lock file
  are copied into `dist/pyodide/` at build time; `loadPyodide({ indexURL: "/pyodide/" })`
  picks them up. No CDN call on first launch — works fully offline.

## v0.14.0 — Code execution + Claude-class chat (2026-06-08)

Travis can now do anything a smart user asks of it via in-app Python
execution, multimodal visual styling analysis, and a chat surface that
shows its work — without losing its persistent-memory + local-first
vertical-pack advantages. The end-state of the v0.14 spec
(`V0_14_0_SPEC.md`).

### What Travis can now do that it couldn't before

1. **Write Python in the moment** to generate any document layout that
   doesn't fit a hardcoded template. Sample-matching invoices,
   sign-in sheets matching a customer's template, constraint solving
   (find quantities that sum to $X exactly), reading .docx files,
   auditor-style cross-document reconciliation.
2. **See sample document styling.** Drop a sample PDF; Travis sends
   it to Claude vision, gets back structured JSON of header colours,
   fonts, table layout, signature placement. Feeds the JSON to the
   Python code so generated documents match the sample.
3. **Show its work step-by-step.** Every tool call, code execution,
   and reasoning step renders inline in the chat with name +
   checkmark + duration. Expandable for notes. No more silent
   "thinking…" spinners.
4. **Maintain multi-day cases.** A "case" survives across
   conversations with a rolling summary and decisions log. Resume
   "the PS 89 reconciliation" 3 days later and Travis picks up
   exactly where he left off.
5. **Save successful generations as reusable templates.** After Taylor
   confirms a custom-generated IS 217 invoice looks right, Travis
   saves the styling + working Python. Next time she invoices IS 217,
   the saved code runs instantly — no re-analysis, no fresh code
   generation.

### Slice-by-slice

**Slice 1 — Code interpreter substrate.**
- Hidden Tauri webview window hosts Pyodide (CPython compiled to WASM)
  with preinstalled reportlab/openpyxl/pypdf/pandas/pillow/python-docx
- New `interpreter` module + `run_python` Tauri command + LLM tool
- Documents mounted at `/inputs/`, outputs collected from `/outputs/`
- Outputs auto-register as Documents via the v0.12 substrate

**Slice 2 — Step-streaming backend.**
- Every tool call wraps in a Step (RAII helper) emitting typed events
- New `step` table for persistence; chat UI subscribes to live events
- Human labels ("Reading PO doc" not "read_document")
- Startup cleanup marks pre-crash 'running' steps as cancelled

**Slice 3 — Chat UI v2 (Claude-class).**
- Collapsible thinking sections, named steps with checkmarks
- Syntax-highlighted code blocks (`prism-react-renderer`) with copy
- Markdown rendering with tables/lists (`react-markdown` + `remark-gfm`)
- Inline file preview cards with OS default viewer integration
- Live step streaming during in-progress responses

**Slice 4 — Multimodal visual styling.**
- `analyze_document_styling` Tauri command + LLM tool
- Reuses Claude's native PDF input (same v0.12 mechanism, new prompt)
- Returns structured JSON: colours, fonts, layout, signature, margins
- Cached on `document.styling_json` for instant reuse

**Slice 5 — Fast/escape path dispatcher.**
- `WorkflowDef` gains `allow_code_escape` + `code_escape_hint`
- LTE invoice + sign-in-sheet workflows allow escape with detailed hints
- System prompt teaches when to use structured action vs `run_python`

**Slice 6 — Long-running cases.**
- New `travis_case` + `case_artifact` tables
- `open_case` / `note_case` / `close_case` / `find_case` LLM tools
- Active cases injected into journal prompt (same shape as initiatives)
- Frontend Tauri commands for case management surfaces

**Slice 7 — `pack_template` memory.**
- New `pack_template` table (workspace, pack, kind, label, counterparty)
- `save_pack_template` / `find_pack_template` / `get_pack_template` tools
- Saved styling JSON + Python code; counterparty-matched lookups
- `used_count` + `last_used_at` for "most reused" surfacing

**Slice 8 — Verification + version bump.**
- Acceptance scope: Taylor's 5 real tasks from the Claude.ai
  conversation (IS 217 invoice from sample, PS 19-style sign-in sheet,
  PS 89 reconciliation with smoking-gun mislabel, constraint solving,
  mid-conversation correction)
- Version 0.14.0 across package.json + Cargo.toml + tauri.conf.json
- Pyodide loads from jsdelivr CDN for v0.14 dev cycle; future polish
  bundles locally for offline use

### New migrations

- `0030_steps.sql` — step events persistence
- `0031_document_styling.sql` — cached styling JSON
- `0032_cases.sql` — travis_case + case_artifact
- `0033_pack_templates.sql` — reusable styling + code per counterparty

### Bundle size

Main JS bundle grew from 284 KB → 537 KB (gzip: 158 KB) from
markdown + syntax highlighting + chat components. Pyodide loads
lazily from CDN. Acceptable cost for the capability unlock.

---

## v0.13.5 — Pin tauri-runtime/wry to ~2.10 (2026-06-07)

v0.13.4 cleared the JS↔Rust version preflight (4m29s — got into the
actual Rust compile) but failed deep in `tauri-2.10.3/src/webview/
mod.rs:707` with a `Fn + Send` vs `Fn + Send + Sync` trait mismatch.
Root cause: the `tauri` crate is pinned to 2.10.3 but its transitive
deps `tauri-runtime` and `tauri-runtime-wry` weren't pinned, so cargo
lifted them to 2.11.2 — the newer runtime traits don't match what
tauri 2.10's webview implementation expects.

Added explicit `tauri-runtime = "~2.10"` and `tauri-runtime-wry =
"~2.10"` pins in Cargo.toml so the whole tauri 2.10 family stays
together.

---

## v0.13.4 — Pin Rust tauri-plugin-dialog to 2.4.2 (2026-06-05)

`tauri info` locally revealed the real mismatch the CI logs kept
referring to: the **Rust** `tauri-plugin-dialog` crate was 2.7.1 while
the **JS** `@tauri-apps/plugin-dialog` was 2.4.2 — Tauri's preflight
requires same major.minor on both sides. I'd been chasing
`@tauri-apps/api` version issues; the actual culprit was the plugin
itself. Plugin-dialog 2.5+ depends on tauri 2.11, which isn't in
our resolved tree.

Pinned `tauri-plugin-dialog = "~2.4"` in Cargo.toml, regenerated
Cargo.lock — Rust crate now resolves to 2.4.2 matching the JS side.

---

## v0.13.3 — Pin @tauri-apps/plugin-opener to 2.5.3 (2026-06-05)

The real root cause of the v0.13.0/.1/.2 CI failures: `@tauri-apps/
plugin-opener@2.5.4` (released this week) bumped its `@tauri-apps/api`
peer dependency to `^2.11.0`, while every other Tauri plugin in our
tree still uses `^2.8.0`. With our generous `~2.5.0` pin, npm hoisted
the latest matching patch (2.5.4), which triggered the tauri-action
preflight mismatch — even though the API itself was resolving to
2.10.1 via overrides, the *declared peer* in node_modules disagreed.

Pinned `plugin-opener` to exactly `2.5.3` until tauri 2.11 is on
crates.io and we can do a coordinated bump across the entire stack.

---

## v0.13.2 — npm `overrides` to force `@tauri-apps/api` consistency (2026-06-05)

v0.13.1's `~2.10.0` direct pin on `@tauri-apps/api` wasn't enough — the
new `@tauri-apps/plugin-dialog` carries `@tauri-apps/api: ^2.8.0` as a
transitive dependency, and npm hoisted the unbounded latest (2.11.0)
into the tree even with the lock present. Added an `overrides` block
that forces every reference in the tree to `~2.10.0`, regenerated the
lock from scratch. Also pinned `@tauri-apps/cli` to `~2.10.0` so the
build CLI stays aligned with the runtime crate.

---

## v0.13.1 — Pin @tauri-apps/* npm packages to ~2.10 (2026-06-05)

The v0.13.0 build failed in CI because npm install picked up
@tauri-apps/api@2.11.0 (latest) while the Rust `tauri` crate is still
2.10.3 on crates.io. Tauri's preflight check rejects the major/minor
mismatch. Pinned all four @tauri-apps/* JS packages to `~2.10.x` /
matching minors so the npm tree stays aligned with the Rust crates
until tauri 2.11 publishes to crates.io. Code unchanged from v0.13.0.

---

## v0.13.0 — Five-piece response to Taylor's first real test (2026-06-04)

Taylor's feedback after using v0.12.3 against her real workflow:
1. "Engagement and contract is too broad — might mean the same thing"
2. "We don't always invoice the full amount at once. A contract can
   have many invoices until the amount is complete"
3. "Upload the PO (or WO) and Travis can create a contract from it"
4. "There's no way from the UI/Ask/chat interface where files can be
   uploaded"
5. "The UI should show that files have been uploaded and the workflow
   drive should always be running/active"

All five land in this release.

### Collapsed contract + engagement into one record (LTE pack v0.7.0)

The two-table distinction was an abstraction I added that didn't match
her real work. Migration `0005_collapse_contract_engagement` extends
the `engagement` table with every contract-shape field (`ref`,
`ceiling_cents`, `term_start`, `term_end`, `signed_at`,
`parent_solicitation`, `pdf_path`, `counterparty`, `contract_status`),
backfills data from any standalone `contract` rows, and synthesises
engagement rows for orphan contracts so no data is lost. The standalone
`contract` table stays for backward compat but is hidden from the
sidebar — engagement IS the contract now.

UI / chat / extraction prompts say "Contract" everywhere. The SQL
table stays named `engagement` for code stability — only labels change.
Pack prompt fragment has an explicit "in this app, contract and
engagement refer to the same record" note at the top so the LLM
doesn't drift back to the old vocabulary.

### Many invoices per contract — draw-down tracking

`propose_program_invoice_draft`'s reply now includes a draw-down line:
"Draw-down: $5,500 invoiced of $7,064 total · $1,564 remaining". If
the new invoice would push past the contract ceiling, the reply warns
"⚠ over ceiling by $X". `lte_find_contract` already surfaced
invoiced/remaining/burn percent; that surface now queries the
engagement table directly.

### PO/WO → contract

New workflow recipe `lte_create_contract_from_doc` (slots: source
document, kind = `po` or `wo`). New action handler
`CreateContractFromDocHandler` extracts vendor, school, period, total
from the document's extracted JSON, resolves/creates the school,
inserts the contract (engagement row) with all fields pre-populated,
and links the source document via `document_link`. The same workflow
takes PO or WO — both represent a contract per Taylor.

### File upload in AskTab (main app chat)

The Ask tab in Manage was a chat surface with no file affordance.
Now mirrors the overlay's wiring:
- Drag-drop listener on the main window
- Paperclip button → `tauri-plugin-dialog`'s native file picker
  (added new dependency `tauri-plugin-dialog` + capability)
- Chip strip showing attached documents
- Each chip expands to the same `DocumentExtractCard` from the
  overlay
- Submit appends `[Attached: name (kind, doc#N)]` to the chat payload
  so the LLM sees the attachment, then clears the strip

### Active workflow indicator (always-visible status)

New `ActiveWorkflowPill` React component, rendered above the input
in both AskTab and the overlay. Shows what Travis is currently
working on, how many slots are filled, what's still missing, and
what the next ask is. Refreshes via a new `workflow-state-changed`
event the backend emits after processing workflow ops, so it stays
in sync without polling. Tappable to expand into the full slot
breakdown.

### Backend changes

- New `tauri-plugin-dialog` plugin registered, `dialog:default` and
  `dialog:allow-open` capabilities added.
- New Tauri command `get_active_workflow(conversationId)` →
  `WorkflowSurface` (recipe info + per-slot filled state + next ask).
- New `workflows::cmd` module.
- New action `lte_create_contract_from_doc` registered with the
  action registry.
- `lte_find_contract` tool rewritten to query `engagement` (with
  the new contract fields) instead of the legacy `contract` table.

---

## v0.12.3 — In-app update banner (2026-06-04)

The v0.12.2 background poll already fires a native OS notification
when a new version is available; this release adds a non-intrusive
in-app banner so the prompt is visible inside Travis itself even when
the OS notification has been dismissed or notification permission is
not granted. Banner appears at the top of the main window, shows the
new version number, and has Install / Dismiss buttons. Dismissals are
per-version per-session.

---

## v0.12.2 — Auto-update polls in the background (2026-06-04)

Travis no longer requires Taylor to remember to check Settings for
updates. A background poll runs every 4 hours: when a newer version is
published in the release feed, Travis emits an `update-available` event
the frontend can listen for AND fires a one-shot system notification.
First check happens ~60 seconds after launch (gives the app room to
settle into its other startup tasks). Once-per-version dedup so back-
to-back polls don't re-notify the same version twice in a session.

The existing Settings "Check for updates" button still works as the
manual path; the new poll just removes the need to remember it.

---

## v0.12.1 — Derive a sign-in sheet from the master Google-Sheet export (2026-06-04)

Taylor's workflow: a Google Form fills a master Google Sheet with every
coach-hours entry across every school LTE serves. To get a sign-in sheet
for one principal to sign, she manually filters down to one engagement
and reformats. That filter-and-reformat step is now Travis's job.

### What ships

- **CSV + XLSX ingestion.** Drop a `.csv`, `.xlsx`, `.xls`, `.xlsm`,
  `.xlsb`, or `.ods` file into the chat overlay; Travis stores it via
  the existing document substrate. New `calamine` and `csv` crates
  handle the read.
- **`coach_hours_master` extraction prompt.** The LLM reads the
  spreadsheet text, infers column mappings (`Coach Name` / `Site` /
  `Date` / `Hours` / `Notes` — variants welcome), normalises dates to
  ISO, returns every row as structured JSON. No filtering at extract
  time — the workflow does that.
- **New workflow recipe `lte_derive_sign_in_sheet`.** Slots: master
  spreadsheet (Document), engagement (Entity), period (DateRange).
- **New action handler `DeriveSignInSheetHandler`.** Loads the
  extracted rows, filters by school name (fuzzy match against the
  engagement's school) AND date in period AND has-coach/hours,
  upserts the matched rows into `coach_hours` (dedup by coach + school
  + date), renders the printable PDF via the existing
  `render_sign_in_sheet`, registers the result as a Travis-generated
  document for round-trip.
- **Skip report.** The confirmation message says how many rows
  matched, how many were dropped for wrong school / out of period /
  missing fields — so Taylor catches data-quality issues at the
  master-sheet level.

### Example flow

```
Taylor: derive a sign-in sheet for math at PS498 for January

Travis: [asks for the master sheet if not already attached]
Taylor: [drops Hours_Master.xlsx]
Travis: read 437 rows. Engagement = math team coaching at PS 498?
Taylor: yes

Travis: 18 matching rows for math at PS 498 between 2026-01-01 and
        2026-01-31 (3 new, 15 already on file, 419 skipped — wrong
        school or out of period). PDF saved to Downloads. Want to
        open it?
```

### What's next (Path B, not in this release)

Native Google Sheets integration — Drive `.readonly` OAuth scope, thin
Sheets client, configurable sheet-id/tab/column mapping per workspace.
Removes the manual-export step. Tracked alongside WORKFLOWS_BACKLOG.md.

---

## v0.12.0 — Docs-first workflows: ingest, extract, reconcile, preview (2026-06-04)

Travis now meets Taylor where she actually works — documents (POs, work
orders, signed sheets, contracts) as first-class inputs and outputs. She
states intent ("invoice PS498 for Jan-Feb"); Travis drives the workflow,
asks for the inputs it needs (drop the PO, drop the signed sheet, or
reuse what's linked), extracts structured data, reconciles across docs,
and proposes the draft. The same engine generalises to any pack's
workflow shape — [WORKFLOWS_BACKLOG.md](./WORKFLOWS_BACKLOG.md) enumerates
the capabilities core needs for full horizontal scale.

### Slice 1 — Workflow recipes + dialogue manager

- New `workflows` module: `WorkflowDef` / `Slot` / `SlotKind` types, per-
  conversation `workflow_state` table, dialogue manager that renders
  "what's filled · what's missing · what to ask next" into the LLM prompt.
- LLM drives transitions via a new `workflowOps` field on the journal
  extraction schema — `start` / `fillSlot` / `complete` / `abandon`.
- Migration `0028_workflows.sql`.
- `PackHandle::workflows()` lets each pack contribute recipes (mirrors
  `register_actions` / `register_tools`). Framework in core, recipes in
  packs.
- LTE pack ships its first recipe: `lte_generate_invoice` (slots: school,
  engagement, period, PO, signed sheet, optional WO).

### Slice 2 — Document substrate

- New `documents` module: `document` + `document_link` tables.
- Content-addressed file storage at
  `<app_data>/documents/<hash_prefix>/<hash><ext>` — duplicate drops
  dedup automatically.
- Tauri commands: `ingest_document`, `list_documents`, `get_document`,
  `get_document_path`, `link_document`, `set_document_kind`,
  `delete_document`.
- Drag-and-drop affordance in the chat overlay — Taylor drops a PDF, it
  hashes, copies, and surfaces as a chip above the input.
- Migration `0029_documents.sql`.

### Slice 3 — Read & digest

- PDF text-layer extraction via `pdf-extract` crate (pure Rust, no
  native deps).
- Kind-specific extraction prompts for PO / WO / signed sheet / invoice /
  contract — LLM in JSON mode produces structured fields.
- Fire-and-forget background extraction on ingest; `extract_document`
  Tauri command for manual / forced re-extraction.
- New LLM tools: `read_document`, `find_documents`.

### Vision fallback for scanned PDFs

- When `pdf-extract` returns no text layer (paper sheets faxed/scanned
  back), Travis sends the PDF bytes directly to Claude via the native
  `document` content block — no PDFium / Tesseract / OS-side OCR
  needed. Claude OCRs and returns the same JSON shape as text-path
  extraction.
- New `LlmProvider::extract_pdf(bytes, prompt, max_tokens)` trait
  method. Claude implements; OpenAI and Ollama return a clear "switch
  to Claude in Settings for scanned PDFs" error.
- 30MB cap per file; bigger PDFs need page-splitting (future).

### Slice 4 — Doc-entity round-trip wiring

- Every Travis-generated PDF (invoice, work order, sign-in sheet) now
  registers as a `document` row with `source = generated_by_travis`.
- Round-trip: the PDF Travis emits is the same shape it can ingest later.
- `register_generated_document` helper in `documents::cmd` — packs call
  it after writing their PDFs.

### Slice 5 — Multi-doc reconciliation

- New `reconcile_documents` LLM tool: walks N documents' extracted JSON,
  flags PO-number mismatches, school-name mismatches, period-window
  inconsistencies, PO-vs-invoice total mismatches.
- Travis uses this when multiple document slots are filled on the active
  workflow, *before* proposing the finalize action — so inconsistencies
  surface in chat rather than in the rendered invoice.

### Slice 6 — Modify / regenerate

- `update_document_extraction` Tauri command for full-overwrite
  corrections to extracted JSON.
- `update_document_field` LLM tool for surgical edits ("change line 2
  unit price to $5031.30") via dot-path. Source PDF never modified —
  only the structured layer Travis reasons over.
- Generated PDFs round-trip via Slice 4: re-emitting after a data
  correction re-registers the new PDF automatically.

### Preview

- `preview_document` Tauri command + LLM tool open any stored document
  with the OS default viewer (Preview / Acrobat / browser / Excel) via
  the existing `tauri-plugin-opener`. Taylor says "show me that invoice",
  Travis opens the PDF.

### Extraction confirmation cards

- New `DocumentExtractCard` React component. Each attached-doc chip in
  the overlay is now a toggle — tap to expand into a card showing every
  extracted field, nested arrays (line items) rendered as sub-groups.
- Inline editing: tap any field, type, hit save. Edits dispatch
  `update_document_extraction` (full overwrite) — the source PDF is
  untouched. Re-extract button forces a fresh extractor run.
- "View source" button opens the original PDF via `preview_document`.
- Card refreshes automatically when the backend emits the
  `document-extracted` event after the background extractor finishes.
- Type coercion on save: numeric strings become numbers, "true"/"false"
  booleans, empty strings null. Conservative; preserves shape.

### Backlog

- [WORKFLOWS_BACKLOG.md](./WORKFLOWS_BACKLOG.md) — exhaustive list of
  workflow framework capabilities core needs to scale horizontally
  beyond LTE-shape (slot kinds, branching, loops, sub-workflows,
  external-action finalisers, multi-actor approval, audit trails).

### Dependencies

- `sha2 = "0.10"` — file-content hashing for documents.
- `pdf-extract = "0.7"` — text-layer extraction.

---

## v0.11.0 — BRAIN.md capabilities #2-#7 complete (2026-05-21)

Travis goes from "graph-aware operations assistant" to "partner
that thinks alongside you" — the seven BRAIN.md capabilities are
now substrate-complete. Plus a macOS keychain fix that turns
N-prompts-per-session into one.

### Capabilities shipped

- **#2 Personality.** Single source persona module
  (`src/persona/mod.rs`) — values + voice + hard-line constraints
  (Travis v1). Per-user voice corrections accumulate via the
  `update_profile_context` action (append, dedup, bound at 10).
- **#3 Learning others' personalities.** User-model background
  task derives activity patterns (active hours, capture cadence,
  question ratio) into `user_profile.derived_model_json`. Per-
  entity personality slots (contact window, style hint, top
  topics) for person entities with ≥5 mentions, persisted under
  `entity.attributes_json.personality`.
- **#4 Collaboration.** New `initiative` table. Tasks and
  conversations can tag one. `create_initiative` and
  `close_initiative` actions; journal prompt now includes an
  ACTIVE INITIATIVES block so multi-session pushes resume
  without restating context.
- **#5 Proactivity 2.0.** Observer scans the graph every
  proactive tick: mention spikes, signed sheets ready to invoice,
  stale invoice drafts. Findings append as candidate reasons in
  the proactive LLM prompt. Rhythm-aware timing reads the user
  model's peak window and biases toward silence outside it.
- **#6 Self-advocacy.** Recurring unaddressed capability gaps
  (≥3 hits in 14 days) surface as ONE Travis-voice ask through
  the clarifying-questions pipe, with a 7-day cooldown after
  surface. No pestering; soft anti-pestering thresholds.
- **#7 Wellbeing.** Affect-signal extraction (tone + themes)
  per journal capture. Recurring-theme observer detects topics
  the user keeps returning to with concerned/drained tone.
  Persona gains wellbeing constraints (never therapeutic, never
  wellness performance, push back once on self-harming asks).
  Affect data **never** appears in exports.

### Fixes

- **macOS keychain prompt per LLM call.** `secrets.rs` now
  caches API keys in a process-wide OnceLock map. First call
  hits the keychain; every subsequent call reads from memory.
  Same threat model (secret already in process memory when
  used); meaningful UX win on macOS where keychain access
  triggered a password modal per request.

### Privacy posture

Wellbeing affect signals are the most sensitive bytes Travis
generates. The export logic excludes the `affect_signal` table
explicitly. They're not in any pack-queryable surface. Per
BRAIN.md's surveillance-creep failure mode: descriptive
observations only, no aggregation, no transmission, no
prescriptive labels.

### Migrations

`0024_user_model.sql`, `0025_advocacy_cooldown.sql`,
`0026_initiatives.sql`, `0027_affect_signals.sql`. All
additive; existing data unchanged.

## v0.10.0 — Phase 4.5 cognition complete (2026-05-21)

The full BRAIN.md Phase 4.5 build list lands. Travis now thinks
alongside the user with composed graph queries, persisted
reasoning conclusions, multi-turn working memory, recency-aware
ranking, and graded confidence — instead of recomposing intent
from scratch every turn. Substrate work that unlocks the rest of
the cognition roadmap (personality, learning others, proactivity,
self-advocacy, wellbeing).

### Items shipped (BRAIN.md ranking order)

1. **Embedding-based entity retrieval** — `retrieve_semantic`
   cosines against the existing entity index for fuzzy/pronoun-
   shaped queries the exact-name path missed.
2. **Structured fact extraction** — `entityFacts` bucket on the
   journal extractor; each fact persists as a typed claim.
3. **Memory consolidation tick** — background pass every 30 min
   summarises stale entities into stable claim rows so retrieval
   doesn't get noisier over time.
4. **Multi-hop traversal** — `graph_neighbors` LLM tool walks
   `mentioned_with` edges up to 3 hops out with strength ranking.
5. **Confidence in answers** — `ConfidenceBand` (high/medium/low)
   annotated on every GraphHit so Travis can quote certainty
   rather than asserting flat.
6. **Working memory cache** — in-process per-conversation
   hypothesis store with 30-min TTL; multi-turn reasoning
   compounds rather than restarting.
7. **Persisted claims layer** — new `claim` table with
   confidence + source attribution; contradicting claims kept
   side-by-side flagged `contested` rather than silently
   overwritten.
8. **Active forgetting / decay** — 30-day half-life multiplier
   on semantic ranking; ancient strong matches no longer outrank
   recent weak ones.
9. **Per-entity recall tooltip** — capture chips hover-expand
   into a popover showing what Travis remembers about that
   entity (mentions, claims, recent snippets, related entities).
10. **Inference helpers driving conversation** — refinement
    candidates piped into the in-thread clarifying-question
    surface; `*:unknown` entities with 5+ mentions trigger one
    focused question with role suggestions inline.

### Migration

Core migration `0023_claims_and_facts.sql` creates the `claim`
table and adds `entity.last_consolidated_at`. Additive + safe;
existing data unchanged.

## v0.9.0 — Chat-first operations + generic pack bridge (2026-05-20)

The COO can drive the entire LTE billing chain through conversation
without opening a Manage tab. Travis decides per-call whether to
silent-create or confirm-card, asks one focused question per gap
with clickable options rather than typed input, ranks ambiguous
matches by recency + activity, and resumes mid-flow on the next
turn. Pack v0.6.0.

### Highlights

- **Six new LLM-callable handlers** for the LTE chain. Schools and
  coaches are observational (silent creates via tools); contracts,
  engagements, work orders, purchase orders, and coach hours go
  through action confirmation cards (commit to relationships /
  billable artifacts).
- **Four read-only search tools** with ranking + rationale:
  `lte_find_or_create_school`, `lte_find_contract`,
  `lte_find_engagement`, `lte_summarize_context`. Each returns
  ranked candidates so the LLM presents the top match or asks
  between 2-3.
- **Generic pack bridge.** `pack_introspect` lists every enabled
  pack's tables + field schemas; `pack_query` reads rows from any
  table with safe filters (`eq`/`ne`/`lt`/`lte`/`gt`/`gte`/`like`/
  `ilike`/`in`/`isNull`/`isNotNull`), workspace-clamped
  automatically. Field names validated; no SQL injection. Unblocks
  every "Travis, look up …" question across any current or future
  pack.
- **Selection chip UX.** Chat reply parser detects `⊙ ⊕ ⊡ 📅`
  markers. Single-select chips submit on click; add-new chips
  styled subtly differently; multi-select accumulates with a "Send
  selection (N)" button; date chips open the native OS picker and
  submit the chosen ISO date. Pure markdown convention — zero
  schema changes; Travis just emits markers in its reply text.
- **Prompt fragment teaches the loop.** Confirmation policy,
  ambiguity handling, selection markers, resumption cues, and
  bias-toward-action are all spelled out so the LLM doesn't need
  to re-derive intent each turn.

### What this unlocks

> **Taylor:** Create an invoice for PS95.
>
> **Travis** (silently creates PS95, finds three active contracts):
> Saved PS95 as a new school. No engagement yet, and three contracts
> could fit:
> - ⊙ QR179CF — Systemwide Services (38% burn)
> - ⊙ NYCPS HS Math — Supt. White pursuit
> - ⊙ NYCPS Tutoring
> - ⊕ New contract
>
> **Taylor** *(clicks QR179CF)*
>
> **Travis:** Proposing engagement "PS95 — 26-27" under QR179CF.
> Stage assessment. *(Confirm card.)*
>
> *(After confirm…)*
>
> What scope items? You can paste from the WO or pick from the
> catalog: ⊡ Data Coaching, ⊡ Leadership Coaching, ⊡ Instructional
> Coaching, ⊡ School Assessment …

End-to-end without a click into Manage.

## v0.8.0 — LTE contracts: first-class master agreements (2026-05-20)

Promotes contract tracking from a free-text field to a typed table.
The "don't abstract on n=1" guardrail no longer applies — the COO
runs multiple master agreements in parallel, and the spec's deferred
follow-up (`LTE_INVOICING_SPEC.md` §11) ships here. Pack v0.5.0.

### Highlights

- **`contract` table** — ref (unique per workspace), name,
  counterparty, parent_solicitation, term_start/end, ceiling_cents,
  signed_at, status (`draft`/`active`/`expired`/`terminated`/
  `archived`), notes, pdf_path. Primary tab in Manage.
- **Soft FK on the chain.** `engagement.contract_id`,
  `work_order.contract_id`, and `purchase_order.contract_id`
  added. `ON DELETE SET NULL` — deleting a contract leaves its
  history visible rather than cascading away invoices.
- **Backfill, no data loss.** Migration 0004 scans existing
  `engagement.contract_ref` strings, inserts one contract row per
  distinct ref (workspace-scoped), then sets the FKs by string
  match. `contract_ref` stays as a display field for legacy.
- **Two new alerts.** `contract_near_ceiling` (Money): active
  contracts where invoiced ≥ 90% of `ceiling_cents` (skips
  ceiling=0). `contract_expiring_soon` (Action): active contracts
  with `term_end` ≤ 60 days out. Surfaces in Splash like every
  other LTE alert.

### What does *not* break

- Existing `engagement.contract_ref` strings continue to work and
  render. The new FK is additive — set the contract on the
  engagement and downstream WO/PO inherit through the chain.
- `propose_program_invoice_draft` and all PDF generators are
  unchanged. They didn't reference contracts directly; the FK
  routes through the engagement they already use.
- Spec §11 in `LTE_INVOICING_SPEC.md` is now superseded — leaving
  the line as a historical note since the rationale shaped the
  v0.4.0 schema.

## v0.7.0 — LTE invoicing: document layer + validators + PDFs (2026-05-20)

Closes the post-sale half of the Lead to Empower pack. v0.6.0
modeled what LTE sells (catalog) and how it delivers (the 3 A's);
this release handles **turning delivered work into a paid invoice**
through the NYC DOE four-document chain — Work Order → Purchase
Order → Sign-in Sheet → Invoice → Polaris submission.

Driven by the COO's recorded walkthrough and the PS/MS 498 sample
documents (PO `WR260363316`, invoice `LTE2064981`). Spec:
`LTE_INVOICING_SPEC.md`. Pack version `0.4.0`.

### Highlights

- **The document layer.** Two new typed tables — `work_order`
  (vendor-issued, school-countersigned) and `purchase_order`
  (DOE-issued, received) — both linked to engagements and pulling
  line items from `engagement_module` (no schema duplication).
  `invoice_line` table for multi-module invoices with snapshot
  qty + unit_price so post-send scope edits don't rewrite history.
  `engagement_module.qty` (NEW) captures billable units per module.
- **Three deterministic validators at draft→sent.** Catalog/agreed
  unit-price match (catches the PS 498 Leadership-billed-at-
  Instructional-rate error); per-line arithmetic (catches the
  qty × price ≠ subtotal mismatch); period-within-PO-window. Refuses
  the transition with a *fix-shaped* message, not a generic 400.
- **Two new alerts.** `overlapping_invoice_period` — engagement-
  scoped (so multi-engagement schools don't false-positive), covers
  same-date double-billing, period overlap, and outside-PO-window
  in one cast. Solves Jacob-goes-from-memory. `wo_date_outside_
  school_year` catches the 02/15/2025-vs-2026 typo.
- **Three PDF generators.** Work Order in NYC DOE format,
  Sign-in Sheet in LTE table layout (replaces Taylor's Excel
  cleanup dance entirely), Invoice in LTE letterhead (replaces
  Canva). All write to Downloads. All branding parameterised from
  `company_profile` — a sibling consulting firm swaps the row and
  reuses every template.
- **Settings → Company.** Single-row edit form for company_profile.
  Edit once; every WO / sign-in sheet / invoice picks up the new
  values automatically.
- **`propose_program_invoice_draft` action.** Builds multi-line
  invoices from an engagement + period: resolves engagement,
  picks the covering PO, computes remaining billable qty per
  scope item (planned − already billed), formats the date list
  per module from coach_hours, inserts the invoice + invoice_line
  rows. The "draft this month's invoices" handler.
- **`lte_validate_invoice` LLM tool.** Read-only — runs the same
  draft→sent validators against a draft and reports the verdict
  conversationally. Travis can use it before suggesting send.

### Migration

Pack-owned migration `0003_invoicing.sql`. Creates 4 tables,
ALTERs 3 existing (engagement_module, invoice, coach_hours), all
additive with safe defaults. Pre-existing data stays intact.
First-install seeds the `company_profile` row with LTE defaults
(verbatim from the MTAC #R1179 application package); upgrades
keep any existing row via `INSERT OR IGNORE`.

## v0.6.0 — LTE program delivery: the 3 A's, catalog & quotes (2026-05-19)

The Lead to Empower pack modeled only the billing spine (coaches,
hours, signing sheets, invoices) — *money out the door*, with no
representation of what LTE sells or how it delivers it. Digesting the
full NYC DOE MTAC #R1179 application supplied the missing half. This
release encodes it.

### Highlights

- **The "3 A's" state machine.** New `engagement` table — one run per
  school — moving Assessment → Action Planning → Accountable →
  closed, with the signed metrics agreement as the gate into
  delivery. Stage advances from conversation (track-everything;
  Travis proposes the transition, doesn't make you fill a form).
- **The 21-line catalog.** New `catalog_module` table seeded verbatim
  from Appendix F — both pillars (Leadership Development; Data-Driven
  Decision-Making & Teacher Effectiveness), every line with its
  price, session shape, and participant envelope. Plus `assessment`
  (the diagnostic), `engagement_module` (scope of work), and
  `accountability_review` (the ~3/year metrics checkpoints).
- **Quote / margin tool.** `lte_quote_margin` — a read-only LLM tool
  that computes the Appendix G cost model (labor = sessions × hours ×
  instructors × $100/hr, + G&A + materials + rental; margin = list −
  cost) for any module with staffing/price overrides. Answers
  "what's our margin if we run Developing Data-Driven Practices for
  40 kids with one facilitator?" in conversation. Pinned to the
  source numbers by unit tests (Authentic Leadership → $231 / 9.0%).
  New `quote` table persists pre-sale scenarios for bid comparison.
- **Operational alerts for the program side.** Three additions to
  Splash: engagements delivering without a signed metrics agreement,
  active engagements with no accountability review on record (money —
  unreviewed metrics loses renewals), and engagements stuck in
  Assessment with no diagnostic recorded.
- **Billing bridge.** `coach_hours.engagement_id` ties delivered
  hours back to the engagement they served (forward column; typed UI
  wiring in a later slice).

### Notes

- First pack-owned migrations for `lead_to_empower` (the billing
  spine stays in core's `0003_domain.sql` for history continuity).
  Pack version → 0.3.0.
- Specs: `LTE_PACK_SPEC.md`, `LTE_QUOTE_SPEC.md`. Persisted-quote
  stored-computed columns are deferred to a custom quote UI slice
  (documented in the quote spec); the tool is the compute engine
  meanwhile.

## v0.5.1 — Export your data (2026-05-09)

Adds a transparency hatch: a Settings → **Export** section that
dumps every user-table row in the current instance to a JSON file
in the user's Downloads folder. Built for the consented
pre-commercialization observation arrangement — Travis is still a
black box; the export is how an operator inspects what's been
captured.

### Highlights

- **Settings → Export.** Single button writes
  `travis-export-<timestamp>.json` to Downloads (or
  `<timestamp>-full.json` when sensitive workspaces are included).
  Reveal-in-folder affordance for one-click attach-to-email.
- **Privacy posture.** Sensitive workspaces (health/therapy/legal/
  finance) are excluded by default; explicit checkbox to opt in.
  OAuth tokens (`access_token`, `refresh_token`,
  `credentials_json`) always redacted regardless. Embedding blobs
  replaced with byte-length sentinels — file stays inspectable
  without leaking 3 KB vectors per row.
- **Transparency surface.** The result panel shows the file path,
  size, total row count, per-table breakdown (collapsible), and
  any redactions applied — the user sees exactly what's in the
  file before sharing.
- **Backend dynamic.** Walks every user table via `PRAGMA
  table_info`, encodes columns by declared type (int/real/text/
  blob/bool). Filters by workspace when the table has a
  `workspace_id` column. Adds new tables automatically as packs
  ship migrations; no per-table maintenance.

## v0.5.0 — Knowledge graph foundation + Phase 3 token economy (2026-05-09)

Travis stops being a notes app with extras and starts forming
persistent memory. Every named person / place / organisation
mentioned in a journal entry now becomes a typed entity with a
mention timeline, confidence, and embedding. Co-occurrences become
graph edges. The LLM gets entity context injected silently on every
turn — Travis recognises Maria from prior captures without being
asked. Phase 3's token-economy work also rolls in: heuristic
fast-path, intent router, Haiku tier for capture-style turns. Design
in [`KNOWLEDGE_GRAPH.md`](./KNOWLEDGE_GRAPH.md).

### Highlights — Knowledge graph

- **Ambient entity capture.** Migration `0021_graph_extensions.sql`
  extends the existing spine `entity` table with embedding,
  confidence, tags, archive, and pack-table back-reference columns;
  `0022_entity_inference_state.sql` adds the cooldown timestamp for
  refinement prompts. Every journal turn passes through three
  layers: pack-declared kind extraction (coach, school, dept,
  tutor, student) at 0.7 confidence, generic
  `person/place/org:unknown` extraction at 0.5, and pack-typed CRUD
  at 1.0. Names recurring across kinds dedup against the highest-
  confidence existing entity in the workspace.
- **Mention timeline.** Every journal turn appends a `mentioned`
  spine event linking each touched entity to the source
  `journal_entry_id` with a 120-char snippet — a per-entity
  timeline ready to query without joining through journal text.
- **Co-mention edges.** Every unordered pair of entities mentioned
  in the same turn upserts a `mentioned_with` relation with a
  count tracked in `attributes_json`. Slice 5's canonical ordering
  prevents duplicate edges per pair.
- **Entity embedding pipeline.** Background sweeper
  (`graph_indexer::spawn`) keeps `entity.embedding_vector` current —
  every 5 minutes it picks up to 50 stale-or-never-indexed entities
  ordered by mentions desc, embeds via fastembed (already loaded
  for journal indexing), writes back. 7-day staleness threshold.
- **Graph-aware retrieval.** `memory::graph::retrieve` resolves the
  current turn's entity hints to known entities and surfaces a
  GRAPH MEMORY block alongside the existing text-similarity
  RELEVANT MEMORY: 5 most recent events, 3 most recent mention
  snippets, top-2 co-mentioned entities. Cheap by construction —
  indexed lookups with strict per-hit caps.
- **Inference helpers.**
  `recurring_mention_candidates`/`edge_proposals`/`name_conflicts`
  query for ambient `*:unknown` entities ripe for refinement, pairs
  co-mentioned often enough to deserve a labelled edge, and
  same-name conflicts across kinds. `apply_refinement` /
  `accept_edge_proposal` / `merge_entities` commit the user's
  answers — designed to be driven by conversation rather than a
  graph dashboard (per the minimal-surfaces directive).
- **Capture chip.** When extraction matches a pre-existing entity
  (mentions_count > 1), the overlay shows a faint *"→ Maria
  (coach)"* chip below the chat reply. Passive recognition; no
  interaction needed.

### Highlights — Phase 3 token economy

- **Cache hygiene.** Audit confirmed all four system-prompt
  builders (journal, summary, proactive, ask) keep dynamic content
  out of the cached prefix. Anthropic's `cache_control: ephemeral`
  on `system` covers tools transitively (canonical order is
  tools → system → messages). Docstring guard added to
  `build_system_prompt` so future contributors don't sneak in
  per-turn data.
- **Heuristic fast-path.** Pure greetings ("hi", "good morning"),
  acknowledgments ("thanks", "ok"), and direct task completions
  ("done 12", "mark 5 done") now skip the LLM entirely — synthetic
  Extraction flows through the existing persistence pipeline.
- **Intent router.** `classify_intent` runs cheap heuristics
  (question marks, leading question words, capture verbs, length)
  to bucket each turn into Query / Capture / Ambiguous. Captures
  skip the memory::retrieve fastembed call + full table scan;
  questions and ambiguous turns keep the full retrieval.
- **Haiku tier.** Capture-classified turns route to
  `claude-haiku-4-5` instead of `claude-sonnet-4-6` — extraction is
  structural and Haiku handles it well at ~3-4× lower cost. Honours
  the user's explicit `profile.model`; only swaps the implicit
  default. Non-Claude providers fall back to default since they
  don't have a comparable cheap tier wired up.

### Manage redesign

The horizontal tab strip is replaced by a sidebar with grouped
navigation: **Capture** (Ask, Tasks, Threads, Reminders) at the
top, then one group per enabled pack with the pack's display name
as the group header (Lead to Empower → Coaches/Schools/Hours/
Sheets/Invoices; Tutoring → Tutors/Students/Sessions/Reports), with
**Diagnostics** as a trailing collapsible group only visible when
the dev toggle is on. People/Places/Orgs tabs explicitly **not**
shipped per the minimal-surfaces directive — the graph is internal
magic, not a CRUD surface.

### Bug fixes

- **Reminders scheduler** was logging *"no column found for name:
  workspace_id"* every 30 seconds because Phase 2's reminder
  scoping pass missed `due_now()`'s SELECT. Now selects the column
  the `Reminder` struct expects.
- **Knowledge graph tabs reverted** in the same release — they were
  briefly added during slice 12 before the user's *"keep Manage
  minimal"* directive landed. Frontend KnowledgeTab.tsx,
  lib/knowledge.ts, and the `list_entities_by_family` Tauri command
  removed; backend graph helpers stay since they drive the chip
  and prompt-injection surfaces.

### Migrations

- `0021_graph_extensions.sql` — schema_version 20 → 21. Adds
  `embedding_vector`, `embedding_indexed_at`, `confidence`, `tags`,
  `archived_at`, `pack_table_id` columns to `entity`; new indexes on
  archived filter / pack-projection lookup / kind-by-workspace
  listing / mention-timeline ordering / relation traversal.
- `0022_entity_inference_state.sql` — schema_version 21 → 22. Adds
  `last_clarification_at` to `entity` plus an index for the
  refinement-candidate query.

### What this enables

Travis recognises names across captures without being told. Cmd-J
"hours with Maria today" auto-resolves to the existing coach Maria
and pulls her recent mentions, related entities, and last-seen into
the LLM context. Inference helpers are ready to drive
through-conversation refinement — Travis can ask "is Maria the L2E
coach or the personal contact?" and apply the answer when the user
replies in chat. The capture chip is the only visible UI surface;
the rest is silent.

## v0.4.0 — Workspaces (2026-05-08)

Travis now keeps separate worlds separate. A workspace scopes every
operational record — tasks, reminders, journal entries, conversations,
embeddings, and every typed pack table — to the world it belongs in.
Switch workspaces from the header chip; the Manage tabs, splash, and
proactive nudges all re-scope. Sensitive categories (Health, Therapy,
Legal, Finance) stay isolated by default per the asymmetric rule —
they don't bleed into other workspaces' reads, and Travis won't auto-
route captures into them. Design is in
[`WORKSPACES.md`](./WORKSPACES.md).

### Highlights

- **Per-row `workspace_id` scoping.** Migration `0020_workspaces.sql`
  adds the column to every scoped core table (task, reminder,
  journal_entry, conversation, embedding, entity, relation, event,
  summary, email_sent) and the L2E pack's typed tables; the tutoring
  pack adds it via its own per-pack migration. Existing rows backfill
  into the default `Personal` workspace.
- **Active + visible workspace state.** `AppState.workspace` holds
  `{active_id, visible_ids}`, refreshed on switch. Reads expand
  across `visible_ids` (active + cross-visible non-sensitive peers);
  writes stamp `active_id`. Asymmetric isolation: sensitive
  workspaces collapse `visible_ids` to themselves.
- **Workspace switcher.** Header chip shows the active workspace's
  name with a warn-yellow tint + lock icon for sensitive ones. Click
  to switch. The `workspace-changed` event refreshes every subscribed
  view.
- **Settings → Workspaces.** Full CRUD: create, rename, recategorise,
  toggle cross-visibility, archive, unarchive. Sensitive cross-
  visibility toggle includes a warning copy.
- **Auto-close idle conversations.** Daily background tick closes any
  `awaiting_user` conversation whose `updated_at` is 7+ days old, so
  the resume-where-you-left-off surface stays clean.
- **Workspace-aware system prompts.** Journal, summary, ask, and
  proactive nudge prompts include the active workspace's name +
  category. Sensitive workspaces get an extra do-not-bleed line.
- **Workspace-scoped semantic memory.** Embeddings denormalise
  `workspace_id` at insert time; retrieval scans only rows in the
  visible set. Cross-workspace recall happens silently when active
  is non-sensitive; sensitive contexts only see themselves.
- **Intelligent LLM routing.** The journal extraction tool gains a
  `workspaceRouting` field. High/medium-confidence picks for non-
  sensitive targets restamp the journal entry, conversation,
  embeddings, tasks, and reminders into the routed workspace. Low-
  confidence and sensitive targets demote to a clarifying question.
  The overlay shows a "Captured to <name>" chip when routing
  diverges from the active workspace.
- **Onboarding workspace step.** New step between the pack picker
  and the done screen lets the user add a Work / Personal / Other
  workspace inline. Sensitive categories deferred to Settings — they
  deserve a deliberate add.

### Migrations

- `0020_workspaces.sql` — schema_version 19 → 20. Creates the
  `workspace` table, the default `Personal` row, the
  `meta.active_workspace_id` pointer, and `ALTER TABLE ADD COLUMN
  workspace_id INTEGER NOT NULL DEFAULT 1` on every scoped core
  table + the L2E pack's typed tables. Indexed for filter speed.
- Tutoring pack `0002_workspace_id.sql` — adds `workspace_id` to
  tutor / student / session / progress_report.

### Known v1 cuts

- 3-capture suggestion to switch the active workspace (deferred —
  routing works per-capture, switching stays manual).
- Per-entity remembered disambiguation (e.g. "Maria → always
  Personal") deferred — routing decides fresh each turn.
- Persistent sensitive-workspace banner across the whole app —
  switcher chip is the only indicator for now.

## v0.3.0 — Plugin platform + runtime pack selection (2026-05-08)

Packs become a real plugin format. Every primary table from every
enabled pack now renders as a Manage tab — list, detail, edit, delete
— with **zero pack-side UI code**. Pack authors ship schema metadata;
core materialises the UI dynamically. Custom React components are
optional, ship inside the pack, and override the auto-CRUD when the
UX warrants it. The pack-authoring guide is at
[`AUTHORING_PACKS.md`](./AUTHORING_PACKS.md).

### Highlights

- **Schema-driven auto-CRUD.** New `PackHandle::tables()` declares
  every typed table with rich field metadata (`FieldType` covers
  Text, LongText, Email, Phone, Integer, Number, Currency, Date,
  DateTime, Bool, Enum, Ref, Json, Timestamp). Generic Tauri commands
  (`pack_table_list / _get / _upsert / _delete`) build SQL from the
  metadata; SQL-injection-safe by construction.
- **Auto list / detail / form views.** Frontend `src/lib/autoCRUD/`
  contains type-aware components (`ListView`, `DetailView`,
  `FormView`, `FieldCell`, `FieldInput`) that render any pack's
  table. Sortable columns, click-to-detail, edit forms, two-click
  delete.
- **Custom UI overrides.** Pack-shipped React components live at
  `src/packs/<slug>/ui/` and register in `src/lib/packRegistry.ts`.
  The L2E `InvoicesTab` moves into the L2E pack and demonstrates
  the override path.
- **Operational alerts.** New `PackHandle::alerts()` returns
  `AlertDef` entries with severity (money / action / info) and SQL
  for the headline metric. The Splash screen renders these
  prominently above the entity stats. L2E ships *Hours not yet
  invoiced* + *Signing sheets awaiting signature*; tutoring ships
  *Progress reports drafted but not sent* + *No-show sessions to
  follow up*.
- **Runtime pack selection.** `meta.pack.<slug>.enabled` per DB
  decides which compiled-in packs participate. Onboarding step 8
  asks "What should Travis help with?"; Settings → Packs lets users
  toggle anytime. Cargo features stay as a build-time lever for
  distros (`--no-default-features --features pack-tutoring`).
- **Tutoring pack.** The second vertical pack ships in the default
  build, runtime-disabled by default. Validates that the abstraction
  isn't accidentally L2E-shaped — writing the second pack felt
  mechanical: declare schema, ship migrations, register entity
  kinds. No UI code, no Tauri commands.

### What this enables

A new vertical pack now needs only: `tables.rs` schema declarations,
a SQL migration, an entity-kinds list, a prompt fragment, an alert
or two. Roughly half a day from the right MARKET.md vertical to a
working pack with full UI. Custom UI is opt-in for places the
auto-CRUD shape doesn't suit.

### Migrations

No new core migrations in v0.3.0; pack metadata lives in compile-
time `&'static` data. The tutoring pack's `0001_init.sql` runs as
a per-pack migration, tracked in `meta.pack.tutoring.schema_version`.

### Breaking changes

None for end users on the default build. The L2E pack's invoice tab
is now sourced from `src/packs/lead_to_empower/ui/InvoicesTab.tsx`
instead of `src/manage/tabs/InvoicesTab.tsx` (path change only;
identical behaviour).

### Internal docs

- `PLUGIN_PLATFORM.md` — design spec; slices 1–7 shipped, slice 8
  (onboarding hooks) deferred per `DEFERRED.md`.
- `AUTHORING_PACKS.md` — comprehensive guide to building and
  evolving packs.

---

## v0.2.0 — Pack architecture (2026-05-07)

Travis is now generic at the data-model level. The vertical-specific
code that used to be baked into core (after-school enrichment program
ops — coaches, schools, signing sheets, NYC DoF invoicing) lives
entirely as an installable pack under
`src-tauri/src/packs/lead_to_empower/`. Future verticals — tutoring,
home care, therapy, field service — ship as their own packs and plug
into the same extension points.

### Highlights

- **Pack architecture.** New `PackHandle` trait declares a pack's
  slug, version, migrations, prompt fragment, entity kinds, action
  kinds, and registration hooks for tools and action handlers. Packs
  gate compilation behind a Cargo feature flag
  (`pack-lead-to-empower` is default-on).
- **Universal spine.** Three new core tables — `entity`, `relation`,
  `event` — give every domain object a place in a cross-pack
  rendezvous index. Packs sync their typed data into the spine via
  explicit writes, so retrieval and the future knowledge graph see
  one unified view.
- **Action + tool registries.** Action dispatch and tool registration
  are runtime registries that packs extend at startup. The static
  `actions::dispatch` match and the hardcoded
  `tools::read_only_registry` are gone.
- **Dynamic journal extraction.** The schema for the journal
  extraction LLM tool derives entity buckets and the
  `proposedActions` enum from the live pack registry. Adding a pack
  with new entity kinds doesn't require touching `journal.rs`.
- **Pack-supplied prompt fragments.** Each pack contributes a
  system-prompt fragment that's appended to journal, proactive,
  summary, and ask-Travis prompts.
- **Frontend pack gating.** The Manage tab list hides pack-supplied
  UI (the L2E Invoices tab) when the corresponding pack is disabled.
  `appStatus.enabledPacks` exposes the pack list.
- **`task` graduates to core.** L2E-specific `link_kind` CHECK
  constraint dropped; new `entity_id` column links to the spine.

### Migrations

- `0018_pack_spine.sql` — generalises `entity_index` → `entity`,
  adds `relation` and `event`.
- `0019_task_to_core.sql` — recreates `task` without the L2E CHECK
  constraint, adds `entity_id`.

Both run cleanly on a fresh DB. Existing dev installs that predate
the LF-pinning of migration files in `.gitattributes` may hit a
sqlx checksum drift on first launch (delete the data dir to reset).

### What this enables

Building the next vertical pack — tutoring, home care, therapy, MSP,
legal — is now scoped to creating a `src-tauri/src/packs/<slug>/`
directory with a `PackHandle` impl plus typed tables and Tauri
commands. No core changes needed. `MARKET.md` lists the 20 target
verticals; `PACKS.md` is the format spec.

### Breaking changes

None for end users on the default build. The L2E pack ships enabled
by default and behaves identically to v0.1.3.

### Internal docs

- `PACKS.md` — pack format spec.
- `PACKS_AUDIT.md` — full refactor record (12 steps).
- `ROADMAP.md` — Phase 1 marked shipped; remaining bullets cover the
  second-pack validation milestone.

---

## v0.1.3 — Dark window chrome + granular proactive schedule (2026-04-28)

See [GitHub release](https://github.com/myketheguru/travis-releases/releases/tag/v0.1.3).

## v0.1.2 — macOS visual fixes

## v0.1.1 — Startup hardening, voice dropdown, onboarding overflow fix

## v0.1.0 — Initial release
