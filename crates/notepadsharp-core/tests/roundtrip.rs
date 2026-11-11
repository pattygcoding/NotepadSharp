//! End-to-end test of the document lifecycle: open a real file, edit it with the core
//! APIs, run a search/replace, save, and verify the bytes on disk.

use std::path::PathBuf;

use notepadsharp_core::lineops;
use notepadsharp_core::search::{SearchMode, SearchQuery};
use notepadsharp_core::{Document, Encoding, Eol, Language};

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("notepadsharp-it-{}-{name}", std::process::id()))
}

#[test]
fn edit_search_and_save_preserves_encoding_and_line_endings() {
    let path = temp_path("roundtrip.rs");
    let original = "\u{FEFF}// demo\r\nfn main() {\r\n    println!(\"hi\");\r\n}\r\n";
    std::fs::write(&path, original.as_bytes()).expect("write sample file");

    let mut document = Document::open(&path).expect("open sample file");

    // Decoding: BOM detected, CRLF remembered, Rust detected from the extension.
    assert_eq!(document.encoding, Encoding::Utf8Bom);
    assert_eq!(document.eol, Eol::CrLf);
    assert_eq!(document.language, Language::Rust);
    assert_eq!(
        document.text(),
        "// demo\nfn main() {\n    println!(\"hi\");\n}\n"
    );
    assert!(!document.is_dirty());

    // Line operation: duplicate the `println!` line.
    let caret = document.buffer.line_start(2);
    let selection = lineops::duplicate_line(&mut document.buffer, caret);
    assert_eq!(document.buffer.line_count(), 6);
    assert_eq!(document.buffer.line_of_offset(selection.start), 3);
    assert!(document.is_dirty());

    // Search and replace with a capture group.
    let query = SearchQuery::new(r#"println!\("(\w+)"\)"#, SearchMode::Regex, true);
    let (replaced, count) = query
        .replace_all(document.text(), r#"println!("$1 again")"#)
        .expect("regex replace");
    assert_eq!(count, 2);
    document.replace_all(replaced);

    // Saving restores the BOM and CRLF endings.
    document.save().expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF], "BOM preserved");
    let text = String::from_utf8(bytes[3..].to_vec()).expect("valid utf-8 payload");
    assert_eq!(
        text,
        "// demo\r\nfn main() {\r\n    println!(\"hi again\");\r\n    println!(\"hi again\");\r\n}\r\n"
    );
    assert!(!document.is_dirty(), "saving clears the modified flag");

    // Undo takes the document back to the state before the replace.
    assert!(document.undo().is_some());
    assert_eq!(document.text().matches("hi again").count(), 0);

    std::fs::remove_file(&path).ok();
}

#[test]
fn round_trips_utf16_documents() {
    let path = temp_path("utf16.txt");
    let original = "héllo\r\nwörld\r\n";
    std::fs::write(&path, Encoding::Utf16Le.encode(original)).expect("write utf-16 file");

    let mut document = Document::open(&path).expect("open utf-16 file");
    assert_eq!(document.encoding, Encoding::Utf16Le);
    assert_eq!(document.eol, Eol::CrLf);
    assert_eq!(document.text(), "héllo\nwörld\n");

    document.buffer.insert(0, "> ");
    document.save().expect("save");

    let (decoded, encoding) = Encoding::decode(&std::fs::read(&path).expect("read back"));
    assert_eq!(encoding, Encoding::Utf16Le);
    assert_eq!(decoded, "> héllo\r\nwörld\r\n");

    std::fs::remove_file(&path).ok();
}
