//! What `scripts/bundle.sh` builds slinger.app from: the Info.plist it fills
//! in and the icon it copies. The script itself is not part of the gates; it
//! verifies the bundle it makes (`codesign --verify --strict`).

/// The plist the script fills in: `@VERSION@` and `@BUILD@` are its slots.
const PLIST: &str = include_str!("../../Info.plist");

/// The `<string>` value of `key` in [`PLIST`].
fn value(key: &str) -> Option<&'static str> {
    let after = PLIST.split_once(&format!("<key>{key}</key>"))?.1;
    let rest = after.trim_start().strip_prefix("<string>")?;
    Some(rest.split_once("</string>")?.0)
}

#[test]
fn the_plist_names_the_app_program_and_leaves_the_version_and_build_to_the_script() {
    assert_eq!(value("CFBundleExecutable"), Some("slinger-app"));
    assert_eq!(value("CFBundleShortVersionString"), Some("@VERSION@"));
    assert_eq!(value("CFBundleVersion"), Some("@BUILD@"));
}

#[test]
fn the_plist_names_the_icon_the_script_copies() {
    assert_eq!(value("CFBundleIconFile"), Some("slinger"));
}

/// `crates/app/slinger.icns`, which `scripts/icon.sh` makes from its source.
#[test]
fn the_committed_icon_is_an_icns_file_with_images_in_it() {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("slinger.icns");
    let bytes = std::fs::read(&icon).unwrap();
    assert!(bytes.starts_with(b"icns"), "{} is not an icns file", icon.display());
    let length = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
    assert_eq!(usize::try_from(length).unwrap(), bytes.len(), "the header's length is the file's");
    assert!(bytes.len() > 1024, "an icns file with no images: {} bytes", bytes.len());
}
