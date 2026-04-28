# Firebase Telemetry Setup

This walks you through standing up the Firebase backend that receives Travis's metadata events and the build-time wiring on the Travis side.

**Time:** ~25 minutes the first time.
**Cost:** free for any realistic usage. Firebase Spark plan covers Firestore and limited Cloud Functions; Blaze plan unlocks Functions v2 with a generous free tier.
**Privacy posture:** Travis only ever sends metadata (event kind, counts, intent, provider name, etc.) — never raw note text or user identifiers. Audit `telemetry::emit` call sites in the Rust source to confirm.

---

## 1. Create the Firebase project

1. Go to https://console.firebase.google.com
2. **Add project** → name it `Travis Telemetry` (or whatever)
3. Optional: skip Google Analytics — we don't need it
4. Wait for provisioning (~30s)

## 2. Enable Firestore

1. Sidebar → **Build → Firestore Database**
2. **Create database**
3. **Start in production mode** (we'll write only via the function, never from clients, so leave the default deny rules)
4. Region: pick one close to you (`us-central1` is the default)
5. Done

## 3. Upgrade to Blaze plan (optional but recommended)

Cloud Functions v2 (gen-2) requires the Blaze (pay-as-you-go) plan. The free tier inside Blaze is generous — for our usage you'll almost certainly stay free.

1. Sidebar → ⚙ next to project name → **Usage and billing → Modify plan**
2. **Blaze (Pay as you go)** → link a billing account
3. Optional: set a budget alert at $5/month so you get a warning if anything weird happens

(If you'd rather stay on Spark, you can use Functions v1 — the function code below works for both; just remove the `v2/https` import lines and use the `functions.https.onRequest` syntax.)

## 4. Install the Firebase CLI locally

```bash
npm i -g firebase-tools
firebase login
```

The login opens a browser, you authorize, done.

## 5. Initialize a Functions project

In a directory **outside** the Travis repo (this is its own thing):

```bash
mkdir travis-telemetry-fn
cd travis-telemetry-fn
firebase init functions
```

Pick:
- **Use an existing project** → select your "Travis Telemetry" project
- Language: **JavaScript**
- ESLint: **No** (your call)
- Install dependencies now: **Yes**

You'll get a `functions/` directory with `index.js` and a `package.json`.

## 6. Set the ingest secret

Generate a strong random token. Anything 32+ chars works:

```bash
openssl rand -base64 32
# or on Windows PowerShell:
# [Convert]::ToBase64String((1..32 | ForEach-Object { Get-Random -Maximum 256 }))
```

**Save the output somewhere — you'll paste it twice (into Firebase secrets and into Travis env vars).**

Then store it as a Functions secret:

```bash
firebase functions:secrets:set TRAVIS_INGEST_TOKEN
```

It'll prompt you to paste the value.

## 7. Drop in the function code

Replace `functions/index.js` with:

```js
const { onRequest } = require("firebase-functions/v2/https");
const { defineSecret } = require("firebase-functions/params");
const { initializeApp } = require("firebase-admin/app");
const { getFirestore, FieldValue } = require("firebase-admin/firestore");

initializeApp();
const TRAVIS_INGEST_TOKEN = defineSecret("TRAVIS_INGEST_TOKEN");

exports.travisIngest = onRequest(
  { secrets: [TRAVIS_INGEST_TOKEN], cors: false, region: "us-central1" },
  async (req, res) => {
    if (req.method !== "POST") {
      res.set("Allow", "POST");
      return res.status(405).send("POST only");
    }
    const auth = req.get("authorization") || "";
    if (auth !== `Bearer ${TRAVIS_INGEST_TOKEN.value()}`) {
      return res.status(401).send("unauthorized");
    }
    const { source, events } = req.body || {};
    if (!Array.isArray(events) || events.length === 0) {
      return res.status(400).send("events array required");
    }
    const db = getFirestore();
    const col = db.collection("travis_events");
    const batch = db.batch();
    for (const e of events) {
      batch.set(col.doc(), {
        source: source ?? "unknown",
        kind: e.kind ?? "unknown",
        ts: e.ts ?? null,
        payload: e.payload ?? {},
        receivedAt: FieldValue.serverTimestamp(),
      });
    }
    await batch.commit();
    return res.status(200).json({ ok: true, count: events.length });
  }
);
```

## 8. Deploy

```bash
firebase deploy --only functions
```

After 1–3 minutes you'll see something like:

```
✔  functions[travisIngest(us-central1)]: Successful create operation.
Function URL (travisIngest(us-central1)):
https://us-central1-travis-telemetry.cloudfunctions.net/travisIngest
```

**Save that URL.**

## 9. Bake the URL + token into Travis at build time

Travis reads `TRAVIS_TELEMETRY_URL` and `TRAVIS_TELEMETRY_TOKEN` via Rust's `option_env!` — they're embedded into the binary at compile time. The user never sees or configures them. If either is unset at build time, telemetry silently no-ops.

**On Windows (PowerShell), in the Travis repo:**

```powershell
$env:TRAVIS_TELEMETRY_URL   = "https://us-central1-travis-telemetry.cloudfunctions.net/travisIngest"
$env:TRAVIS_TELEMETRY_TOKEN = "the-token-you-saved-in-step-6"
npm run tauri dev
```

**On macOS / Linux:**

```bash
export TRAVIS_TELEMETRY_URL="https://us-central1-travis-telemetry.cloudfunctions.net/travisIngest"
export TRAVIS_TELEMETRY_TOKEN="the-token-you-saved-in-step-6"
npm run tauri dev
```

For production builds, set these in your CI environment / GitHub Actions secrets and they'll be baked into the released binaries.

> Note: anyone with the binary can extract the token by `strings`-ing it. For an internal-only ops tool that's fine. If you ever ship this externally, rotate the token and consider a lightweight verification scheme (signed payloads, time-bounded tokens, etc.).

## 10. Verify

1. Run Travis: `npm run tauri dev` (with the env vars set)
2. Check the dev terminal — you should NOT see `telemetry: no compile-time TRAVIS_TELEMETRY_URL — sender disabled`. Instead you'll see (within ~75s of launch) `telemetry: sent N events`.
3. Open Firebase Console → Firestore → `travis_events` collection. You should see documents arriving with fields like:
   ```
   source: "travis"
   kind: "app_start"
   ts: "2026-04-27T..."
   payload: { version: "0.1.0", platform: "windows" }
   receivedAt: <server timestamp>
   ```
4. Trigger a journal entry in Travis. Within 60s a `journal_ingested` event should appear.

If nothing arrives:
- Check the Cloud Function logs in Firebase Console (Functions → Logs)
- Confirm the URL exactly matches (trailing slash matters)
- Confirm the token matches (no extra whitespace from copy-paste)

## 11. Build a dashboard (later)

For now, browsing Firestore directly is fine. When you want a real dashboard:

- **Quick:** Firebase Console has decent collection browsing + filtering
- **Better:** wire Firestore to Looker Studio for free chart dashboards (Looker has a Firestore connector)
- **Custom:** small static site on Firebase Hosting that queries Firestore via the Web SDK with read-only API key + Firestore security rules limiting it to a known IP / your auth domain

## 12. What events Travis emits today

| kind | when | payload (representative) |
|-|-|-|
| `app_start` | every launch | `{version, platform}` |
| `journal_ingested` | each Cmd+J → Enter | `{intent, ok, created, completed, questions, gaps, provider}` |

Adding more is one line at a Rust call site:

```rust
telemetry::emit(&pool, "task_state_changed", json!({
    "from": old_status,
    "to": new_status,
})).await;
```

I'll wire more emitters as we add features. The Firestore schema is freeform under `payload`, so new event kinds work without schema changes on either side.

---

## Adding fields to events (or new event kinds)

The wire format is fixed — `{kind, ts, payload}` — but `payload` is free-form JSON. New event kinds and new payload fields just appear in Firestore. No migration needed on either side.

## Disabling telemetry without redeploying

Two ways:
- **Build with no env vars** — `option_env!` returns `None`, sender doesn't spawn.
- **Block at the function** — temporarily reject in `index.js` and redeploy, e.g. `return res.status(503).send("paused")`. Events queue locally on the client and retry up to 5 times before stopping.

## Costs to watch

The big knob is event volume. As of this writing, you're emitting maybe 5-50 events/day in normal use, well within Firebase free tier:
- Firestore: 50K writes/day free
- Cloud Functions: 2M invocations/month free
- Egress: 5GB/month free

If you ever blow past these you'll get a budget alert (if you set one in step 3). Sensible payload limits in the function (`events.length` or per-event size) would be a good safety valve to add when usage scales.
