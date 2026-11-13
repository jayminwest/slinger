//! The file of session ids, `sessions.json` in the data directory: the id
//! each conversation last had, so the next start resumes it, even after a
//! restart. An agent keeps one under its own name, and one per profile it
//! ran under, under `<agent>#<profile>`.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use super::{ProcessError, Result};

/// The key of a conversation in the file of ids: the agent's name, or
/// `<agent>#<profile>` for one of its profiles.
pub fn key(agent: &str, profile: Option<&str>) -> String {
    profile.map_or_else(|| agent.to_string(), |p| format!("{agent}#{p}"))
}

/// The file of session ids: `{key: id}` as JSON, keyed by [`key`].
#[derive(Debug)]
pub struct Ids {
    path: PathBuf,
    /// Held across a read and its write, so two writers do not lose one.
    lock: Mutex<()>,
}

impl Ids {
    /// The file at `path`; none there holds no ids.
    pub fn new(path: PathBuf) -> Self {
        Self { path, lock: Mutex::new(()) }
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(self.load()?.remove(key))
    }

    /// The id of `agent`'s conversation under no profile, else of the
    /// first of its profiles' in the file: one it had, if any.
    pub fn any_of(&self, agent: &str) -> Result<Option<String>> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut all = self.load()?;
        let prefix = key(agent, Some(""));
        Ok(all
            .remove(agent)
            .or_else(|| all.into_iter().find_map(|(k, id)| k.starts_with(&prefix).then_some(id))))
    }

    pub fn set(&self, key: &str, id: &str) -> Result<()> {
        self.change(|all| {
            all.insert(key.to_string(), id.to_string());
        })
    }

    /// Drops `key`'s id: its next start begins a new session.
    pub fn forget(&self, key: &str) -> Result<()> {
        self.change(|all| {
            all.remove(key);
        })
    }

    fn change(&self, edit: impl FnOnce(&mut BTreeMap<String, String>)) -> Result<()> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut all = self.load()?;
        edit(&mut all);
        let text = serde_json::to_string_pretty(&all).map_err(|e| self.failed(e.into()))?;
        let next = self.path.with_extension("json.next");
        std::fs::write(&next, text + "\n")
            .and_then(|()| std::fs::rename(&next, &self.path))
            .map_err(|source| self.failed(source))
    }

    fn load(&self) -> Result<BTreeMap<String, String>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| self.failed(e.into())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(self.failed(e)),
        }
    }

    pub fn failed(&self, source: io::Error) -> ProcessError {
        ProcessError::Ids { path: self.path.clone(), source }
    }
}
