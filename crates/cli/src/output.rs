//! The output contract (docs/cli.md). Commands return data; this module alone
//! decides how it reaches the terminal:
//! - human mode: the result on stdout, `✗ message` and a hint on stderr
//! - `--json`: exactly one envelope document on stdout, errors included

use std::io::{self, Write};

use serde_json::{Map, Value, json};

use slinger_control::RemoteError as Known;

/// Stable exit codes. Scripts and agents branch on these; never renumber.
pub mod exit {
    pub const OK: u8 = 0;
    pub const ERROR: u8 = 1;
    pub const USAGE: u8 = 2;
    pub const NOT_FOUND: u8 = 3;
    pub const CONFLICT: u8 = 4;
    pub const INVALID: u8 = 5;
    pub const UNAVAILABLE: u8 = 6;
}

/// Color only on a TTY; `NO_COLOR` wins, then `FORCE_COLOR`.
pub fn color_enabled(no_color: bool, force_color: Option<&str>, is_tty: bool) -> bool {
    if no_color {
        return false;
    }
    force_color.map_or(is_tty, |v| v != "0")
}

/// A command's result: `data` for `--json` (an object), `human` for the terminal.
pub struct Output {
    data: Value,
    human: String,
}

impl Output {
    pub fn new(data: Value, human: String) -> Self {
        Self { data, human }
    }
}

/// A failure as callers see it: a stable code, a message, a hint, an exit code.
#[derive(Debug, PartialEq)]
pub struct Failure {
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
    pub exit: u8,
}

impl Failure {
    pub fn usage(message: &str, hint: String) -> Self {
        let (code, message) = ("usage".to_string(), message.to_string());
        Self { code, message, hint: Some(hint), exit: exit::USAGE }
    }
}

impl From<&anyhow::Error> for Failure {
    fn from(err: &anyhow::Error) -> Self {
        let Some(known) = err.downcast_ref::<Known>() else {
            return Self {
                code: "internal".into(),
                message: format!("{err:#}"),
                hint: None,
                exit: exit::ERROR,
            };
        };
        let exit = match known.code() {
            "not_found" => exit::NOT_FOUND,
            "conflict" => exit::CONFLICT,
            "invalid" => exit::INVALID,
            "unavailable" => exit::UNAVAILABLE,
            _ => exit::ERROR,
        };
        Self {
            code: known.code().to_string(),
            message: known.to_string(),
            hint: known.hint().map(str::to_string),
            exit,
        }
    }
}

#[expect(clippy::struct_excessive_bools, reason = "independent output switches")]
pub struct Ctx<'a> {
    pub out: &'a mut dyn Write,
    err: &'a mut dyn Write,
    pub command: &'static str,
    pub json: bool,
    pub quiet: bool,
    pub verbose: bool,
    pub color: bool,
}

impl<'a> Ctx<'a> {
    pub fn new(out: &'a mut dyn Write, err: &'a mut dyn Write, json: bool, color: bool) -> Self {
        Self { out, err, command: "", json, quiet: false, verbose: false, color: color && !json }
    }

    pub fn emit(&mut self, output: &Output) -> io::Result<u8> {
        if self.json {
            let mut doc = Map::new();
            doc.insert("success".into(), json!(true));
            doc.insert("command".into(), json!(self.command));
            if let Value::Object(fields) = &output.data {
                doc.extend(fields.clone());
            }
            self.write_json(&Value::Object(doc))?;
        } else if !output.human.is_empty() {
            writeln!(self.out, "{}", output.human)?;
        }
        Ok(exit::OK)
    }

    pub fn fail(&mut self, f: &Failure) -> io::Result<u8> {
        if self.json {
            let mut doc = json!({ "success": false, "command": self.command, "error": f.message, "code": f.code });
            if let (Some(hint), Value::Object(map)) = (&f.hint, &mut doc) {
                map.insert("hint".into(), json!(hint));
            }
            self.write_json(&doc)?;
        } else {
            let mark = self.paint("31", "✗");
            writeln!(self.err, "{mark} {}", f.message)?;
            if let Some(hint) = &f.hint {
                let hint = self.paint("2", hint);
                writeln!(self.err, "  {hint}")?;
            }
        }
        Ok(f.exit)
    }

    /// Progress and confirmations; `--quiet` and `--json` silence them.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the first command that reports progress uses it")
    )]
    pub fn info(&mut self, message: &str) -> io::Result<()> {
        if self.quiet || self.json { Ok(()) } else { writeln!(self.err, "{message}") }
    }

    /// Diagnostics shown only with `--verbose`.
    pub fn debug(&mut self, message: &str) -> io::Result<()> {
        if !self.verbose {
            return Ok(());
        }
        let message = self.paint("2", message);
        writeln!(self.err, "{message}")
    }

    fn paint(&self, sgr: &str, text: &str) -> String {
        if self.color { format!("\x1b[{sgr}m{text}\x1b[0m") } else { text.to_string() }
    }

    fn write_json(&mut self, doc: &Value) -> io::Result<()> {
        let text = serde_json::to_string_pretty(doc).map_err(io::Error::other)?;
        writeln!(self.out, "{text}")
    }
}
