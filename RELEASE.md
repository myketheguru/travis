# Shipping Travis

End-to-end walkthrough: get the project on GitHub, generate the updater
signing key, set the GitHub Actions secrets, and ship the first signed
release that running installs can auto-update from.

Replace `leadtoempower/travis` with whatever GitHub org/repo you actually
use.

---

## 1. Initial git setup (local)

From the project root:

```sh
git init
git add .
git status                  # sanity-check that .env, target/, *.key are NOT staged
git commit -m "Initial commit"
git branch -M main
```

If anything sensitive shows up in `git status` (e.g. `src-tauri/.env` or
a `*.key` file), `git restore --staged <file>` and add it to `.gitignore`
before committing.

---

## 2. Create the GitHub repo

Use the GitHub CLI (recommended; needs `gh auth login` first):

```sh
gh repo create leadtoempower/travis --private --source=. --remote=origin --push
```

Or do it through the web UI:

1. github.com → New repo → name `travis` → Private → don't initialize.
2. Copy the remote URL it shows you, then locally:

```sh
git remote add origin git@github.com:leadtoempower/travis.git
git push -u origin main
```

Pushing will trigger the **CI** workflow (`.github/workflows/ci.yml`),
which runs `tsc --noEmit` + `cargo check`. Watch it succeed in the
Actions tab before moving on.

---

## 3. Generate the Tauri updater signing key

This Ed25519 keypair is what proves "this update came from us" to every
running installation. The private key only ever lives on your machine
plus inside the GitHub Actions secret store. **Losing it permanently
disables auto-updates for everyone already running the app.**

```sh
mkdir -p ~/.tauri
npx tauri signer generate -w ~/.tauri/travis.key
```

It will:

1. Prompt you for a password — pick a strong one and write it down (you'll
   paste it as a GitHub secret in step 5).
2. Print the **public key** to stdout.
3. Write the password-protected **private key** to `~/.tauri/travis.key`.

### Paste the public key into the app

Open `src-tauri/tauri.conf.json` and replace
`REPLACE_ME_WITH_OUTPUT_OF_tauri_signer_generate` with the public key
that was printed:

```json
"plugins": {
  "updater": {
    ...
    "pubkey": "dW50cn...<paste>"
  }
}
```

Commit:

```sh
git add src-tauri/tauri.conf.json
git commit -m "Wire updater public key"
git push
```

### Back up the private key

The private key file (`~/.tauri/travis.key`) is unrecoverable if you lose
it. Copy it to:

- A password manager (1Password / Bitwarden have file attachment fields), or
- An encrypted drive / encrypted backup, or
- A second machine you control.

Don't email it. Don't put it in the repo. Don't paste it into Slack.

---

## 4. Register OAuth apps (one-time per provider)

### Google (Calendar read + Gmail send)

1. Google Cloud Console → APIs & Services → Credentials → Create credentials → OAuth client ID.
2. Application type: **Desktop app**.
3. Save the `client_id` + `client_secret` shown.
4. APIs & Services → OAuth consent screen → publish (or add yourself as a test user while in dev).
5. Enable APIs: Gmail API + Google Calendar API.

### Microsoft (Outlook calendar + send mail)

1. Azure Portal → App registrations → New registration.
2. Supported account types: **Accounts in any organizational directory and personal Microsoft accounts**.
3. Redirect URI: pick **Mobile and desktop applications** as the platform, then add the literal value `http://localhost`. (Azure ignores the port for localhost — the app picks a random free port at runtime.)
4. After registering: API permissions → Add a permission → Microsoft Graph → Delegated → `Calendars.Read`, `Mail.Send`, `User.Read`, `offline_access`. Click "Grant admin consent" if you can; otherwise users will be prompted on first connect.
5. Certificates & secrets → New client secret → save the **Value** shown (you won't see it again).
6. Note the **Application (client) ID** from the Overview tab.

---

## 5. Set GitHub Actions secrets

Repo → Settings → Secrets and variables → Actions → "New repository
secret". Add each of:

| Name | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | the entire contents of `~/.tauri/travis.key` (use `cat` and paste) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password you chose at keygen time |
| `TRAVIS_GOOGLE_CLIENT_ID` | from Google Cloud Console |
| `TRAVIS_GOOGLE_CLIENT_SECRET` | from Google Cloud Console |
| `TRAVIS_MICROSOFT_CLIENT_ID` | Application (client) ID from Azure |
| `TRAVIS_MICROSOFT_CLIENT_SECRET` | client secret Value from Azure |
| `TRAVIS_TELEMETRY_URL` | your Cloud Function URL, if you're running one |
| `TRAVIS_TELEMETRY_TOKEN` | the bearer token your function expects |

For local dev the same values can also live in `src-tauri/.env` (which
is gitignored — `build.rs` reads it and forwards each KEY=VALUE to rustc
as a compile-time env var). The CI uses the secrets directly, no `.env`
involved.

---

## 6. Ship the first release

```sh
# Bump versions in lockstep — they MUST match.
# Edit:
#   src-tauri/tauri.conf.json   ("version": "0.2.0")
#   src-tauri/Cargo.toml        (version = "0.2.0")

git add src-tauri/tauri.conf.json src-tauri/Cargo.toml
git commit -m "Release 0.2.0"
git tag v0.2.0
git push origin main
git push origin v0.2.0
```

The tag push triggers `.github/workflows/release.yml`. It will:

1. Build on Windows, macOS (universal), and Linux in parallel (≈ 15–25 min).
2. Sign each installer with the Ed25519 key, producing a `.sig` sidecar.
3. Generate `latest.json` (the updater manifest).
4. Create a **draft** release tagged `v0.2.0` with all the artifacts attached.

When all three jobs go green:

1. Releases tab → edit the draft.
2. Edit the release notes (or paste a changelog).
3. Click **Publish release**.

The moment you publish, every running install on the next "Check for
updates" tap (Settings → Updates) will see the new version, verify the
signature, download, and install.

---

## 7. Confirm the loop works

On a clean test machine (or a VM):

1. Install the **previous** version of Travis.
2. Open Settings → Updates. Click "Check for updates".
3. Confirm the install button flips to "Install v0.2.0", click it.
4. The app downloads, verifies, applies, and restarts.

If the updater errors out with "signature mismatch" or similar: the
`pubkey` in `tauri.conf.json` of the *running* install doesn't match the
key the new release was signed with. That mismatch can only be fixed by
shipping a new install of the right pubkey manually — which is why
**don't lose the private key**.

---

## Quick reference

```sh
# Local: bump + tag + push
git tag v0.2.0 && git push origin v0.2.0

# Local: build a one-off signed installer without going through CI
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/travis.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="<your password>"
npm run tauri build

# Local: build a updater manifest from the artifacts of `tauri build`
npm run release:manifest -- \
  --release-url https://github.com/leadtoempower/travis/releases/download/v0.2.0 \
  --notes "Release notes here"
```
