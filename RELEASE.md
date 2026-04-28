# Shipping Travis (two-repo setup)

Two GitHub repos are involved:

- **`myketheguru/travis`** — private. Source code, build config, secrets.
  All development and tag pushes happen here.
- **`myketheguru/travis-releases`** — public. Holds nothing but the built
  installers and the updater manifest (`latest.json`). End users download
  from here. The auto-updater fetches `latest.json` from here.

This split lets the source stay private while keeping installers and the
updater feed publicly fetchable (anonymous downloads, no GitHub auth).

Replace `myketheguru/...` below with whatever org/user you actually use.

---

## 1. Initial git setup (local, in this directory)

```sh
git init
git add .
git status                  # double-check that .env, target/, *.key are NOT staged
git commit -m "Initial commit"
git branch -M main
```

---

## 2. Create both GitHub repos

```sh
# Private source repo
gh repo create myketheguru/travis --private --source=. --remote=origin --push

# Public releases-only repo (initially empty; the workflow populates it)
gh repo create myketheguru/travis-releases --public --description "Public release artifacts for Travis"
```

Or via the web UI: create both manually, then `git remote add origin … && git push -u origin main` for the source repo.

The first push triggers `.github/workflows/ci.yml`. Watch it succeed in
the source repo's Actions tab before continuing.

---

## 3. Generate the Tauri updater signing key

```sh
mkdir -p ~/.tauri
npx tauri signer generate -w ~/.tauri/travis.key
```

It will:
1. Prompt you for a password — pick a strong one. **Write it down.**
2. Print the **public key** to stdout.
3. Write the password-protected **private key** to `~/.tauri/travis.key`.

### Paste the public key into the app

Open `src-tauri/tauri.conf.json` and replace
`REPLACE_ME_WITH_OUTPUT_OF_tauri_signer_generate` with the public key
that was printed. Commit + push.

```sh
git add src-tauri/tauri.conf.json
git commit -m "Wire updater public key"
git push
```

### Back up the private key

`~/.tauri/travis.key` is unrecoverable if you lose it. Losing it means
existing installs can never auto-update again — they'd have to be
manually reinstalled with a binary signed by a new keypair. Copy it to:

- A password manager file attachment (1Password / Bitwarden), or
- An encrypted backup volume, or
- A second machine you control.

Don't email it. Don't paste it into chat. Don't put it in either repo.

---

## 4. Create a fine-grained PAT for cross-repo publishing

The release workflow runs in the **private** source repo but needs to
upload assets to the **public** releases repo. GitHub's automatic
`GITHUB_TOKEN` only grants access to the current repo, so we use a
fine-grained PAT scoped narrowly to the releases repo.

1. Go to https://github.com/settings/personal-access-tokens/new
2. **Token name**: `travis-releases-publisher`
3. **Resource owner**: your account (`myketheguru`)
4. **Expiration**: 1 year (set a calendar reminder; rotating PATs is healthy)
5. **Repository access** → **Only select repositories** → pick `travis-releases`
6. **Permissions** → **Repository permissions** → **Contents** → **Read and write**
7. Click **Generate token**, copy the value (it's shown once).

---

## 5. Register OAuth apps (one-time per provider)

### Google (Calendar read + Gmail send)

1. Google Cloud Console → APIs & Services → Credentials → Create credentials → OAuth client ID.
2. Application type: **Desktop app**.
3. Save the `client_id` + `client_secret`.
4. APIs & Services → OAuth consent screen → publish (or add yourself as a test user during dev).
5. Enable APIs: Gmail API + Google Calendar API.

### Microsoft (Outlook calendar + send mail)

1. Azure Portal → App registrations → New registration.
2. Supported account types: **Accounts in any organizational directory and personal Microsoft accounts**.
3. Redirect URI: pick **Mobile and desktop applications** as the platform, then add the literal value `http://localhost`. (Azure ignores the port for localhost — the app picks a random free port at runtime.)
4. After registering: **API permissions** → Add a permission → Microsoft Graph → Delegated → `Calendars.Read`, `Mail.Send`, `User.Read`, `offline_access`. Click "Grant admin consent" if you can.
5. **Certificates & secrets** → New client secret → save the **Value** shown immediately (you won't see it again).
6. Note the **Application (client) ID** from the Overview tab.

---

## 6. Set GitHub Actions secrets

In the **private** `travis` repo: Settings → Secrets and variables →
Actions → "New repository secret". Add each of:

| Name | Value |
|---|---|
| `RELEASES_REPO_PAT` | the PAT from step 4 |
| `TAURI_SIGNING_PRIVATE_KEY` | full contents of `~/.tauri/travis.key` (use `cat ~/.tauri/travis.key` and paste) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password you chose at keygen time |
| `TRAVIS_GOOGLE_CLIENT_ID` | from Google Cloud Console |
| `TRAVIS_GOOGLE_CLIENT_SECRET` | from Google Cloud Console |
| `TRAVIS_MICROSOFT_CLIENT_ID` | Application (client) ID from Azure |
| `TRAVIS_MICROSOFT_CLIENT_SECRET` | client secret Value from Azure |
| `TRAVIS_TELEMETRY_URL` | your Cloud Function URL (skip if you're not running telemetry) |
| `TRAVIS_TELEMETRY_TOKEN` | the bearer token your function expects |

Locally the same values can also live in `src-tauri/.env` (gitignored —
`build.rs` reads it and forwards each KEY=VALUE to rustc as a
compile-time env var). The CI uses the secrets directly, no `.env`
involved.

---

## 7. Ship the first release

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

The tag push triggers `.github/workflows/release.yml`, which runs in two
stages:

1. **build-and-stage** (Win + Mac + Linux in parallel, 15–25 min): each
   platform builds, signs, and attaches its installers to a draft release
   in **this private repo** under tag `v0.2.0`. tauri-action regenerates
   `latest.json` after each upload, so when all three jobs finish the
   draft has every platform represented.
2. **promote-public** (after the matrix completes, ~1 min): downloads
   every staged asset and creates a draft release in
   `travis-releases` with the same tag.

When all jobs go green:

1. Go to **`myketheguru/travis-releases`** → Releases → edit the draft.
2. Add real release notes / changelog.
3. Click **Publish release**.

The moment you publish, every running install on the next "Check for
updates" tap (Settings → Updates) will see the new version, verify the
signature against the embedded public key, download, install, and restart.

The **staging** draft in the private repo can be left alone (it's an
audit trail) or deleted to save storage — it has no functional role
once promote-public has run.

---

## 8. Confirm the loop works

On a clean test machine (or VM):

1. Install the **previous** version of Travis.
2. Open Settings → Updates. Click "Check for updates".
3. Confirm the install button flips to "Install v0.2.0", click it.
4. The app downloads, verifies, applies, and restarts on the new version.

If the updater errors with "signature mismatch" or similar: the `pubkey`
in `tauri.conf.json` of the *running* install doesn't match the key the
new release was signed with. The mismatch can only be fixed by shipping
a new install of the right pubkey manually — which is why the private
key backup matters.

---

## 9. Enable the landing page (one-time)

The landing page lives in `landing/` in this private source repo. The
`.github/workflows/deploy-site.yml` workflow pushes it to the
`gh-pages` branch of `travis-releases` on every push to `main` that
touches `landing/`.

After the **first** successful run of that workflow:

1. The public `travis-releases` repo will have a new `gh-pages` branch.
2. Go to `travis-releases` → **Settings** → **Pages**.
3. **Source**: "Deploy from a branch".
4. **Branch**: `gh-pages`, folder `/ (root)`. Click **Save**.
5. After ~1 minute, the public URL appears at the top of that page —
   typically `https://myketheguru.github.io/travis-releases/`.

The page reads the latest release from `travis-releases` via the GitHub
API at runtime, so once a release is published its installers light up
on the page automatically — no need to redeploy the page per release.

To trigger an initial deploy without editing the page, run the workflow
manually:

```sh
gh workflow run deploy-site.yml
```

---

## Quick reference

```sh
# Bump + tag + push (kicks off CI)
git tag v0.2.0 && git push origin v0.2.0

# Build a one-off signed installer locally (no CI)
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/travis.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="<your password>"
npm run tauri build

# Build a hand-rolled latest.json from local artifacts
npm run release:manifest -- \
  --release-url https://github.com/myketheguru/travis-releases/releases/download/v0.2.0 \
  --notes "Release notes here"
```
