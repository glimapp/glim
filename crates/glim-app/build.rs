use std::{env, path::PathBuf, process::Command};
fn main() {
    embed_build_identity();
    embed_diagram_assets();
    println!("cargo:rerun-if-changed=../../assets/reader-limits.json");
    println!("cargo:rerun-if-changed=../../scripts/generate-reader-limits.py");
    let limits_out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let generated = Command::new("python3")
        .arg("../../scripts/generate-reader-limits.py")
        .arg("--out-dir")
        .arg(&limits_out)
        .status()
        .expect("Generate shared reader limits");
    assert!(generated.success(), "Reader limit generation failed");

    println!("cargo:rerun-if-changed=../../assets/macos/Info.plist");
    let metadata =
        std::fs::read_to_string("../../assets/macos/Info.plist").expect("Missing release metadata");
    let build = metadata
        .split("<key>CFBundleVersion</key><string>")
        .nth(1)
        .and_then(|s| s.split("</string>").next())
        .expect("Missing release build number");
    assert!(build.parse::<u64>().is_ok(), "Invalid release build number");
    println!("cargo:rustc-env=GLIM_BUILD_NUMBER={build}");
    println!("cargo:rerun-if-env-changed=GLIM_UPDATE_PUBLIC_KEY");
    assert!(
        env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos"),
        "Glim supports only macOS (Apple Silicon and Intel)"
    );
    build_native_preview();
    if env::var_os("CARGO_FEATURE_MACOS_UPDATER").is_some() {
        build_updater();
    }
}

fn embed_build_identity() {
    for name in ["GLIM_BUILD_CHANNEL", "GLIM_BUILD_TIMESTAMP"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    // Refresh direct Cargo builds when sources change, not just packaged builds.
    for path in [
        "crates",
        "native",
        "assets",
        "scripts",
        "vendor",
        "Cargo.toml",
        "Cargo.lock",
    ] {
        println!("cargo:rerun-if-changed=../../{path}");
    }
    let output = |program: &str, args: &[&str]| {
        Command::new(program)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    for name in ["HEAD", "index"] {
        if let Some(path) = output("git", &["rev-parse", "--git-path", name]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(reference) = output("git", &["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = output("git", &["rev-parse", "--git-path", &reference])
    {
        println!("cargo:rerun-if-changed={path}");
    }
    let channel = env::var("GLIM_BUILD_CHANNEL").unwrap_or_else(|_| "local".into());
    assert!(
        matches!(channel.as_str(), "local" | "release"),
        "Invalid build channel"
    );
    let timestamp = env::var("GLIM_BUILD_TIMESTAMP")
        .ok()
        .or_else(|| output("date", &["+%Y-%m-%d %H:%M:%S %z"]))
        .expect("Cannot determine build time");
    let hash = output("git", &["rev-parse", "--short=12", "HEAD"]).unwrap_or_default();
    let dirty = output(
        "git",
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .is_some_and(|status| !status.is_empty());
    println!("cargo:rustc-env=GLIM_BUILD_CHANNEL={channel}");
    println!("cargo:rustc-env=GLIM_BUILD_TIMESTAMP={timestamp}");
    println!("cargo:rustc-env=GLIM_BUILD_HASH={hash}");
    println!("cargo:rustc-env=GLIM_BUILD_DIRTY={dirty}");
}

fn build_native_preview() {
    for source in [
        "Preview.swift",
        "FormatCatalog.swift",
        "PreviewRequest.swift",
        "ImagePreview.swift",
        "ReaderResources.swift",
        "ImageTextExtraction.swift",
        "PreviewCopy.swift",
        "FloatingReaderToolbar.swift",
        "ReaderActions.swift",
        "Telemetry.swift",
        "HTMLPreview.swift",
        "WorkbookPreview.swift",
        "WordPreview.swift",
        "EmbeddedPreview.swift",
        "FinderIntegration.swift",
    ] {
        println!("cargo:rerun-if-changed=../../native/{source}");
    }
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        _ => panic!("Unsupported macOS architecture"),
    };
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut compiler = Command::new("xcrun");
    let sdk = Command::new("xcrun")
        .arg("--show-sdk-path")
        .output()
        .expect("macOS SDK path");
    let sdk = PathBuf::from(String::from_utf8_lossy(&sdk.stdout).trim());
    if sdk
        .join("System/Library/Frameworks/AppKit.framework/Headers/NSGlassEffectView.h")
        .is_file()
    {
        compiler.arg("swiftc").arg("-DGLIM_NATIVE_GLASS");
    } else {
        compiler.arg("swiftc");
    }
    let status = compiler
        .args([
            "-O",
            "-swift-version",
            "5",
            "-emit-library",
            "-static",
            "-module-name",
            "GlimNativePreview",
            "-target",
            &format!("{arch}-apple-macos11.0"),
            "../../native/Preview.swift",
            "../../native/FormatCatalog.swift",
            "../../native/PreviewRequest.swift",
            "../../native/ImagePreview.swift",
            "../../native/ReaderResources.swift",
            "../../native/ImageTextExtraction.swift",
            "../../native/PreviewCopy.swift",
            "../../native/FloatingReaderToolbar.swift",
            "../../native/ReaderActions.swift",
            "../../native/Telemetry.swift",
            "../../native/HTMLPreview.swift",
            "../../native/WorkbookPreview.swift",
            "../../native/WordPreview.swift",
            "../../native/EmbeddedPreview.swift",
            "../../native/FinderIntegration.swift",
        ])
        .arg(output.join("ReaderLimits.swift"))
        .args(["-o"])
        .arg(output.join("libGlimNativePreview.a"))
        .status()
        .expect("Native previews require Apple Command Line Tools (swiftc)");
    assert!(status.success(), "Embedded preview compilation failed");
    let compiler = Command::new("xcrun")
        .args(["--find", "swiftc"])
        .output()
        .unwrap();
    assert!(compiler.status.success(), "Cannot locate Swift runtime");
    let compiler = PathBuf::from(String::from_utf8(compiler.stdout).unwrap().trim());
    let runtime = compiler
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("lib/swift/macosx");
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-search=native={}", runtime.display());
    println!("cargo:rustc-link-search=native=/usr/lib/swift");
    println!("cargo:rustc-link-lib=static=GlimNativePreview");
    println!("cargo:rustc-link-lib=framework=FinderSync");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
}

fn build_updater() {
    println!("cargo:rerun-if-changed=../../native/Updater.swift");
    println!("cargo:rerun-if-env-changed=GLIM_SPARKLE_DIR");
    let framework = PathBuf::from(env::var_os("GLIM_SPARKLE_DIR").expect(
        "Run scripts/fetch-sparkle.sh and set GLIM_SPARKLE_DIR before enabling macos-updater",
    ));
    assert!(
        framework.join("Sparkle.framework/Sparkle").is_file(),
        "Missing Sparkle.framework"
    );
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        _ => panic!("Unsupported macOS architecture"),
    };
    let status = Command::new("xcrun")
        .args([
            "swiftc",
            "-O",
            "-swift-version",
            "5",
            "-emit-library",
            "-static",
            "-module-name",
            "GlimUpdater",
            "-target",
            &format!("{arch}-apple-macos11.0"),
            "-F",
        ])
        .arg(&framework)
        .args(["../../native/Updater.swift", "-o"])
        .arg(output.join("libGlimUpdater.a"))
        .status()
        .expect("Swift compiler is required");
    assert!(status.success(), "Updater compilation failed");
    println!("cargo:rustc-link-search=framework={}", framework.display());
    println!("cargo:rustc-link-lib=framework=Sparkle");
    println!("cargo:rustc-link-lib=static=GlimUpdater");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../../../Frameworks");
    // Unbundled test executables find the same pinned framework without installing it.
    if env::var("PROFILE").as_deref() != Ok("release") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", framework.display());
    }
}

// Dynamic engine imports are still served entirely from the signed executable.
fn embed_diagram_assets() {
    let root = std::fs::canonicalize("../../assets/markdown").expect("diagram assets");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut pending = vec![root.clone()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).expect("diagram asset directory") {
            let path = entry.expect("diagram asset entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|ext| ext == "js" || ext == "html" || ext == "wasm")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    let entries = files
        .iter()
        .map(|path| {
            let route = format!("/{}", path.strip_prefix(&root).unwrap().to_string_lossy());
            let mime = match path.extension().unwrap().to_str().unwrap() {
                "html" => "text/html; charset=utf-8",
                "wasm" => "application/wasm",
                _ => "text/javascript; charset=utf-8",
            };
            format!(
                "({route:?}, {mime:?}, include_bytes!({:?}))",
                path.to_str().unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    std::fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("diagram_assets.rs"),
        format!("const DIAGRAM_ASSETS: &[(&str, &str, &[u8])] = &[{entries}];"),
    )
    .expect("embed diagram engine assets");
}
