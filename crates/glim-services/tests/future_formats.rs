use glim_services::{formats, text_formats, vector_formats};
#[test]
fn container_previews_reject_damage_and_do_not_extract() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paths.zip");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    archive
        .start_file("../outside.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    archive.write_all(b"untrusted").unwrap();
    archive.finish().unwrap();
    let before = std::fs::read(&path).unwrap();
    let preview = formats::read_preview(&path).unwrap();
    assert!(
        preview
            .text
            .contains(&glim_services::locale::tr(" · Unsafe extraction path"))
    );
    assert_eq!(before, std::fs::read(&path).unwrap());
    assert!(!dir.path().parent().unwrap().join("outside.txt").exists());
    std::fs::write(&path, b"PK\x03\x04broken").unwrap();
    assert!(formats::read_preview(&path).is_err());
}
#[test]
fn documents_and_structured_data_keep_source_separate() {
    let source = "= 标题\n\n[source,python]\n----\nprint('test')\n----\n";
    let preview = text_formats::markdown(std::path::Path::new("test.adoc"), source);
    assert!(preview.starts_with("# 标题"));
    assert!(preview.contains("```\nprint('test')\n```"));
    assert!(glim_services::json::parse_format("{name:'demo',hex:0x10,}", "json5").is_ok());
    let nodes = glim_services::json::parse_format("{\"a\":1}\n{\"a\":2}\n", "ndjson").unwrap();
    assert_eq!(nodes[0].children.len(), 2);
    assert!(glim_services::json::parse_format("{}\ninvalid", "ndjson").is_err());
    assert_eq!(
        glim_services::csv::read_page(
            "a\tb\n1\t\"two\tthree\"".as_bytes(),
            b'\t',
            0,
            0,
            &glim_services::work::Cancellation::default()
        )
        .unwrap()
        .rows[1][1],
        "two\tthree"
    );
    let coordinates = text_formats::markdown(
        std::path::Path::new("test.gpx"),
        "<gpx><wpt lat=\"39.9\" lon=\"116.4\"/></gpx>",
    );
    assert!(coordinates.contains("| 116.4 | 39.9 |"));
}
#[test]
fn malformed_mesh_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.obj");
    std::fs::write(&path, "v 0 0 0\nf 1 2 3").unwrap();
    assert!(vector_formats::read_preview(&path).is_err());
}
#[test]
fn numpy_values_match_the_declared_shape() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("array.npy");
    let header = "{'descr': '<f8', 'fortran_order': False, 'shape': (1, 2), }\n";
    let mut bytes = b"\x93NUMPY\x01\x00".to_vec();
    bytes.extend_from_slice(&(header.len() as u16).to_le_bytes());
    bytes.extend_from_slice(header.as_bytes());
    bytes.extend_from_slice(&1.5_f64.to_le_bytes());
    bytes.extend_from_slice(&2.5_f64.to_le_bytes());
    std::fs::write(&path, &bytes).unwrap();
    assert!(
        formats::read_preview(&path)
            .unwrap()
            .text
            .contains("[1] 2.5")
    );
    bytes.truncate(bytes.len() - 8);
    std::fs::write(&path, bytes).unwrap();
    assert!(formats::read_preview(&path).is_err());
}
#[test]
fn optional_exploration_corpus_is_read_without_mutation() {
    let Some(root) = std::env::var_os("GLIM_FORMAT_FIXTURES") else {
        return;
    };
    fn visit(path: &std::path::Path, count: &mut usize) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, count);
                continue;
            }
            if path.file_name().unwrap() == "README.md" {
                continue;
            }
            let before = std::fs::read(&path).unwrap();
            if path.file_name().unwrap() == "截断ZIP.zip" {
                assert!(formats::read_preview(&path).is_err());
            } else if formats::supports(&path) {
                assert!(!formats::read_preview(&path).unwrap().text.is_empty());
            } else if vector_formats::supports(&path) {
                assert!(!vector_formats::read_preview(&path).unwrap().png.is_empty());
            } else if glim_services::media::supports(&path) {
                assert!(
                    !glim_services::media::read_image(&path)
                        .unwrap()
                        .png
                        .is_empty()
                );
            } else {
                let document = glim_services::files::read_document(&path).unwrap();
                if text_formats::supports(&path) {
                    assert!(!text_formats::markdown(&path, &document.text).is_empty());
                }
                if path
                    .extension()
                    .is_some_and(|e| e == "json5" || e == "ndjson")
                {
                    glim_services::json::parse_format(
                        &document.text,
                        path.extension().unwrap().to_str().unwrap(),
                    )
                    .unwrap();
                }
            }
            assert_eq!(before, std::fs::read(&path).unwrap(), "{}", path.display());
            *count += 1;
        }
    }
    let mut count = 0;
    visit(std::path::Path::new(&root), &mut count);
    assert_eq!(count, 35);
}
