//! Shared helpers used by both the Google and Microsoft OAuth flows: the
//! tiny date math we need to compare token-expiry timestamps without pulling
//! `chrono` parsing into the hot path, and the localhost-listener callback
//! parser. Each provider supplies its own success-page HTML.

use std::time::Duration;

use anyhow::anyhow;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Return an ISO-8601 UTC timestamp `d` seconds from now.
pub(crate) fn iso_utc_in(d: Duration) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let target = now + d.as_secs() as i64;
    let (y, m, day) = days_to_ymd(target / 86400);
    let secs_in_day = (target % 86400) as u32;
    let h = secs_in_day / 3600;
    let mi = (secs_in_day % 3600) / 60;
    let s = secs_in_day % 60;
    format!("{y:04}-{m:02}-{day:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// True if `iso_utc` is in the past (or within 60s of now). Returns true on
/// parse failure so callers fall through to a refresh.
pub(crate) fn is_expired(iso_utc: &str) -> bool {
    let bytes = iso_utc.as_bytes();
    if bytes.len() < 19 {
        return true;
    }
    let parse_n = |s: &str| -> Option<i64> { s.parse().ok() };
    let y: i64 = parse_n(&iso_utc[0..4]).unwrap_or(0);
    let m: i64 = parse_n(&iso_utc[5..7]).unwrap_or(0);
    let d: i64 = parse_n(&iso_utc[8..10]).unwrap_or(0);
    let h: i64 = parse_n(&iso_utc[11..13]).unwrap_or(0);
    let mi: i64 = parse_n(&iso_utc[14..16]).unwrap_or(0);
    let s: i64 = parse_n(&iso_utc[17..19]).unwrap_or(0);
    let target = ymd_to_days(y as i32, m as u32, d as u32) * 86400 + h * 3600 + mi * 60 + s;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    target - 60 <= now
}

fn ymd_to_days(y: i32, m: u32, d: u32) -> i64 {
    // Civil-from-days, inverse. Howard Hinnant.
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn days_to_ymd(days_since_epoch: i64) -> (i32, u32, u32) {
    let mut days = days_since_epoch + 719468;
    let era = if days >= 0 { days / 146097 } else { (days - 146096) / 146097 };
    days -= era * 146097;
    let yoe = (days - days / 1460 + days / 36524 - days / 146096) / 365;
    let y = (yoe + era * 400) as i32;
    let doy = days - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Accept exactly one connection on `listener`, parse the OAuth `code` and
/// `state` query params from the GET line, and respond with `success_html`
/// (or a generic error page if the upstream returned an error). Returns
/// `(code, state)`.
pub(crate) async fn wait_for_callback(
    listener: TcpListener,
    success_html: &'static str,
) -> anyhow::Result<(String, String)> {
    let (mut socket, _addr) = listener
        .accept()
        .await
        .map_err(|e| anyhow!("accept: {e}"))?;

    let mut buf = vec![0u8; 8192];
    let n = socket
        .read(&mut buf)
        .await
        .map_err(|e| anyhow!("read: {e}"))?;
    let req = String::from_utf8_lossy(&buf[..n]);

    let first_line = req.lines().next().unwrap_or("");
    let path_and_query = first_line.split_whitespace().nth(1).unwrap_or("/");
    let url = url::Url::parse(&format!("http://127.0.0.1{path_and_query}"))
        .map_err(|e| anyhow!("parse callback url: {e}"))?;

    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            _ => {}
        }
    }

    let body: &str = if error.is_some() || code.is_none() {
        r#"<!doctype html><html><body style="font-family:system-ui;background:#0a0a18;color:#ececf1;padding:40px"><h2>Something went wrong</h2><p>Travis didn't receive a permission code. You can close this tab and try again from Settings.</p></body></html>"#
    } else {
        success_html
    };
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = socket.write_all(resp.as_bytes()).await;
    let _ = socket.shutdown().await;

    if let Some(err) = error {
        anyhow::bail!("provider declined the request: {err}");
    }
    let code = code.ok_or_else(|| anyhow!("missing code in callback"))?;
    let state = state.ok_or_else(|| anyhow!("missing state in callback"))?;
    Ok((code, state))
}
