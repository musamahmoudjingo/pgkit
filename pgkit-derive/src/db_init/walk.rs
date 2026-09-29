use std::path::{Path, PathBuf};

use proc_macro2::Span;

use super::fs::error;

/// Get all `.sql` files directly inside a folder named `kind` (`migrations`
/// or `seeds`), found by walking `root` recursively.
pub fn walk(root: &Path, kind: &str, span: Span) -> syn::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk_into(root, kind, span, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_into(dir: &Path, kind: &str, span: Span, files: &mut Vec<PathBuf>) -> syn::Result<()> {
    for path in entries(dir, span)? {
        if !path.is_dir() {
            continue;
        }
        if path.file_name().is_some_and(|name| name == kind) {
            files.extend(
                entries(&path, span)?
                    .into_iter()
                    .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "sql")),
            );
        } else {
            walk_into(&path, kind, span, files)?;
        }
    }
    Ok(())
}

fn entries(dir: &Path, span: Span) -> syn::Result<Vec<PathBuf>> {
    std::fs::read_dir(dir)
        .map_err(|err| error(span, dir, &err.to_string()))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|e| error(span, dir, &e.to_string()))
        })
        .collect()
}
