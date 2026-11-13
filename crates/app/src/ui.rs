//! Agent UIs (docs/design/live-items.md): what a UI's page is, the rules of
//! its sandbox and bridge v1. Pure: `platform` builds the web view from the
//! constants here and pipes every load, navigation and message through a
//! [`Bridge`], which decides.
//!
//! A page is served from `slinger-ui://<hex>/`, the hex of its blob id, with
//! [`CSP`] on the response; [`RULES`] blocks every other load; the bridge lets
//! only that first load through. [`SHIM`] defines `window.slinger`, whose two
//! calls post JSON text to the handler named [`HANDLER`]. [`script`] adds the
//! look a page starts from: the canvas's paper, ink and type ([`LOOK`]).

use std::sync::Arc;

use serde_json::{Value, json};

use crate::config::{Rgb, Theme};

/// The URL scheme the page is served from.
pub const SCHEME: &str = "slinger-ui";

/// The name of the one script message handler.
pub const HANDLER: &str = "slinger";

/// The bridge version this host speaks.
pub const BRIDGE: &str = "1";

/// The content security policy on the page's response, so the page cannot
/// leave it out. One line, with no newline at the end.
pub const CSP: &str = include_str!("ui/policy.txt");

/// The content rule list: block every load but the scheme's.
pub const RULES: &str = r#"[{"trigger":{"url-filter":".*"},"action":{"type":"block"}},{"trigger":{"url-filter":"^slinger-ui:"},"action":{"type":"ignore-previous-rules"}}]"#;

/// The name the rule list is compiled under.
pub const RULES_ID: &str = "slinger-ui-sandbox-1";

/// Defines `window.slinger` at document start, in the main frame only.
pub const SHIM: &str = include_str!("ui/shim.js");

/// The look every page starts from, under its own styles: each rule is
/// `:where()`, so any rule of the page's wins. Its colours are the
/// `--slinger-*` variables [`script`] sets from the theme. It goes into a
/// JavaScript template literal: no backtick, backslash or `${`.
pub const LOOK: &str = include_str!("ui/look.css");

/// The user script each page gets at document start: [`SHIM`], then a
/// style element, first in the document, that sets the theme's colours as
/// `--slinger-paper`, `-ink`, `-muted`, `-accent` and `-error`, the colour
/// scheme its paper calls for, and [`LOOK`]. A page with no styles of its
/// own sits on the canvas as its ink does.
pub fn script(theme: &Theme) -> String {
    // By hand, not `format!`: hex formatting costs the app its size budget.
    let mut js = String::from(SHIM);
    js.push_str("(()=>{const s=document.createElement(\"style\");s.textContent=`:root{");
    let colours = [
        ("paper", theme.background),
        ("ink", theme.text),
        ("muted", theme.muted),
        ("accent", theme.focus),
        ("error", theme.error),
    ];
    for (name, Rgb { r, g, b }) in colours {
        js.push_str("--slinger-");
        js.push_str(name);
        js.push_str(":#");
        for nibble in [r >> 4, r & 15, g >> 4, g & 15, b >> 4, b & 15] {
            js.push(char::from_digit(u32::from(nibble), 16).unwrap_or('0'));
        }
        js.push(';');
    }
    let Rgb { r, g, b } = theme.background;
    let light = u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 > 127_500;
    js.push_str(if light { "color-scheme:light}" } else { "color-scheme:dark}" });
    js.push_str(LOOK);
    js.push_str("`;document.documentElement.prepend(s)})();");
    js
}

/// The largest reply, serialized.
pub const MAX_REPLY: usize = 16384;

/// The most replies one showing sends.
pub const MAX_REPLIES: u32 = 100;

/// The largest data a UI is given, serialized.
pub const MAX_DATA: usize = 16384;

/// The data `text` gives a UI's context.
///
/// # Errors
///
/// What to fix: text that is not JSON, or over [`MAX_DATA`] bytes.
pub fn data(text: &str) -> Result<Value, String> {
    if text.len() > MAX_DATA {
        return Err(format!("the data is at most {MAX_DATA} bytes"));
    }
    serde_json::from_str(text).map_err(|e| format!("the data is not JSON: {e}"))
}

/// A UI's code as the daemon keeps it: one version of a name, by hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    pub name: String,
    pub version: u64,
    /// The blob id: `sha256:` and 64 lowercase hex characters.
    pub hash: String,
    pub html: Vec<u8>,
}

/// What a pane shows: a UI's code and, when an agent opened it with
/// `ui open`, the showing's id and the agent's data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub code: Code,
    /// The showing's id, which its replies name; `None` for a UI opened by
    /// name, whose replies go nowhere.
    pub open: Option<String>,
    pub data: Value,
}

/// A UI ready to show: its code, the URL it is served from, the showing it
/// is and the context `slinger.context()` gives it.
#[derive(Debug, PartialEq)]
pub struct Page {
    pub code: Code,
    pub url: String,
    pub open: Option<String>,
    pub context: String,
}

impl Page {
    /// The page for `shown` in `host` (`pane` or `canvas`).
    ///
    /// # Errors
    ///
    /// The line to show instead: a hash that is not a blob id, or a bridge
    /// version this host does not speak.
    pub fn new(shown: Shown, host: &str) -> Result<Self, String> {
        let Shown { code, open, data } = shown;
        let hex = code
            .hash
            .strip_prefix("sha256:")
            .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| format!("{} is not a blob id", code.hash))?;
        if let Some(v) = declared(&String::from_utf8_lossy(&code.html))
            && v != BRIDGE
        {
            return Err(format!("bridge v{v} is not supported"));
        }
        let url = format!("{SCHEME}://{}/", hex.to_ascii_lowercase());
        let context = json!({
            "v": 1,
            "ui": { "name": code.name, "version": code.version, "hash": code.hash },
            "host": host,
            "open": open,
            "data": data,
        })
        .to_string();
        Ok(Self { code, url, open, context })
    }

    /// The response to a load of `url`: the page for its own URL, with the
    /// policy; 404 with no body for any other.
    pub fn respond(&self, url: &str) -> Response<'_> {
        if url == self.url {
            let headers = vec![
                ("Content-Type", "text/html; charset=utf-8"),
                ("Content-Security-Policy", CSP),
            ];
            Response { status: 200, headers, body: &self.code.html }
        } else {
            Response { status: 404, headers: Vec::new(), body: &[] }
        }
    }
}

/// What the scheme handler answers a load with.
#[derive(Debug, PartialEq, Eq)]
pub struct Response<'a> {
    pub status: i16,
    pub headers: Vec<(&'static str, &'static str)>,
    pub body: &'a [u8],
}

/// The bridge version a page declares with
/// `<meta name="slinger-bridge" content="N">`, if any.
fn declared(html: &str) -> Option<&str> {
    // ASCII lowercase keeps every byte where it was, so offsets carry over.
    let lower = html.to_ascii_lowercase();
    let at = lower.find("name=\"slinger-bridge\"")?;
    let start = lower[..at].rfind('<')?;
    let end = at + lower[at..].find('>')?;
    let content = start + lower[start..end].find("content=\"")? + "content=\"".len();
    let close = content + lower[content..end].find('"')?;
    Some(&html[content..close])
}

/// Why the bridge refused a call: `<code>: <message>` to the page, which
/// turns it into an `Error` with that `code`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

fn refuse(code: &'static str, message: impl Into<String>) -> Refusal {
    Refusal { code, message: message.into() }
}

/// One showing of a page: what it may load and what its calls get.
#[derive(Debug)]
pub struct Bridge {
    page: Arc<Page>,
    loaded: bool,
    replies: u32,
}

impl Bridge {
    pub fn new(page: Arc<Page>) -> Self {
        Self { page, loaded: false, replies: 0 }
    }

    pub fn page(&self) -> &Page {
        &self.page
    }

    /// Whether a navigation to `url` may go ahead: the page's own load in
    /// the main frame, once. Every other is cancelled.
    pub fn navigate(&mut self, url: &str, main_frame: bool) -> bool {
        let first = !self.loaded && main_frame && url == self.page.url;
        self.loaded |= first;
        first
    }

    /// The answer to `message` from the shim: the context as JSON text, or
    /// `null` once `send` took a reply (its `ui_reply` line).
    ///
    /// # Errors
    ///
    /// `invalid` for a message or value that is not one, `too_large`,
    /// `limit`, and `unavailable` with `send`'s message.
    pub fn call(
        &mut self,
        message: &str,
        send: impl FnOnce(String) -> Result<(), String>,
    ) -> Result<String, Refusal> {
        let message: Value = serde_json::from_str(message)
            .map_err(|_| refuse("invalid", "a message is JSON text"))?;
        if message["v"] != 1 {
            return Err(refuse("invalid", "this host speaks bridge v1"));
        }
        match (message["call"].as_str(), message["value"].as_str()) {
            (Some("context"), _) => Ok(self.page.context.clone()),
            (Some("reply"), Some(text)) => {
                if text.len() > MAX_REPLY {
                    return Err(refuse(
                        "too_large",
                        format!("a reply is at most {MAX_REPLY} bytes"),
                    ));
                }
                if self.replies >= MAX_REPLIES {
                    return Err(refuse(
                        "limit",
                        format!("a showing sends at most {MAX_REPLIES} replies"),
                    ));
                }
                let value: Value = serde_json::from_str(text)
                    .map_err(|_| refuse("invalid", "the value is not JSON"))?;
                let code = &self.page.code;
                let line = json!({
                    "v": 1, "kind": "ui_reply", "ui": code.name, "version": code.version,
                    "hash": code.hash, "open": self.page.open, "value": value,
                });
                send(line.to_string()).map_err(|e| refuse("unavailable", e))?;
                self.replies += 1;
                Ok("null".to_string())
            }
            _ => Err(refuse("invalid", "the calls are context and reply")),
        }
    }
}

#[cfg(test)]
mod tests;
