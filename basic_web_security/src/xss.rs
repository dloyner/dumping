//! XSS (Cross-Site Scripting)
//!
//! Big idea: user input ends up inside HTML without being escaped, so the
//! browser treats it as code and runs it under YOUR origin.
//!   - Stored:    payload is saved on the server (comment, chat) and shown to everyone.
//!   - Reflected: payload is in the request (e.g. `?q=...`) and echoed back immediately.
//!   - DOM-based: the bug lives purely in client-side JS (`innerHTML = location.hash`).
//!
//! Main defense: escape output by context (or use an auto-escaping template engine).
//! Defense in depth: CSP (see `headers`), HttpOnly cookies (see `session`).

use axum::{Router, extract::Query, response::Html, routing::get};
use serde::Deserialize;

/// Escape the 5 characters that matter in an HTML body / quoted attribute.
pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

/// VULNERABLE: reflects the query straight into HTML (reflected XSS).
async fn search_unsafe(Query(p): Query<SearchQuery>) -> Html<String> {
    Html(format!("<h1>Results for {}</h1>", p.q))
}

/// SAFE: output is escaped, the payload is shown as plain text.
async fn search_safe(Query(p): Query<SearchQuery>) -> Html<String> {
    Html(format!("<h1>Results for {}</h1>", escape_html(&p.q)))
}

pub fn router() -> Router {
    Router::new()
        .route("/xss/unsafe", get(search_unsafe))
        .route("/xss/safe", get(search_safe))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_tag_is_neutralized() {
        let out = escape_html("<script>alert(1)</script>");
        assert_eq!(out, "&lt;script&gt;alert(1)&lt;/script&gt;");
        assert!(!out.contains('<'));
    }

    #[test]
    fn quotes_are_escaped_for_attributes() {
        assert_eq!(escape_html(r#""onmouseover="x"#), "&quot;onmouseover=&quot;x");
    }
}
