//! check:layers: layer boundaries, by text search. Each rule in
//! `budgets/layer-rules.json` names files under `crates/` and the tokens they
//! may not hold. A rule that scans no file fails, and so does an allow entry
//! that excuses nothing, so neither can outlive the code it describes.

use std::path::Path;

use serde_json::Value;

use crate::files;

const RULES: &str = "budgets/layer-rules.json";

/// One boundary: the `.rs` files under `paths`, minus those under `except`,
/// hold none of the `forbid` tokens outside comment lines.
pub struct Rule {
    pub name: String,
    pub why: String,
    pub paths: Vec<String>,
    pub except: Vec<String>,
    pub forbid: Vec<String>,
}

/// One file exempt from one rule.
pub struct Allow {
    pub rule: String,
    pub path: String,
}

pub fn check(root: &Path) -> Result<String, Vec<String>> {
    let (rules, allows) = parse(&files::read_json(root, RULES)?)?;
    let mut sources = Vec::new();
    for file in files::under(root, &["crates"], &["rs"]) {
        let text = files::read(root, &file)?;
        sources.push((file, text));
    }
    let errors = evaluate(&rules, &allows, &sources);
    let summary = format!("check:layers: {} rules ok", rules.len());
    if errors.is_empty() { Ok(summary) } else { Err(errors) }
}

fn parse(config: &Value) -> Result<(Vec<Rule>, Vec<Allow>), Vec<String>> {
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let list = |v: &Value| v.as_array().into_iter().flatten().map(text).collect::<Vec<_>>();
    let rules: Vec<Rule> = config["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| Rule {
            name: text(&r["name"]),
            why: text(&r["why"]),
            paths: list(&r["paths"]),
            except: list(&r["except"]),
            forbid: list(&r["forbid"]),
        })
        .collect();
    let allows: Vec<Allow> = config["allow"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| !text(&a["why"]).is_empty())
        .map(|a| Allow { rule: text(&a["rule"]), path: text(&a["path"]) })
        .collect();
    let mut errors = Vec::new();
    for r in &rules {
        if r.name.is_empty() || r.why.is_empty() || r.paths.is_empty() || r.forbid.is_empty() {
            errors.push(format!("{RULES}: rule '{}' needs name, why, paths and forbid", r.name));
        }
    }
    if allows.len() != config["allow"].as_array().map_or(0, Vec::len) {
        errors.push(format!("{RULES}: every allow entry needs rule, path and a non-empty why"));
    }
    if errors.is_empty() { Ok((rules, allows)) } else { Err(errors) }
}

/// Checks every rule against `sources` (`(path, text)` pairs) and returns the errors.
pub fn evaluate(rules: &[Rule], allows: &[Allow], sources: &[(String, String)]) -> Vec<String> {
    let mut errors = Vec::new();
    let mut used = Vec::new();
    for rule in rules {
        let within = |file: &str, dirs: &[String]| dirs.iter().any(|d| is_under(file, d));
        let scoped: Vec<&(String, String)> = sources
            .iter()
            .filter(|(f, _)| within(f, &rule.paths) && !within(f, &rule.except))
            .collect();
        if scoped.is_empty() {
            errors.push(format!(
                "layer rule {} in {RULES} scans no file; fix its paths or delete it",
                rule.name
            ));
        }
        for (file, text) in scoped {
            let hits = violations(text, &rule.forbid);
            if hits.is_empty() {
                continue;
            }
            if let Some(i) = allows.iter().position(|a| a.rule == rule.name && a.path == *file) {
                used.push(i);
                continue;
            }
            errors.extend(hits.iter().map(|(n, line)| {
                format!("{file}:{n}: {line}  [layer rule {}: {}]", rule.name, rule.why)
            }));
        }
    }
    for (i, a) in allows.iter().enumerate().filter(|(i, _)| !used.contains(i)) {
        errors.push(format!(
            "stale allow entry {i} (rule {}, path {}) in {RULES} matches no violation; delete it",
            a.rule, a.path
        ));
    }
    errors
}

fn is_under(file: &str, path: &str) -> bool {
    let path = path.trim_end_matches('/');
    file == path || file.strip_prefix(path).is_some_and(|rest| rest.starts_with('/'))
}

/// The non-comment lines of `text` that hold a forbidden token, as `(line number, line)`.
fn violations<'a>(text: &'a str, forbid: &[String]) -> Vec<(usize, &'a str)> {
    text.lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.trim()))
        .filter(|(_, line)| !line.starts_with("//"))
        .filter(|(_, line)| forbid.iter().any(|t| has_token(line, t)))
        .collect()
}

/// `token` as a whole word (`unsafe` is not in `unsafe_code`). A trailing `*`
/// matches any word that starts with the token (`objc2*` is in `objc2_app_kit`).
pub fn has_token(line: &str, token: &str) -> bool {
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let (token, prefix) = token.strip_suffix('*').map_or((token, false), |t| (t, true));
    line.match_indices(token).any(|(i, _)| {
        let before = line[..i].chars().next_back();
        let after = line[i + token.len()..].chars().next();
        !before.is_some_and(is_word) && (prefix || !after.is_some_and(is_word))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: [String; 0] = [];

    fn rule() -> Rule {
        Rule {
            name: "core-is-pure".into(),
            why: "plain logic".into(),
            paths: vec!["crates/core".into()],
            except: vec!["crates/core/src/ffi".into()],
            forbid: vec!["unsafe".into(), "objc2*".into()],
        }
    }

    fn sources(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries.iter().map(|(f, t)| ((*f).to_string(), (*t).to_string())).collect()
    }

    #[test]
    fn matches_whole_words_and_prefixes() {
        assert!(has_token("unsafe { x() }", "unsafe"));
        assert!(!has_token("#![deny(unsafe_code)]", "unsafe"));
        assert!(!has_token("let not_unsafe = 1;", "unsafe"));
        assert!(has_token("use objc2_app_kit::NSWindow;", "objc2*"));
        assert!(!has_token("use myobjc2::X;", "objc2*"));
        assert!(has_token("use std::process::Command;", "std::process"));
    }

    #[test]
    fn reports_a_violation_with_the_rule_and_its_reason() {
        let src = sources(&[("crates/core/src/lib.rs", "fn f() {}\nunsafe fn g() {}\n")]);
        let errors = evaluate(&[rule()], &[], &src);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("crates/core/src/lib.rs:2: unsafe fn g() {}"));
        assert!(errors[0].ends_with("[layer rule core-is-pure: plain logic]"));
    }

    #[test]
    fn skips_comments_excepted_paths_and_other_crates() {
        let src = sources(&[
            ("crates/core/src/lib.rs", "// unsafe is not used here\n/// objc2 neither\n"),
            ("crates/core/src/ffi/mod.rs", "unsafe fn g() {}\n"),
            ("crates/core-extras/src/lib.rs", "unsafe fn g() {}\n"),
        ]);
        assert_eq!(evaluate(&[rule()], &[], &src), NONE);
    }

    #[test]
    fn an_allow_entry_exempts_one_file_from_one_rule() {
        let src = sources(&[("crates/core/src/lib.rs", "unsafe fn g() {}\n")]);
        let allow = |rule: &str| Allow { rule: rule.into(), path: "crates/core/src/lib.rs".into() };
        assert_eq!(evaluate(&[rule()], &[allow("core-is-pure")], &src), NONE);
        assert_eq!(evaluate(&[rule()], &[allow("other-rule")], &src).len(), 2);
    }

    #[test]
    fn fails_a_stale_allow_entry() {
        let src = sources(&[("crates/core/src/lib.rs", "fn f() {}\n")]);
        let allow = Allow { rule: "core-is-pure".into(), path: "crates/core/src/lib.rs".into() };
        let errors = evaluate(&[rule()], &[allow], &src);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("stale allow entry"));
    }

    #[test]
    fn fails_a_rule_that_scans_no_file() {
        let errors = evaluate(&[rule()], &[], &sources(&[("crates/cli/src/main.rs", "")]));
        assert!(errors[0].contains("scans no file"));
    }

    #[test]
    fn rejects_incomplete_rules_and_allow_entries() {
        let config = serde_json::json!({
            "rules": [{ "name": "r", "paths": ["crates"], "forbid": ["x"] }],
            "allow": [{ "rule": "r", "path": "crates/a.rs" }],
        });
        assert_eq!(parse(&config).err().map(|e| e.len()), Some(2));
    }
}
