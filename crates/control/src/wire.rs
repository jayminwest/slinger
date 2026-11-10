//! The wire format: one JSON object per line. A connection carries one request
//! line and one reply line. docs/service.md is the contract.

use std::io::{self, BufRead, Write};

use serde_json::{Map, Value, json};

use crate::{RemoteError, Reply, Request};

/// The protocol version, `v` in every request. See "Versioning" in docs/service.md.
pub const VERSION: u64 = 1;

/// The longest request line the daemon reads, in bytes, newline excluded.
pub const MAX_LINE: usize = 64 * 1024;

const SEE_DOCS: &str = "see docs/service.md for the request format";

/// Reads one request line. `Ok(None)`: the peer closed without sending one.
///
/// # Errors
///
/// `bad_request` for a line that is not a request, `too_large` for a line
/// over [`MAX_LINE`] and `unsupported_version` for another protocol version.
/// The daemon sends the error back as the reply.
pub fn read_request(reader: &mut impl BufRead) -> Result<Option<Request>, RemoteError> {
    let bad = |what: String| RemoteError::new("bad_request", what, Some(SEE_DOCS.into()));
    let line = match read_line(reader) {
        Ok(Line::Text(line)) => line,
        Ok(Line::End) => return Ok(None),
        Ok(Line::TooLong) => {
            let what = format!("request is over {MAX_LINE} bytes");
            return Err(RemoteError::new("too_large", what, Some("send a smaller request".into())));
        }
        Err(e) => return Err(bad(format!("request did not arrive: {e}"))),
    };
    let doc: Value =
        serde_json::from_slice(&line).map_err(|e| bad(format!("request is not JSON: {e}")))?;
    match doc.get("v").map(Value::as_u64) {
        Some(Some(VERSION)) => {}
        Some(_) => {
            return Err(RemoteError::new(
                "unsupported_version",
                format!("protocol version {} is not supported", doc["v"]),
                Some(format!("this daemon speaks version {VERSION}; upgrade the older side")),
            ));
        }
        None => return Err(bad("request has no protocol version `v`".into())),
    }
    serde_json::from_value(doc).map(Some).map_err(|e| bad(format!("request is not valid: {e}")))
}

enum Line {
    /// The line, without its newline.
    Text(Vec<u8>),
    /// End of input before any byte.
    End,
    /// A line over [`MAX_LINE`]. It was read to its end and dropped, so the
    /// peer can finish writing before it reads the reply.
    TooLong,
}

fn read_line(reader: &mut impl BufRead) -> io::Result<Line> {
    let mut line = Vec::new();
    let mut seen = 0_usize;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() && seen == 0 {
            return Ok(Line::End);
        }
        if chunk.is_empty() {
            break;
        }
        let end = chunk.iter().position(|b| *b == b'\n');
        let take = end.unwrap_or(chunk.len());
        if seen + take <= MAX_LINE {
            line.extend_from_slice(&chunk[..take]);
        }
        seen += take;
        reader.consume(end.map_or(take, |at| at + 1));
        if end.is_some() {
            break;
        }
    }
    Ok(if seen <= MAX_LINE { Line::Text(line) } else { Line::TooLong })
}

/// Writes one reply line: `{"ok":true,...}` or `{"ok":false,"code":...,"error":...}`.
///
/// # Errors
///
/// The writer's error.
pub fn write_reply(writer: &mut impl Write, reply: &Result<Reply, RemoteError>) -> io::Result<()> {
    let body = match reply {
        Ok(reply) => serde_json::to_value(reply),
        Err(error) => serde_json::to_value(error),
    };
    let mut doc = Map::new();
    doc.insert("ok".into(), json!(reply.is_ok()));
    if let Value::Object(fields) = body.map_err(io::Error::other)? {
        doc.extend(fields);
    }
    let mut line = Value::Object(doc).to_string();
    line.push('\n');
    writer.write_all(line.as_bytes())?;
    writer.flush()
}

/// The request line a client sends: a [`Request`], or a type that writes
/// the same line for verbs [`Request`] does not serialize.
pub(crate) fn request_line<R: serde::Serialize>(request: &R) -> io::Result<String> {
    #[derive(serde::Serialize)]
    struct Line<'a, R> {
        v: u64,
        #[serde(flatten)]
        request: &'a R,
    }
    let mut line =
        serde_json::to_string(&Line { v: VERSION, request }).map_err(io::Error::other)?;
    line.push('\n');
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Greeting;

    fn read(input: &[u8]) -> Result<Option<Request>, RemoteError> {
        read_request(&mut io::BufReader::with_capacity(16, input))
    }

    fn code(input: &[u8]) -> String {
        read(input).unwrap_err().code().to_string()
    }

    #[test]
    fn reads_what_a_client_writes() {
        let request = Request::Hello { name: "ada".into() };
        let line = request_line(&request).unwrap();
        assert_eq!(line, "{\"v\":1,\"op\":\"hello\",\"name\":\"ada\"}\n");
        assert_eq!(read(line.as_bytes()).unwrap(), Some(request));
    }

    #[test]
    fn ignores_unknown_fields_and_a_missing_newline() {
        let line = br#"{"v":1,"op":"hello","name":"ada","later":true}"#;
        assert_eq!(read(line).unwrap(), Some(Request::Hello { name: "ada".into() }));
    }

    #[test]
    fn record_requests_carry_their_fields_flat_and_default_the_rest() {
        let line = br#"{"v":1,"op":"record_append","kind":"agent_done","source":"cli"}"#;
        let Some(Request::RecordAppend(event)) = read(line).unwrap() else { panic!() };
        assert_eq!((event.tags, event.payload), (crate::NewTags::default(), json!({})));
        let filter = crate::EventFilter {
            kind: Some("agent_done".into()),
            after: Some(3),
            ..Default::default()
        };
        let line = request_line(&Request::RecordList(filter.clone())).unwrap();
        assert_eq!(line, "{\"v\":1,\"op\":\"record_list\",\"kind\":\"agent_done\",\"after\":3}\n");
        assert_eq!(read(line.as_bytes()).unwrap(), Some(Request::RecordList(filter)));
        let line = br#"{"v":1,"op":"record_list"}"#;
        assert_eq!(read(line).unwrap(), Some(Request::RecordList(crate::EventFilter::default())));
    }

    #[test]
    fn ui_requests_round_trip() {
        for (request, line) in [
            (
                Request::UiPut { name: "a".into(), path: "/tmp/a.html".into(), source: "s".into() },
                r#"{"v":1,"op":"ui_put","name":"a","path":"/tmp/a.html","source":"s"}"#,
            ),
            (Request::UiList, r#"{"v":1,"op":"ui_list"}"#),
            (Request::UiShow { name: "a".into() }, r#"{"v":1,"op":"ui_show","name":"a"}"#),
            (
                Request::UiExport { ui: "a@2".into(), dir: "/tmp".into() },
                r#"{"v":1,"op":"ui_export","ui":"a@2","dir":"/tmp"}"#,
            ),
        ] {
            assert_eq!(request_line(&request).unwrap(), format!("{line}\n"));
            assert_eq!(read(line.as_bytes()).unwrap(), Some(request));
        }
    }

    #[test]
    fn blob_requests_name_a_file_not_its_bytes() {
        let put = Request::BlobPut { path: "/tmp/a".into() };
        let line = request_line(&put).unwrap();
        assert_eq!(line, "{\"v\":1,\"op\":\"blob_put\",\"path\":\"/tmp/a\"}\n");
        assert_eq!(read(line.as_bytes()).unwrap(), Some(put));
        let get = Request::BlobGet { id: "sha256:00".into(), path: "/tmp/b".into() };
        let line = request_line(&get).unwrap();
        assert_eq!(
            line,
            "{\"v\":1,\"op\":\"blob_get\",\"id\":\"sha256:00\",\"path\":\"/tmp/b\"}\n"
        );
        assert_eq!(read(line.as_bytes()).unwrap(), Some(get));
    }

    #[test]
    fn end_of_input_is_not_a_request() {
        assert_eq!(read(b"").unwrap(), None);
    }

    #[test]
    fn rejects_malformed_requests_as_bad_request() {
        for line in [
            &b"\n"[..],
            b"not json\n",
            b"\xff\xfe\n",
            b"[1]\n",
            br#"{"op":"hello","name":"ada"}"#,
            br#"{"v":1}"#,
            br#"{"v":1,"op":"nope"}"#,
            br#"{"v":1,"op":"hello"}"#,
            br#"{"v":1,"op":"hello","name":7}"#,
            br#"{"v":1,"op":"record_append","source":"cli"}"#,
            br#"{"v":1,"op":"record_list","after":-1}"#,
            br#"{"v":1,"op":"blob_get","path":"/tmp/b"}"#,
        ] {
            let err = read(line).unwrap_err();
            assert_eq!(err.code(), "bad_request", "{}", String::from_utf8_lossy(line));
            assert!(err.hint().is_some());
        }
    }

    #[test]
    fn rejects_other_versions() {
        assert_eq!(code(br#"{"v":2,"op":"hello","name":"ada"}"#), "unsupported_version");
        assert_eq!(code(br#"{"v":"1","op":"hello","name":"ada"}"#), "unsupported_version");
    }

    #[test]
    fn rejects_an_oversized_line_and_reads_it_to_its_end() {
        let mut input = vec![b'x'; MAX_LINE + 1];
        input.extend_from_slice(b"\nnext");
        let mut reader = io::BufReader::new(&input[..]);
        assert_eq!(read_request(&mut reader).unwrap_err().code(), "too_large");
        assert_eq!(reader.buffer(), b"next");
    }

    #[test]
    fn accepts_a_line_of_exactly_the_limit() {
        let line = |name: &str| format!("{{ \"v\":1,\"op\":\"hello\",\"name\":\"{name}\" }}\n");
        let padding = "a".repeat(MAX_LINE + 1 - line("").len());
        assert_eq!(line(&padding).len(), MAX_LINE + 1);
        assert!(read(line(&padding).as_bytes()).unwrap().is_some());
        assert_eq!(code(line(&format!("{padding}a")).as_bytes()), "too_large");
    }

    #[test]
    fn writes_success_and_failure_as_one_line_each() {
        let mut out = Vec::new();
        let greeting = Greeting { greeting: "hello, ada".into() };
        write_reply(&mut out, &Ok(Reply::Hello(greeting))).unwrap();
        write_reply(&mut out, &Err(RemoteError::new("invalid", "name is empty", None))).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "{\"greeting\":\"hello, ada\",\"ok\":true}\n\
             {\"code\":\"invalid\",\"error\":\"name is empty\",\"ok\":false}\n"
        );
    }
}
