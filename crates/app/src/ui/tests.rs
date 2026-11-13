use super::*;

const HASH: &str = "sha256:9F2C000000000000000000000000000000000000000000000000000000000001";

fn code(html: &str) -> Code {
    Code { name: "pick-slot".into(), version: 3, hash: HASH.into(), html: html.into() }
}

fn shown(code: Code) -> Shown {
    Shown { code, open: None, data: Value::Null }
}

fn page(html: &str) -> Arc<Page> {
    let open = Some("pick-slot#12".to_string());
    let shown = Shown { code: code(html), open, data: json!({"slots": ["Tue"]}) };
    Arc::new(Page::new(shown, "pane").unwrap())
}

#[test]
fn a_page_is_served_from_its_hash_with_the_policy_and_nothing_else_is() {
    let page = page("<p>hi</p>");
    let url = "slinger-ui://9f2c000000000000000000000000000000000000000000000000000000000001/";
    assert_eq!(page.url, url);
    let served = page.respond(url);
    assert_eq!((served.status, served.body), (200, b"<p>hi</p>".as_slice()));
    assert!(served.headers.contains(&("Content-Security-Policy", CSP)));
    for other in
        [&format!("{url}x"), "slinger-ui://other/", "file:///etc/hosts", "http://127.0.0.1/"]
    {
        assert_eq!(page.respond(other), Response { status: 404, headers: vec![], body: &[] });
    }
}

#[test]
fn the_context_says_what_the_ui_was_given() {
    let context: Value = serde_json::from_str(&page("").context).unwrap();
    assert_eq!(
        context,
        json!({
            "v": 1, "ui": {"name": "pick-slot", "version": 3, "hash": HASH},
            "host": "pane", "open": "pick-slot#12", "data": {"slots": ["Tue"]},
        })
    );
    let by_name = Page::new(shown(code("")), "pane").unwrap();
    let context: Value = serde_json::from_str(&by_name.context).unwrap();
    assert_eq!((&context["open"], &context["data"]), (&Value::Null, &Value::Null));
}

#[test]
fn a_page_that_is_not_one_or_declares_another_bridge_shows_a_line_instead() {
    let bad = Code { hash: "sha256:xyz".into(), ..code("") };
    assert_eq!(Page::new(shown(bad), "pane").unwrap_err(), "sha256:xyz is not a blob id");
    let v2 = code("<!doctype html>\n<META Name=\"slinger-bridge\" content=\"2\">");
    assert_eq!(Page::new(shown(v2), "pane").unwrap_err(), "bridge v2 is not supported");
    for fine in
        ["<meta name=\"slinger-bridge\" content=\"1\">", "<meta name=\"slinger-bridge\">", "<p>"]
    {
        assert!(Page::new(shown(code(fine)), "pane").is_ok(), "{fine}");
    }
}

#[test]
fn only_the_first_load_of_the_page_in_the_main_frame_goes_ahead() {
    let page = page("");
    let mut bridge = Bridge::new(Arc::clone(&page));
    assert!(!bridge.navigate("http://127.0.0.1:1/", true));
    assert!(!bridge.navigate(&page.url, false), "a frame");
    assert!(bridge.navigate(&page.url, true));
    assert!(!bridge.navigate(&page.url, true), "a reload");
    assert_eq!(bridge.page(), &*page);
}

#[test]
fn context_answers_with_the_context() {
    let page = page("");
    let mut bridge = Bridge::new(Arc::clone(&page));
    assert_eq!(bridge.call(r#"{"v":1,"call":"context"}"#, |_| Ok(())), Ok(page.context.clone()));
}

#[test]
fn a_reply_goes_out_as_one_ui_reply_line() {
    let mut bridge = Bridge::new(page(""));
    let mut sent = Vec::new();
    let answer = bridge.call(r#"{"v":1,"call":"reply","value":"{\"slot\":\"Wed\"}"}"#, |line| {
        sent.push(line);
        Ok(())
    });
    assert_eq!(answer, Ok("null".into()));
    let line: Value = serde_json::from_str(&sent[0]).unwrap();
    assert_eq!(
        line,
        json!({
            "v": 1, "kind": "ui_reply", "ui": "pick-slot", "version": 3, "hash": HASH,
            "open": "pick-slot#12", "value": {"slot": "Wed"},
        })
    );
}

fn code_of(answer: Result<String, Refusal>) -> &'static str {
    answer.unwrap_err().code
}

#[test]
fn bad_calls_are_refused_with_a_stable_code() {
    let mut bridge = Bridge::new(page(""));
    for message in
        ["", "[]", r#"{"call":"context"}"#, r#"{"v":1,"call":"open"}"#, r#"{"v":1,"call":"reply"}"#]
    {
        assert_eq!(code_of(bridge.call(message, |_| Ok(()))), "invalid", "{message}");
    }
    let not_json = json!({"v": 1, "call": "reply", "value": "{"}).to_string();
    assert_eq!(code_of(bridge.call(&not_json, |_| Ok(()))), "invalid");
    let big = json!({"v": 1, "call": "reply", "value": format!("\"{}\"", "x".repeat(MAX_REPLY))});
    assert_eq!(code_of(bridge.call(&big.to_string(), |_| Ok(()))), "too_large");
    let reply = r#"{"v":1,"call":"reply","value":"1"}"#;
    let refused = bridge.call(reply, |_| Err("no agent".into())).unwrap_err();
    assert_eq!(refused.to_string(), "unavailable: no agent");
}

#[test]
fn a_showing_sends_at_most_the_limit_of_replies() {
    let mut bridge = Bridge::new(page(""));
    let reply = r#"{"v":1,"call":"reply","value":"1"}"#;
    for _ in 0..MAX_REPLIES {
        assert!(bridge.call(reply, |_| Ok(())).is_ok());
    }
    assert_eq!(code_of(bridge.call(reply, |_| Ok(()))), "limit");
}

#[test]
fn data_is_json_of_at_most_the_limit() {
    assert_eq!(data(r#"{"port":1}"#), Ok(json!({"port": 1})));
    assert!(data("{").unwrap_err().starts_with("the data is not JSON"));
    let big = format!("\"{}\"", "x".repeat(MAX_DATA));
    assert_eq!(data(&big).unwrap_err(), format!("the data is at most {MAX_DATA} bytes"));
}

#[test]
fn the_policy_is_one_header_line() {
    assert!(CSP.starts_with("default-src 'none';") && CSP.ends_with("base-uri 'none'"));
    assert!(!CSP.chars().any(char::is_control));
}

#[test]
fn a_page_starts_from_the_themes_paper_ink_and_type_under_its_own_styles() {
    let theme = Theme::default();
    let dark = script(&theme);
    assert!(dark.starts_with(SHIM), "the bridge first");
    assert!(dark.contains("--slinger-paper:#1e1e1e;--slinger-ink:#e6e6e6;"), "{dark}");
    assert!(dark.contains("--slinger-accent:#4c9aff;") && dark.contains("color-scheme:dark"));
    assert!(dark.contains("documentElement.prepend"), "before the page's own styles");
    for rule in LOOK.lines() {
        assert!(rule.starts_with(":where("), "a page's rule wins over {rule}");
    }
    assert!(LOOK.contains("var(--slinger-paper)") && LOOK.contains("system-ui"));
    assert!(!LOOK.contains('`') && !LOOK.contains('\\') && !LOOK.contains("${"));
    let white = Rgb { r: 0xfa, g: 0xfa, b: 0xf7 };
    let light = script(&Theme { background: white, ..theme });
    assert!(light.contains("--slinger-paper:#fafaf7;") && light.contains("color-scheme:light"));
}

#[test]
fn the_guide_names_only_colours_a_page_is_given() {
    let guide = include_str!("../../../cli/src/guide.md");
    let given = script(&Theme::default());
    let named: Vec<_> = guide.split("var(").skip(1).filter_map(|t| t.split(')').next()).collect();
    assert!(named.len() >= 5, "{named:?}");
    for name in named {
        assert!(given.contains(&format!("{name}:#")), "the guide names {name}");
    }
}
