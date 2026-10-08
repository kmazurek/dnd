//! Writing dropped paths to stdout.

use std::io::{self, Write};
use std::path::Path;

/// Writes each path on its own line and flushes, so that consumers reading a
/// pipe see drops immediately rather than when the process exits.
pub fn write_paths<'a>(
    mut out: impl Write,
    paths: impl IntoIterator<Item = &'a Path>,
) -> io::Result<()> {
    for path in paths {
        out.write_all(path.as_os_str().as_encoded_bytes())?;
        out.write_all(b"\n")?;
    }
    out.flush()
}

/// Like [`write_paths`] but targets stdout and ignores a closed pipe, which
/// simply means nobody is listening any more.
pub fn print_paths<'a>(paths: impl IntoIterator<Item = &'a Path>) {
    if let Err(err) = write_paths(io::stdout().lock(), paths)
        && err.kind() != io::ErrorKind::BrokenPipe
    {
        eprintln!("dnd: failed to write to stdout: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_path_per_line() {
        let mut buf = Vec::new();
        write_paths(&mut buf, [Path::new("/a b/c"), Path::new("/d")]).unwrap();
        assert_eq!(buf, b"/a b/c\n/d\n");
    }

    #[test]
    fn nothing_for_no_paths() {
        let mut buf = Vec::new();
        write_paths(&mut buf, []).unwrap();
        assert!(buf.is_empty());
    }
}
