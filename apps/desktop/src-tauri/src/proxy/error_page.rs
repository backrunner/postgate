use bytes::Bytes;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub enum ProxyErrorKind {
    Upstream,
    Request,
    Tunnel,
}

impl ProxyErrorKind {
    fn title(self) -> &'static str {
        match self {
            Self::Upstream => "Upstream error",
            Self::Request => "Request error",
            Self::Tunnel => "Tunnel error",
        }
    }

    fn summary(self) -> &'static str {
        match self {
            Self::Upstream => "PostGate could not complete the request to the upstream server.",
            Self::Request => "PostGate could not read or prepare the browser request.",
            Self::Tunnel => "PostGate could not establish or continue the HTTPS tunnel.",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Upstream => {
                "Check that the target service is running, reachable, and accepting this protocol."
            }
            Self::Request => "Check the request body, headers, and any request-modifying rules.",
            Self::Tunnel => "Check the CONNECT target, certificate trust, and TLS configuration.",
        }
    }
}

pub fn html_error_body(status: u16, kind: ProxyErrorKind, detail: &str) -> Bytes {
    Bytes::from(format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{status} {title} - PostGate</title>
  <style>
    :root {{
      color-scheme: light dark;
      --bg: #f4f4f5;
      --panel: #ffffff;
      --text: #18181b;
      --muted: #71717a;
      --line: #d4d4d8;
      --accent: #dc2626;
      --code-bg: #18181b;
      --code-text: #f4f4f5;
    }}
    @media (prefers-color-scheme: dark) {{
      :root {{
        --bg: #09090b;
        --panel: #18181b;
        --text: #f4f4f5;
        --muted: #a1a1aa;
        --line: #3f3f46;
        --accent: #f87171;
        --code-bg: #09090b;
        --code-text: #e4e4e7;
      }}
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      min-height: 100vh;
      display: grid;
      place-items: center;
      padding: 32px 18px;
      background:
        linear-gradient(135deg, color-mix(in srgb, var(--accent) 8%, transparent), transparent 32rem),
        var(--bg);
      color: var(--text);
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }}
    main {{
      width: min(760px, 100%);
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--panel);
      box-shadow: 0 24px 70px rgba(0, 0, 0, 0.14);
      overflow: hidden;
    }}
    header {{
      display: flex;
      gap: 16px;
      align-items: flex-start;
      padding: 28px 28px 20px;
      border-bottom: 1px solid var(--line);
    }}
    .badge {{
      flex: 0 0 auto;
      min-width: 64px;
      padding: 8px 10px;
      border-radius: 6px;
      background: color-mix(in srgb, var(--accent) 13%, transparent);
      color: var(--accent);
      text-align: center;
      font: 700 18px/1.1 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    }}
    h1 {{
      margin: 0;
      font-size: clamp(24px, 3vw, 34px);
      line-height: 1.08;
      letter-spacing: 0;
    }}
    p {{
      margin: 10px 0 0;
      color: var(--muted);
      line-height: 1.55;
      font-size: 15px;
    }}
    section {{ padding: 24px 28px 28px; }}
    h2 {{
      margin: 0 0 10px;
      font-size: 12px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--muted);
    }}
    pre {{
      margin: 0;
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      border-radius: 6px;
      padding: 16px;
      background: var(--code-bg);
      color: var(--code-text);
      font: 13px/1.5 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    }}
    footer {{
      display: flex;
      justify-content: space-between;
      gap: 16px;
      padding: 16px 28px;
      border-top: 1px solid var(--line);
      color: var(--muted);
      font-size: 13px;
    }}
    @media (max-width: 560px) {{
      header {{ display: block; padding: 22px 20px 18px; }}
      .badge {{ display: inline-block; margin-bottom: 14px; }}
      section {{ padding: 20px; }}
      footer {{ display: block; padding: 14px 20px; }}
      footer span {{ display: block; margin-top: 6px; }}
    }}
  </style>
</head>
<body>
  <main>
    <header>
      <div class="badge">{status}</div>
      <div>
        <h1>{title}</h1>
        <p>{summary}</p>
      </div>
    </header>
    <section>
      <h2>Error detail</h2>
      <pre>{detail}</pre>
    </section>
    <footer>
      <strong>PostGate proxy</strong>
      <span>{hint}</span>
    </footer>
  </main>
</body>
</html>"#,
        status = status,
        title = kind.title(),
        summary = kind.summary(),
        detail = escape_html(detail),
        hint = kind.hint(),
    ))
}

pub fn html_error_headers(body_len: usize) -> HashMap<String, String> {
    HashMap::from([
        (
            "content-type".to_string(),
            "text/html; charset=utf-8".to_string(),
        ),
        ("content-length".to_string(), body_len.to_string()),
        ("cache-control".to_string(), "no-store".to_string()),
        ("x-postgate-error".to_string(), "1".to_string()),
    ])
}

fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
