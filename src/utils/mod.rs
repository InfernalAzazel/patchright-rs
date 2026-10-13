//! File and text helpers for patching the playwright-rs checkout.

use std::fs;
use std::path::Path;

/// Where the playwright-rs checkout lives; all file paths are relative to it.
pub const ROOT: &str = "playwright-rust";

pub fn read(file: &str) -> String {
    let path = Path::new(ROOT).join(file);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

pub fn write(file: &str, contents: &str) {
    let path = Path::new(ROOT).join(file);
    fs::write(&path, contents).unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
}

/// Replaces `old`, which must occur exactly once, so an upstream change
/// fails loudly instead of being silently skipped.
pub fn patch(file: &str, old: &str, new: &str) {
    let source = read(file);
    let count = source.matches(old).count();
    assert!(count == 1, "{file}: expected one match, found {count}:\n{old}");
    write(file, &source.replacen(old, new, 1));
}

/// Replaces every occurrence of `old`; at least one must exist.
pub fn patch_all(file: &str, old: &str, new: &str) {
    let source = read(file);
    assert!(source.contains(old), "{file}: no match for:\n{old}");
    write(file, &source.replace(old, new));
}

pub fn insert_after(file: &str, anchor: &str, text: &str) {
    patch(file, anchor, &format!("{anchor}{text}"));
}

pub fn insert_before(file: &str, anchor: &str, text: &str) {
    patch(file, anchor, &format!("{text}{anchor}"));
}

/// Replaces `word` where it is not part of a longer identifier, so
/// `playwright_rs_macros` and `playwright-rs-trace` are left alone.
pub fn replace_word(source: &str, word: &str, new: &str) -> String {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(pos) = rest.find(word) {
        let before = rest[..pos].chars().next_back();
        let after = rest[pos + word.len()..].chars().next();
        let standalone = !before.is_some_and(ident) && !after.is_some_and(|c| ident(c) || c == '-');
        out.push_str(&rest[..pos]);
        out.push_str(if standalone { new } else { word });
        rest = &rest[pos + word.len()..];
    }
    out.push_str(rest);
    out
}

/// Collects `.rs` files under `dir`, as paths relative to [`ROOT`], skipping
/// `fuzz` and `target`.
pub fn rust_files(dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if path.is_dir() {
            if name != "fuzz" && name != "target" {
                rust_files(&path, out);
            }
        } else if name.ends_with(".rs") {
            let rel = path.strip_prefix(ROOT).unwrap();
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}
