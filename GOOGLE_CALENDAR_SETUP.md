# Google Calendar Setup

Travis can connect to Google Calendar so you can ask questions like *"what's
on my calendar this week?"*. The connection is per-user and uses Google's
OAuth — Travis never sees your Google password.

There are two parts:

1. **Build-time** (you, the developer, do this once): create a Google Cloud
   project + OAuth client, paste the credentials into the build env vars.
2. **Runtime** (the end user does this per machine): click Connect in
   Settings, sign in to Google, grant access.

## Part 1 — Build-time (developer, once)

### 1.1 Create a Google Cloud project

1. Go to https://console.cloud.google.com/
2. **New project** → name it `Travis` (or whatever you want).
3. Wait for it to provision.

### 1.2 Enable the Google Calendar API

1. Sidebar → **APIs & Services → Library**
2. Search **Google Calendar API** → **Enable**.

### 1.3 Configure the OAuth consent screen

1. Sidebar → **APIs & Services → OAuth consent screen**
2. **User Type**: pick **External** (you'll be the only user during testing,
   but External is what allows real Gmail accounts to sign in)
3. **Create**.
4. Fill in:
   - **App name**: `Travis`
   - **User support email**: your address
   - **Developer contact information**: your address
5. **Save and continue**.
6. **Scopes**: Add → search for and select:
   - `.../auth/calendar.events.readonly` (read events)
   - `.../auth/userinfo.email` (so Travis can show "connected as ..." in Settings)
   - `openid`
7. **Save and continue**.
8. **Test users**: add your own Gmail account (and any other Travis users).
   While the app is in "Testing" status, only listed test users can sign in.
9. **Save and continue** → **Back to dashboard**.

> When you're ready to ship to more users, switch the consent screen status
> from **Testing** to **In production**. Google may ask for verification if
> any of your scopes are "sensitive" — `calendar.events.readonly` is one of
> these. Verification takes a few weeks but is free. Until then you can keep
> adding users to the test-users list (up to 100).

### 1.4 Create the OAuth client

1. Sidebar → **APIs & Services → Credentials**
2. **+ Create credentials → OAuth client ID**
3. **Application type**: **Desktop app**
4. **Name**: `Travis Desktop`
5. **Create**.
6. Copy the **Client ID** and **Client secret**.

### 1.5 Paste into your build env

In `src-tauri/.env` (gitignored):

```
TRAVIS_GOOGLE_CLIENT_ID=<the-client-id-you-copied>.apps.googleusercontent.com
TRAVIS_GOOGLE_CLIENT_SECRET=<the-client-secret-you-copied>
```

(`build.rs` reads `.env` and forwards the values to rustc as compile-time
constants. Same pattern as `TRAVIS_TELEMETRY_*`.)

Rebuild:

```
npm run tauri dev
```

Settings → Calendar should now show **Connect Google Calendar** as enabled.

## Part 2 — Runtime (end user, per machine)

1. Open Travis → Settings (gear icon top-right) → **Calendar**.
2. Click **Connect Google Calendar**.
3. Travis opens your default browser to Google's sign-in page on a
   `127.0.0.1:<random-port>` redirect.
4. Sign in (or pick an account), then click **Allow** to grant Travis
   read-only access to your calendar events.
5. Browser shows *"You can close this tab"*. Return to Travis.
6. Settings now shows **Connected as `<your-email>`**.

To disconnect, click **disconnect** next to your email. Travis deletes the
local refresh token and forgets the connection. To revoke entirely, also visit
https://myaccount.google.com/permissions and remove the Travis app.

## What Travis sees

Only **read-only access to calendar events**. It cannot:
- Create or edit events (planned for a future tool with explicit Confirm cards)
- Read your email, contacts, or any other Google data
- Read calendars you don't own (unless you've shared them in Google Calendar
  itself)

The OAuth refresh token is stored in your OS keychain (Windows Credential
Manager / macOS Keychain / Linux Secret Service) — same place Travis stores
your Claude/OpenAI API key. The short-lived access token is in the local
SQLite DB and gets refreshed automatically when it expires.

## Troubleshooting

**"Calendar isn't set up in this build"** in Settings — the build doesn't
have credentials baked in (skipped Part 1.5). Set the env vars in `.env`,
rebuild, retry.

**"Google didn't return a refresh token"** — happens if you've connected this
client before. Visit https://myaccount.google.com/permissions, find Travis,
click **Remove access**, then try Connect again. (Or delete and re-create the
OAuth client in Cloud Console to be sure.)

**Browser opens but redirect never returns** — make sure no firewall is
blocking `127.0.0.1` ↔ Travis. The redirect URI is `http://127.0.0.1:<port>/callback`
and only opens for the duration of the OAuth flow.

**"OAuth state mismatch"** — possible CSRF (someone else hit your local
listener). Try Connect again from Settings.
