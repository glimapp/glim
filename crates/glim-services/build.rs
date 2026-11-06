use std::{env, path::PathBuf, process::Command};
#[path = "build/distribution.rs"]
mod distribution;

fn main() {
    println!("cargo:rerun-if-changed=build/distribution.rs");
    for key in distribution::INPUTS {
        println!("cargo:rerun-if-env-changed={key}");
    }
    println!(
        "cargo:rustc-env=GLIM_TELEMETRY_ENVIRONMENT={}",
        distribution::environment(|key| env::var(key).ok())
    );
    diagram_cache_fingerprint();
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

    println!("cargo:rerun-if-changed=../../assets/locales");
    println!("cargo:rerun-if-changed=../../scripts/check-locales.py");
    println!("cargo:rerun-if-changed=../../crates/glim-app/src");
    println!("cargo:rerun-if-changed=src");
    let checked = Command::new("python3")
        .arg("../../scripts/check-locales.py")
        .status()
        .expect("Check locale catalogs");
    assert!(checked.success(), "Locale catalog validation failed");
    let mut catalogs = std::fs::read_dir("../../assets/locales")
        .expect("locale directory")
        .map(|entry| entry.expect("locale entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    catalogs.sort();
    let entries = catalogs
        .iter()
        .map(|path| {
            let code = path.file_stem().unwrap().to_str().unwrap();
            let path = std::fs::canonicalize(path).unwrap();
            format!("({code:?}, include_str!({:?}))", path.to_str().unwrap())
        })
        .collect::<Vec<_>>()
        .join(",\n");
    std::fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("locales.rs"),
        format!("const BUNDLED_CATALOGS: &[(&str, &str)] = &[{entries}];"),
    )
    .expect("embed locale catalogs");
    println!("cargo:rerun-if-changed=../../native/Preview.swift");
    println!("cargo:rerun-if-changed=../../native/FormatCatalog.swift");
    println!("cargo:rerun-if-changed=../../native/PreviewRequest.swift");
    println!("cargo:rerun-if-changed=../../native/ImagePreview.swift");
    println!("cargo:rerun-if-changed=../../native/ReaderResources.swift");
    println!("cargo:rerun-if-changed=../../native/ImageTextExtraction.swift");
    println!("cargo:rerun-if-changed=../../native/PreviewCopy.swift");
    println!("cargo:rerun-if-changed=../../native/FloatingReaderToolbar.swift");
    println!("cargo:rerun-if-changed=../../native/ReaderActions.swift");
    println!("cargo:rerun-if-changed=../../native/ReaderFind.swift");
    println!("cargo:rerun-if-changed=../../native/HTMLPreview.swift");
    println!("cargo:rerun-if-changed=../../native/OpenAPIResources.swift");
    println!("cargo:rerun-if-changed=../../assets/openapi/viewer.html");
    println!("cargo:rerun-if-changed=../../scripts/embed-openapi.py");
    assert!(
        Command::new("python3")
            .arg("../../scripts/embed-openapi.py")
            .arg(&limits_out)
            .status()
            .expect("Embed API reference")
            .success()
    );
    println!("cargo:rerun-if-changed=../../native/WorkbookPreview.swift");
    println!("cargo:rerun-if-changed=../../native/WordPreview.swift");
    println!("cargo:rerun-if-changed=../../native/PreviewMain.swift");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        _ => panic!("Unsupported macOS architecture"),
    };
    println!("cargo:rerun-if-changed=../../native/Archive.c");
    let archive = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("glim-archive");
    let status = Command::new("xcrun")
        .args(["clang", "-O2", "-Wall", "-Wextra", "-Werror", "-target"])
        .arg(format!("{arch}-apple-macos11.0"))
        .args(["../../native/Archive.c", "-larchive", "-o"])
        .arg(archive)
        .status()
        .expect("Compile macOS archive helper");
    assert!(status.success(), "Archive helper compilation failed");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("glim-preview");
    let main = output.with_file_name("main.swift");
    std::fs::copy("../../native/PreviewMain.swift", &main)
        .expect("Cannot prepare native preview entry point");
    let mut compiler = Command::new("xcrun");
    let sdk = Command::new("xcrun")
        .arg("--show-sdk-path")
        .output()
        .expect("macOS SDK path");
    let sdk = PathBuf::from(String::from_utf8_lossy(&sdk.stdout).trim());
    compiler.arg("swiftc");
    if sdk
        .join("System/Library/Frameworks/AppKit.framework/Headers/NSGlassEffectView.h")
        .is_file()
    {
        compiler.arg("-DGLIM_NATIVE_GLASS");
    }
    let status = compiler
        .args([
            "-O",
            "-swift-version",
            "5",
            "-target",
            &format!("{arch}-apple-macos11.0"),
            "../../native/WorkbookPreview.swift",
            "../../native/WordPreview.swift",
            "../../native/Preview.swift",
            "../../native/FormatCatalog.swift",
            "../../native/PreviewRequest.swift",
            "../../native/ImagePreview.swift",
            "../../native/ReaderResources.swift",
            "../../native/ImageTextExtraction.swift",
            "../../native/PreviewCopy.swift",
            "../../native/FloatingReaderToolbar.swift",
            "../../native/ReaderActions.swift",
            "../../native/ReaderFind.swift",
            "../../native/HTMLPreview.swift",
            "../../native/OpenAPIResources.swift",
        ])
        .arg(output.with_file_name("ReaderLimits.swift"))
        .arg(output.with_file_name("OpenAPIAssets.swift"))
        .arg(main)
        .arg("-o")
        .arg(output)
        .status()
        .expect("Native previews require Apple Command Line Tools (swiftc)");
    assert!(status.success(), "Native preview helper compilation failed");
}

fn diagram_cache_fingerprint() {
    use sha2::{Digest, Sha256};
    let mut files = vec![
        PathBuf::from("../../Cargo.lock"),
        PathBuf::from("../../assets/reader-limits.json"),
        PathBuf::from("../../crates/glim-app/src/app/diagram_engine.rs"),
        PathBuf::from("src/markdown_preview.rs"),
        PathBuf::from("src/markdown_preview/disk_cache.rs"),
    ];
    let mut directories = vec![PathBuf::from("../../assets/markdown")];
    while let Some(directory) = directories.pop() {
        println!("cargo:rerun-if-changed={}", directory.display());
        for entry in std::fs::read_dir(directory).expect("diagram asset directory") {
            let path = entry.expect("diagram asset entry").path();
            if path.is_dir() {
                directories.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut digest = Sha256::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = std::fs::read(&path).expect("diagram cache fingerprint input");
        digest.update(path.to_string_lossy().as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    println!(
        "cargo:rustc-env=GLIM_DIAGRAM_CACHE_FINGERPRINT={:x}",
        digest.finalize()
    );
}
