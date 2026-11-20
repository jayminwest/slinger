//! Keeps a real record out of temporary directories (slinger-491a02). The
//! system empties those at a restart and after some days, so a data
//! directory there loses every canvas. `--scratch` says the loss is fine: a
//! test's daemon, or one started to try something.

use std::path::{Path, PathBuf};

/// Places the system empties on its own, beside [`std::env::temp_dir`].
const ROOTS: [&str; 5] = ["/tmp", "/var/tmp", "/var/folders", "/dev/shm", "/run"];

/// The temporary directory that holds `dir`, if one does. `dir` need not
/// exist. A link into a temporary directory counts.
#[must_use]
pub fn temporary_root(dir: &Path) -> Option<PathBuf> {
    under(dir, ROOTS.iter().map(PathBuf::from).chain([std::env::temp_dir()]))
}

/// The first of `roots` that holds `dir`, as written or with links resolved.
fn under(dir: &Path, roots: impl Iterator<Item = PathBuf>) -> Option<PathBuf> {
    let dirs = [resolved(dir), dir.to_path_buf()];
    roots
        .flat_map(|root| [resolved(&root), root])
        .find(|root| dirs.iter().any(|d| d.starts_with(root)))
}

/// `path` with its links resolved as far as it exists; the rest as written.
fn resolved(path: &Path) -> PathBuf {
    for ancestor in path.ancestors() {
        if let (Ok(real), Ok(rest)) = (ancestor.canonicalize(), path.strip_prefix(ancestor)) {
            return real.join(rest);
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(all: &[&Path]) -> impl Iterator<Item = PathBuf> {
        all.iter().map(|p| p.to_path_buf()).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn a_directory_the_system_empties_is_temporary_even_before_it_exists() {
        let temp = tempfile::tempdir().unwrap();
        assert!(temporary_root(&temp.path().join("not/made/yet")).is_some());
        assert!(temporary_root(Path::new("/tmp/slg/data")).is_some());
    }

    #[test]
    fn a_directory_that_lasts_is_not() {
        let tmp = Path::new("/tmp");
        assert_eq!(under(Path::new("/home/me/.local/share/slinger"), roots(&[tmp])), None);
        // A name that only starts like a root is another directory.
        assert_eq!(under(Path::new("/tmpfiles/slinger"), roots(&[tmp])), None);
    }

    #[test]
    fn a_link_into_a_temporary_directory_is_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let (real, link) = (dir.path().join("real"), dir.path().join("link"));
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(under(&link.join("data"), roots(&[&real])).is_some());
    }

    #[test]
    fn slingerd_refuses_a_temporary_data_dir_and_makes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, data) = (dir.path().join("s.sock"), dir.path().join("data"));
        let args = ["slingerd", "--socket", socket.to_str().unwrap(), "--data-dir"];
        let args = args.into_iter().chain([data.to_str().unwrap()]);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(crate::run(args, &mut out, &mut err), 2);
        let err = String::from_utf8(err).unwrap();
        assert!(err.starts_with("✗ data directory "), "{err}");
        assert!(
            err.contains(
                ", which the system empties\n  pass a --data-dir that lasts, or --scratch"
            ),
            "{err}"
        );
        assert!(!data.exists() && !socket.exists());
    }
}
