//! CSV export of a scan (one row per node, depth-first order).
//!
//! Columns: `path,kind,size_bytes,size_on_disk_bytes,files,modified_utc,attributes`.
//! Paths are written lossily (`U+FFFD` for unpaired surrogates); every path starts with the
//! scan root (e.g. `C:\`), so no cell starts with a spreadsheet formula character.

use std::io::{self, Write};

use crate::{NodeId, Tree, flags, time};

/// Writes the whole tree. Call [`Tree::aggregate`] first so folder totals are current.
pub fn write_csv<W: Write>(tree: &Tree, out: W) -> io::Result<()> {
    let mut out = io::BufWriter::with_capacity(1 << 20, out);
    out.write_all(b"path,kind,size_bytes,size_on_disk_bytes,files,modified_utc,attributes\r\n")?;
    let mut stack = vec![NodeId::ROOT];
    let mut line = String::with_capacity(512);
    while let Some(id) = stack.pop() {
        let node = tree.node(id);
        line.clear();
        push_field(&mut line, &tree.path(id).to_string_lossy());
        let kind = if node.has(flags::LINK) {
            "link"
        } else if node.is_dir() {
            "folder"
        } else {
            "file"
        };
        use std::fmt::Write as _;
        let _ = write!(
            line,
            ",{kind},{},{},{},{},",
            node.size,
            node.alloc,
            if node.is_dir() { node.files } else { 0 },
            time::format_iso8601(node.mtime)
        );
        push_field(&mut line, &attributes(node.flags));
        line.push_str("\r\n");
        out.write_all(line.as_bytes())?;
        // Push in reverse so children come out in stored order.
        let kids: Vec<NodeId> = tree.children(id).collect();
        stack.extend(kids.into_iter().rev());
    }
    out.flush()
}

fn attributes(f: u16) -> String {
    const NAMES: [(u16, &str); 8] = [
        (flags::CLOUD, "cloud"),
        (flags::HARDLINK_DUP, "hardlink"),
        (flags::COMPRESSED, "compressed"),
        (flags::SPARSE, "sparse"),
        (flags::UNREADABLE, "unreadable"),
        (flags::HIDDEN, "hidden"),
        (flags::SYSTEM, "system"),
        (flags::READONLY, "readonly"),
    ];
    NAMES
        .iter()
        .filter(|(b, _)| f & b != 0)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join(" ")
}

/// RFC 4180 quoting.
fn push_field(line: &mut String, value: &str) {
    if value.contains([',', '"', '\n', '\r']) {
        line.push('"');
        line.push_str(&value.replace('"', "\"\""));
        line.push('"');
    } else {
        line.push_str(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NewEntry;
    use std::ffi::OsStr;

    #[test]
    fn exports_with_quoting() {
        let mut t = Tree::new(OsStr::new("R"));
        let hidden = NewEntry {
            name: OsStr::new("a,\"b\".txt"),
            flags: flags::HIDDEN | flags::CLOUD,
            size: 5,
            alloc: 0,
            mtime: 1,
        };
        t.push_children(NodeId::ROOT, [hidden]).unwrap();
        t.aggregate();
        let mut buf = Vec::new();
        write_csv(&t, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = s.split("\r\n").collect();
        assert_eq!(lines[1], "R,folder,5,0,1,,");
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            lines[2],
            format!("\"R{sep}a,\"\"b\"\".txt\",file,5,0,0,1970-01-01T00:01Z,cloud hidden")
        );
    }
}
