use std::fs;
use std::path::Path;

fn main() {
    // Cargo doesn't track env vars used by `option_env!` by default, so spell
    // out which ones invalidate the build when they change.
    println!("cargo:rerun-if-env-changed=TRAVIS_TELEMETRY_URL");
    println!("cargo:rerun-if-env-changed=TRAVIS_TELEMETRY_TOKEN");
    println!("cargo:rerun-if-env-changed=TRAVIS_GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=TRAVIS_GOOGLE_CLIENT_SECRET");
    println!("cargo:rerun-if-env-changed=TRAVIS_MICROSOFT_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=TRAVIS_MICROSOFT_CLIENT_SECRET");
    // v0.20.2 — Travis Cloud anthropic key, baked at build time so the
    // shipped binary defaults to Travis Cloud for new users without
    // any onboarding LLM-setup step. The CI workflow writes this from
    // a secret; local dev builds without it surface the cloud option
    // as "(this build wasn't compiled with a cloud key)" so devs know
    // to fall back to their own key.
    println!("cargo:rerun-if-env-changed=TRAVIS_CLOUD_ANTHROPIC_KEY");
    println!("cargo:rerun-if-env-changed=TRAVIS_CLOUD_MODEL");

    // Read src-tauri/.env if present and forward each KEY=VALUE to rustc as a
    // compile-time env var. This makes the secrets reproducible across shells
    // and avoids the "I set $env:VAR but cargo didn't see it" trap on Windows.
    let env_path = Path::new(".env");
    if env_path.exists() {
        println!("cargo:rerun-if-changed=.env");
        if let Ok(content) = fs::read_to_string(env_path) {
            for raw in content.lines() {
                let line = raw.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, val)) = line.split_once('=') {
                    let key = key.trim();
                    let mut val = val.trim();
                    if (val.starts_with('"') && val.ends_with('"') && val.len() >= 2)
                        || (val.starts_with('\'') && val.ends_with('\'') && val.len() >= 2)
                    {
                        val = &val[1..val.len() - 1];
                    }
                    if !key.is_empty() {
                        println!("cargo:rustc-env={key}={val}");
                    }
                }
            }
        }
    }

    tauri_build::build()
}
