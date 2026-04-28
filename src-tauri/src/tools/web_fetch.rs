use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::llm::ToolDef;

use super::{Tool, ToolContext};

pub struct WebFetchTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebFetchInput {
    url: String,
    #[serde(default)]
    max_chars: Option<usize>,
}

#[async_trait]
impl Tool for WebFetchTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "web_fetch".into(),
            description: "Fetch a single web page by URL and return its main text content (HTML stripped). Use when the user references a specific URL or asks for content from a known page. Don't use for general web searches — only for retrieving a known URL.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Full http(s) URL to fetch." },
                    "maxChars": {
                        "type": "integer",
                        "description": "Cap returned text length (default 6000, max 20000)."
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: WebFetchInput = serde_json::from_value(input)?;
        let url = p.url.trim().to_string();
        if !url.starts_with("http://") && !url.starts_with("https://") {
            anyhow::bail!("url must start with http:// or https://");
        }
        let max_chars = p.max_chars.unwrap_or(6000).min(20_000);

        let resp = ctx
            .http
            .get(&url)
            .header("user-agent", "Travis/0.1 (personal-ops-assistant)")
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("fetch error: {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!(
                "HTTP {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("")
            );
        }
        let body = resp
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("body decode: {e}"))?;

        let title = extract_title(&body).unwrap_or_else(|| "(no title)".into());
        let text = strip_html_to_text(&body);
        let truncated: String = text.chars().take(max_chars).collect();
        let elided = if text.chars().count() > max_chars {
            "\n\n[…truncated]"
        } else {
            ""
        };

        Ok(format!(
            "URL: {url}\nTitle: {title}\n\n{truncated}{elided}"
        ))
    }
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let after_open = html[start..].find('>')? + start + 1;
    let close_rel = html[after_open..].to_lowercase().find("</title>")?;
    Some(html[after_open..after_open + close_rel].trim().to_string())
}

fn strip_html_to_text(html: &str) -> String {
    let mut s = drop_block(html, "script");
    s = drop_block(&s, "style");
    s = drop_block(&s, "noscript");

    let mut out = String::with_capacity(s.len() / 2);
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }

    let out = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'");

    let mut collapsed = String::with_capacity(out.len());
    let mut last_was_space = false;
    for c in out.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                collapsed.push(' ');
                last_was_space = true;
            }
        } else {
            collapsed.push(c);
            last_was_space = false;
        }
    }
    collapsed.trim().to_string()
}

fn drop_block(html: &str, tag: &str) -> String {
    let lower = html.to_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0usize;
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    while cursor < html.len() {
        if let Some(start_rel) = lower[cursor..].find(&open) {
            let abs_start = cursor + start_rel;
            if let Some(open_end_rel) = html[abs_start..].find('>') {
                let after_open = abs_start + open_end_rel + 1;
                if let Some(close_rel) = lower[after_open..].find(&close) {
                    let abs_close = after_open + close_rel + close.len();
                    out.push_str(&html[cursor..abs_start]);
                    cursor = abs_close;
                    continue;
                }
            }
        }
        out.push_str(&html[cursor..]);
        break;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_basic_html() {
        let h = "<html><head><title>Hi</title></head><body><p>Hello <b>world</b>.</p></body></html>";
        let text = strip_html_to_text(h);
        assert!(text.contains("Hi"));
        assert!(text.contains("Hello world."));
        assert_eq!(extract_title(h).as_deref(), Some("Hi"));
    }

    #[test]
    fn drops_scripts_and_styles() {
        let h = "<style>body{color:red}</style><p>Visible</p><script>alert('x')</script>";
        let text = strip_html_to_text(h);
        assert!(text.contains("Visible"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("color:red"));
    }

    #[test]
    fn decodes_entities() {
        let h = "<p>Tom &amp; Jerry &lt;3 &nbsp;cats</p>";
        let text = strip_html_to_text(h);
        assert!(text.contains("Tom & Jerry <3"));
    }
}
