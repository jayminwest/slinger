//! The profile verbs: a workspace agent's named limits, their requests,
//! reply and client methods (docs/service.md, "Profiles").
//!
//! [`crate::Request`] reads them as one variant, [`ProfileAsk`], and does
//! not write them: a binary that never sends one (the app) carries no code
//! for them.

use serde::{Deserialize, Serialize};

use crate::{Client, RemoteError};

/// A profile request, as a client writes it and the daemon reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ProfileAsk {
    /// The profiles of a workspace's agent.
    ProfileList { workspace: String },
    /// Add a profile, or replace the one of its name whole.
    ProfileSet {
        workspace: String,
        #[serde(flatten)]
        profile: Profile,
    },
    /// Remove a profile.
    ProfileRemove { workspace: String, name: String },
}

/// One profile: a working directory and the permission rules a session
/// under it may use.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    /// The session's working directory on slingerd's machine; absent: the
    /// agent's home.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// Claude Code permission rules, beyond the `slinger` CLI.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

/// The reply to every profile verb: the workspace's profiles after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profiles {
    pub workspace: String,
    pub profiles: Vec<Profile>,
}

impl Client {
    /// The profiles of `workspace`'s agent, in its file's order.
    ///
    /// # Errors
    ///
    /// `not_found` for a workspace that is not open, and `unavailable` when
    /// the daemon does not answer.
    pub fn profile_list(&self, workspace: &str) -> Result<Profiles, RemoteError> {
        self.call(&ProfileAsk::ProfileList { workspace: workspace.into() })
    }

    /// Adds `profile` to `workspace`'s agent, or replaces the one of its
    /// name, in the workspace's file.
    ///
    /// # Errors
    ///
    /// `invalid` for a name, a rule or a directory the daemon refuses, or a
    /// workspace with no agent; `not_found` for a directory or a workspace
    /// that is not there; and `unavailable` when the daemon does not answer.
    pub fn profile_set(&self, workspace: &str, profile: Profile) -> Result<Profiles, RemoteError> {
        self.call(&ProfileAsk::ProfileSet { workspace: workspace.into(), profile })
    }

    /// Removes the profile `name` from `workspace`'s agent.
    ///
    /// # Errors
    ///
    /// `not_found` for a profile or a workspace that is not there, and
    /// `unavailable` when the daemon does not answer.
    pub fn profile_remove(&self, workspace: &str, name: &str) -> Result<Profiles, RemoteError> {
        self.call(&ProfileAsk::ProfileRemove { workspace: workspace.into(), name: name.into() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    #[test]
    fn a_profile_request_reads_back_as_itself_by_its_own_op() {
        let dev = Profile {
            name: "dev".into(),
            dir: Some("/src".into()),
            tools: vec!["Edit".into(), "Bash(git *)".into()],
        };
        for (ask, line) in [
            (
                ProfileAsk::ProfileList { workspace: "w".into() },
                r#"{"v":1,"op":"profile_list","workspace":"w"}"#,
            ),
            (
                ProfileAsk::ProfileSet { workspace: "w".into(), profile: dev },
                r#"{"v":1,"op":"profile_set","workspace":"w","name":"dev","dir":"/src","tools":["Edit","Bash(git *)"]}"#,
            ),
            (
                ProfileAsk::ProfileSet {
                    workspace: "w".into(),
                    profile: Profile { name: "look".into(), ..Profile::default() },
                },
                r#"{"v":1,"op":"profile_set","workspace":"w","name":"look"}"#,
            ),
            (
                ProfileAsk::ProfileRemove { workspace: "w".into(), name: "dev".into() },
                r#"{"v":1,"op":"profile_remove","workspace":"w","name":"dev"}"#,
            ),
        ] {
            assert_eq!(request_line(&ask).unwrap(), format!("{line}\n"));
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(Request::Profile(ask)), "{line}");
        }
    }

    #[test]
    fn a_profile_request_missing_a_field_is_a_bad_request() {
        for line in [
            r#"{"v":1,"op":"profile_list"}"#,
            r#"{"v":1,"op":"profile_set","workspace":"w"}"#,
            r#"{"v":1,"op":"profile_set","workspace":"w","name":"d","tools":"Edit"}"#,
            r#"{"v":1,"op":"profile_remove","workspace":"w"}"#,
        ] {
            let err = crate::read_request(&mut line.as_bytes()).unwrap_err();
            assert_eq!(err.code(), "bad_request", "{line}");
        }
    }
}
