fn main() {
    println!("cargo:rerun-if-changed=../../assets/macos/Info.plist");
    let metadata = std::fs::read_to_string("../../assets/macos/Info.plist")
        .expect("Missing application version metadata");
    let version = metadata
        .split("<key>CFBundleShortVersionString</key><string>")
        .nth(1)
        .and_then(|s| s.split("</string>").next())
        .expect("Missing application version");
    let parts = version.split('.').collect::<Vec<_>>();
    assert!(
        matches!(parts.len(), 3 | 4) && parts.iter().all(|part| part.parse::<u32>().is_ok()),
        "Expected a three- or four-part application version"
    );
    let mut cargo_version = parts[..3].join(".");
    if let Some(hotfix) = parts.get(3) {
        cargo_version.push_str(&format!("+hotfix.{hotfix}"));
    }
    assert_eq!(cargo_version, env!("CARGO_PKG_VERSION"));
    println!("cargo:rustc-env=GLIM_VERSION={version}");
}
