//! Agent profiles (slinger-c8359a): named limits a workspace's agent can run
//! under, kept in the workspace's file as `[[agent.profile]]`
//! (docs/workspace.md, "Profiles").
//!
//! A profile names a working directory and the Claude Code permission rules
//! its session may use. It never changes a session by itself: a session runs
//! under a profile only when a start names it. Its rules take the place of
//! the agent's `tools` and of the rules that write files (the `slinger` CLI
//! stays), so a profile with no rules is one that cannot write. Each rule, a profile's or
//! the agent's own, must name a tool this module knows or an MCP tool
//! ([`check_rule`]). A rule it cannot read is refused, never passed on.

use serde::{Deserialize, Serialize};

use super::Workspace;
use crate::{Error, Result};

/// The longest profile name.
const NAME_MAX: usize = 64;

/// The Claude Code tools a profile's rule may name.
pub const TOOLS: [&str; 10] = [
    "Bash",
    "Edit",
    "Glob",
    "Grep",
    "MultiEdit",
    "NotebookEdit",
    "Read",
    "WebFetch",
    "WebSearch",
    "Write",
];

/// One profile of a workspace's agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Its name: `a-z`, `0-9`, `-` and `_`, at most 64 long.
    pub name: String,
    /// The session's working directory on slingerd's machine, absolute.
    /// `None`: the agent's home.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// The permission rules its session may use without asking, beyond the
    /// `slinger` CLI: `Edit`, `Bash(git *)`. They replace the agent's `tools`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    #[serde(flatten)]
    extra: toml::Table,
}

impl Profile {
    /// A profile, checked.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] for a name, a directory or a rule [`Profile::check`]
    /// refuses.
    pub fn new(name: impl Into<String>, dir: Option<String>, tools: Vec<String>) -> Result<Self> {
        let profile = Self { name: name.into(), dir, tools, extra: toml::Table::new() };
        profile.check()?;
        Ok(profile)
    }

    /// Checks the profile: a name of `a-z`, `0-9`, `-` and `_` (at most 64),
    /// an absolute directory, and rules [`check_rule`] reads.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`], with a hint, for the first thing it refuses.
    pub fn check(&self) -> Result<()> {
        let name = &self.name;
        let good = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
        if name.is_empty() || name.len() > NAME_MAX || !name.chars().all(good) {
            return Err(Error::Invalid {
                message: format!("{name:?} is not a profile name"),
                hint: Some("use a-z, 0-9, - and _, at most 64 long: dev, read-only".into()),
            });
        }
        if let Some(dir) = &self.dir
            && !dir.starts_with('/')
        {
            return Err(Error::Invalid {
                message: format!("profile {name:?}: dir {dir:?} is not an absolute path"),
                hint: Some("pass the full path from /, on slingerd's machine".into()),
            });
        }
        for rule in &self.tools {
            check_rule(rule).map_err(|e| Error::Invalid {
                message: format!("profile {name:?}: {e}"),
                hint: e.hint().map(str::to_string),
            })?;
        }
        Ok(())
    }
}

/// Checks one permission rule: a tool of [`TOOLS`], alone (`Edit`) or with
/// a pattern in parentheses (`Bash(git *)`, `Bash(git:*)`, `Edit(src/**)`),
/// or an MCP rule: `mcp__<server>`, `mcp__<server>__<tool>` or
/// `mcp__<server>__*`, where a server and a tool are letters, digits, `-`
/// and `_`.
///
/// # Errors
///
/// [`Error::Invalid`] for a blank rule, a tool not in [`TOOLS`], a pattern
/// that is blank or not closed, or an MCP rule that is not of that shape.
pub fn check_rule(rule: &str) -> Result<()> {
    let known = match rule.split_once('(') {
        _ if rule.starts_with(MCP) => is_mcp(rule),
        Some((tool, rest)) => rest.strip_suffix(')').is_some_and(|p| {
            TOOLS.contains(&tool) && !p.trim().is_empty() && !p.contains(['\n', '\r'])
        }),
        None => TOOLS.contains(&rule),
    };
    if !known {
        let tool = rule.split_once('(').map_or(rule, |(tool, _)| tool);
        let meant = TOOLS.iter().find(|t| t.eq_ignore_ascii_case(tool.trim()) && **t != tool);
        let meant = meant.map_or_else(String::new, |t| format!("did you mean {t:?}? "));
        return Err(Error::Invalid {
            message: format!("{rule:?} is not a permission rule slinger knows"),
            hint: Some(format!(
                "{meant}a rule is one of: Tool, Tool(pattern), mcp__<server>, \
                 mcp__<server>__<tool>, mcp__<server>__* (e.g. Edit, Bash(git *), mcp__github); \
                 Tool is one of {}",
                TOOLS.join(", ")
            )),
        });
    }
    Ok(())
}

/// The prefix of an MCP rule.
const MCP: &str = "mcp__";

/// Whether `rule` is an MCP rule [`check_rule`] takes.
fn is_mcp(rule: &str) -> bool {
    let Some(rest) = rule.strip_prefix(MCP) else { return false };
    let word = |w: &str| {
        !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    match rest.split_once("__") {
        Some((server, tool)) => word(server) && (tool == "*" || word(tool)),
        None => word(rest),
    }
}

/// Whether a rule names a tool of [`TOOLS`]: one a session under a profile
/// must be given (`--tools`) for the rule to mean anything. An MCP rule
/// does not.
#[must_use]
pub fn builtin(rule: &str) -> Option<&str> {
    let tool = rule.split_once('(').map_or(rule, |(tool, _)| tool);
    TOOLS.contains(&tool).then_some(tool)
}

/// Checks a list of profiles: each one, and no name twice.
pub(super) fn check_all(profiles: &[Profile]) -> Result<()> {
    for (n, profile) in profiles.iter().enumerate() {
        profile.check()?;
        if profiles[..n].iter().any(|p| p.name == profile.name) {
            return Err(Error::Invalid {
                message: format!("two profiles are named {:?}", profile.name),
                hint: Some("give one another name, or remove it".into()),
            });
        }
    }
    Ok(())
}

impl Workspace {
    /// The profiles of the workspace's agent, in the file's order. None for
    /// a workspace with no agent.
    #[must_use]
    pub fn profiles(&self) -> &[Profile] {
        self.agent.as_ref().map(|a| a.profiles.as_slice()).unwrap_or_default()
    }

    /// The profile `name` names.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] when the agent has no such profile, with a hint
    /// that lists the ones it has.
    pub fn profile(&self, name: &str) -> Result<&Profile> {
        self.profiles().iter().find(|p| p.name == name).ok_or_else(|| {
            let names: Vec<&str> = self.profiles().iter().map(|p| p.name.as_str()).collect();
            let hint = if names.is_empty() {
                format!("add one with 'slinger profile set {} {name}'", self.name)
            } else {
                format!("its profiles are {}", names.join(", "))
            };
            Error::NotFound {
                what: format!("profile {name:?} of workspace {:?}", self.name),
                hint: Some(hint),
            }
        })
    }

    /// Adds `profile` to the agent's profiles, or replaces the one of its
    /// name in place. Returns whether it is new.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] for a workspace with no agent.
    pub fn set_profile(&mut self, profile: Profile) -> Result<bool> {
        let name = self.name.clone();
        let agent = self.agent.as_mut().ok_or_else(|| Error::Invalid {
            message: format!("workspace {name:?} has no agent to give a profile"),
            hint: Some("name one under [agent] in its slinger-workspace.toml".into()),
        })?;
        if let Some(kept) = agent.profiles.iter_mut().find(|p| p.name == profile.name) {
            *kept = profile;
            return Ok(false);
        }
        agent.profiles.push(profile);
        Ok(true)
    }

    /// Removes the profile `name` names, and returns it.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] as [`Workspace::profile`].
    pub fn remove_profile(&mut self, name: &str) -> Result<Profile> {
        self.profile(name)?;
        let agent = self
            .agent
            .as_mut()
            .ok_or_else(|| Error::NotFound { what: format!("profile {name:?}"), hint: None })?;
        let at = agent.profiles.iter().position(|p| p.name == name).unwrap_or_default();
        Ok(agent.profiles.remove(at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(text: &str) -> Result<Workspace> {
        Workspace::parse(&format!("version = 1\nname = \"w\"\n[agent]\nname = \"a\"\n{text}"))
    }

    #[test]
    fn a_rule_names_a_known_tool_alone_or_with_a_pattern() {
        for rule in [
            "Edit",
            "Bash(git *)",
            "Bash(git:*)",
            "Edit(src/**)",
            "WebFetch(domain:example.com)",
            "mcp__github",
            "mcp__github__get_issue",
            "mcp__my-server__*",
        ] {
            assert!(check_rule(rule).is_ok(), "{rule}");
        }
        for rule in [
            "",
            " ",
            "edit",
            "Shell",
            "Shell(ls)",
            "Bash()",
            "Bash( )",
            "Bash(git",
            " Edit",
            "mcp__",
            "mcp____tool",
            "mcp__x__",
            "mcp__x(y)",
            "mcp__x__y z",
        ] {
            let err = check_rule(rule).unwrap_err();
            assert_eq!(err.code(), "invalid", "{rule:?}");
            let hint = err.hint().unwrap();
            assert!(hint.contains("Bash, Edit"), "{rule:?}");
            let forms =
                "Tool, Tool(pattern), mcp__<server>, mcp__<server>__<tool>, mcp__<server>__*";
            assert!(hint.contains(forms), "{hint}");
        }
        let near = |rule| check_rule(rule).unwrap_err().hint().unwrap().to_string();
        assert!(near("bash(ls)").starts_with("did you mean \"Bash\"?"), "{}", near("bash(ls)"));
        assert!(near(" Edit").starts_with("did you mean \"Edit\"?"));
        assert!(!near("Shell").contains("did you mean"));
    }

    #[test]
    fn a_rule_names_the_builtin_tool_it_needs() {
        assert_eq!(builtin("Edit(src/**)"), Some("Edit"));
        assert_eq!(builtin("WebFetch"), Some("WebFetch"));
        assert_eq!(builtin("mcp__github__get_issue"), None);
    }

    #[test]
    fn a_profile_needs_a_plain_name_and_an_absolute_dir() {
        assert!(Profile::new("dev-1_a", Some("/src".into()), vec!["Edit".into()]).is_ok());
        for (name, dir) in
            [("", None), ("Dev", None), ("a b", None), ("a#b", None), ("dev", Some("src"))]
        {
            let err = Profile::new(name, dir.map(str::to_string), Vec::new()).unwrap_err();
            assert_eq!(err.code(), "invalid", "{name:?}");
            assert!(err.hint().is_some());
        }
        let long = "a".repeat(65);
        assert_eq!(Profile::new(long, None, Vec::new()).unwrap_err().code(), "invalid");
        let err = Profile::new("dev", None, vec!["Sudo".into()]).unwrap_err();
        assert!(err.to_string().contains("profile \"dev\""), "{err}");
    }

    #[test]
    fn profiles_read_and_write_back_under_the_agent_with_unknown_fields_kept() {
        let ws = workspace(
            "[[agent.profile]]\nname = \"dev\"\ndir = \"/src\"\ntools = [\"Edit\"]\nnew = 1\n\
             [[agent.profile]]\nname = \"look\"\n",
        )
        .unwrap();
        let names: Vec<&str> = ws.profiles().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["dev", "look"]);
        assert_eq!(ws.profile("dev").unwrap().dir.as_deref(), Some("/src"));
        let text = ws.to_toml();
        assert!(text.contains("[[agent.profile]]") && text.contains("new = 1"), "{text}");
        assert_eq!(Workspace::parse(&text).unwrap(), ws);
    }

    #[test]
    fn a_file_with_a_bad_profile_is_invalid() {
        for bad in [
            "[[agent.profile]]\nname = \"dev\"\ntools = [\"Shell\"]\n",
            "[[agent.profile]]\nname = \"dev\"\ntools = [\"\"]\n",
            "[[agent.profile]]\nname = \"dev\"\ndir = \"rel\"\n",
            "[[agent.profile]]\nname = \"dev\"\n[[agent.profile]]\nname = \"dev\"\n",
        ] {
            assert_eq!(workspace(bad).unwrap_err().code(), "invalid", "{bad}");
        }
    }

    #[test]
    fn a_profile_is_set_in_place_found_and_removed() {
        let mut ws = workspace("").unwrap();
        let err = ws.profile("dev").unwrap_err();
        assert_eq!(err.code(), "not_found");
        assert!(err.hint().unwrap().contains("slinger profile set w dev"));
        assert!(ws.set_profile(Profile::new("dev", None, vec!["Edit".into()]).unwrap()).unwrap());
        assert!(ws.set_profile(Profile::new("look", None, Vec::new()).unwrap()).unwrap());
        let wider = Profile::new("dev", Some("/src".into()), vec!["Write".into()]).unwrap();
        assert!(!ws.set_profile(wider.clone()).unwrap(), "replaced");
        assert_eq!(ws.profiles(), [wider.clone(), Profile::new("look", None, vec![]).unwrap()]);
        assert!(ws.profile("nope").unwrap_err().hint().unwrap().contains("dev, look"));
        assert_eq!(ws.remove_profile("dev").unwrap(), wider);
        assert_eq!(ws.remove_profile("dev").unwrap_err().code(), "not_found");

        let mut none = Workspace::named("bare");
        let err = none.set_profile(wider).unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(err.hint().unwrap().contains("[agent]"));
        assert_eq!(none.profiles(), []);
    }
}
