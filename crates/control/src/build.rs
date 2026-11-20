//! The build every binary carries (`build.rs` stamps it): the daemon, the
//! CLI and the app link this crate, so they agree on how a build is named
//! and a client can compare its own build with the daemon's.
//!
//! The stamp names a commit, not the bytes: two builds of the same commit
//! with different uncommitted changes carry the same stamp (`<sha>-dirty`).

/// This package's version, as `--version` prints it. Not the protocol's
/// [`crate::VERSION`].
pub const PACKAGE: &str = env!("CARGO_PKG_VERSION");

/// The build this binary was made from: the commit in lowercase, `-dirty`
/// after it when tracked files differed, and `dev` with no commit.
#[must_use]
pub fn stamp() -> String {
    text(option_env!("SLINGER_BUILD_SHA"), option_env!("SLINGER_BUILD_DIRTY"))
}

/// The stamp for a commit `sha` and a `dirty` flag (`1` for a build with
/// uncommitted changes): the sha in lowercase, `-dirty` after it when dirty,
/// and `dev` when `sha` is not one.
fn text(sha: Option<&str>, dirty: Option<&str>) -> String {
    let sha = sha.map(str::trim);
    let sha =
        sha.filter(|s| (7..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit()));
    match sha {
        Some(sha) if dirty.is_some_and(|d| d.trim() == "1") => {
            format!("{}-dirty", sha.to_lowercase())
        }
        Some(sha) => sha.to_lowercase(),
        None => "dev".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stamp_is_the_sha_dirty_or_dev() {
        let sha = "0123456789ABCDEF0123456789abcdef01234567";
        assert_eq!(text(Some(sha), Some("0")), sha.to_lowercase());
        assert_eq!(text(Some(sha), None), sha.to_lowercase());
        assert_eq!(text(Some(sha), Some("1")), format!("{}-dirty", sha.to_lowercase()));
        assert_eq!(text(Some("unknown"), Some("1")), "dev");
        assert_eq!(text(Some("abc"), None), "dev");
        assert_eq!(text(None, None), "dev");
    }

    #[test]
    fn this_build_has_a_stamp_and_the_package_version() {
        let stamp = stamp();
        assert!(stamp == "dev" || stamp.trim_end_matches("-dirty").len() >= 7, "{stamp}");
        assert_eq!(PACKAGE, env!("CARGO_PKG_VERSION"));
    }
}
