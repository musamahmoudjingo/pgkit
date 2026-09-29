//! Filesystem and path operations that fail as spanned compile errors.

use std::path::Path;

use proc_macro2::Span;

/// `path` relative to `root`, as a string.
pub fn relative(root: &Path, path: &Path, span: Span) -> syn::Result<String> {
    path.strip_prefix(root)
        .map_err(|_| error(span, path, "not under the scan root"))
        .and_then(|rel| path_as_string(rel, span))
}

pub fn file_name(path: &Path, span: Span) -> syn::Result<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .ok_or_else(|| error(span, path, "not a UTF-8 file name"))
}

pub fn path_as_string(path: &Path, span: Span) -> syn::Result<String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| error(span, path, "not a UTF-8 path"))
}

pub fn read(path: &Path, span: Span) -> syn::Result<String> {
    std::fs::read_to_string(path).map_err(|e| error(span, path, &e.to_string()))
}

pub fn error(span: Span, path: &Path, message: &str) -> syn::Error {
    syn::Error::new(span, format!("{}: {message}", path.display()))
}
