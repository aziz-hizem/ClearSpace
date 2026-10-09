//! File-extension interning: each distinct (lower-cased) extension gets a `u16` id so that
//! nodes stay small and per-type breakdowns are a single array pass.

use std::collections::HashMap;

/// No extension (also used for directories).
pub const EXT_NONE: u16 = 0;
/// Bucket for extensions beyond the table's capacity.
pub const EXT_OTHER: u16 = 1;

const MAX_EXT_LEN: usize = 16;

/// Interned extension names.
#[derive(Debug, Clone)]
pub struct ExtTable {
    names: Vec<Box<str>>,
    ids: HashMap<Box<str>, u16>,
}

impl Default for ExtTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtTable {
    pub fn new() -> ExtTable {
        ExtTable {
            names: vec!["".into(), "(other)".into()],
            ids: HashMap::new(),
        }
    }

    /// Number of ids in use, including the two reserved ones.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// The extension's lower-case name without the dot ("" for [`EXT_NONE`]).
    pub fn name(&self, id: u16) -> &str {
        self.names.get(id as usize).map_or("", |s| s)
    }

    pub(crate) fn slot(&self, id: u16) -> usize {
        (id as usize).min(self.names.len() - 1)
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        self.names.iter().map(|n| n.len() * 2 + 48).sum()
    }

    /// Interns the extension of a file name given as encoded `OsStr` bytes.
    pub fn intern_from_name(&mut self, name: &[u8]) -> u16 {
        let Some(ext) = extension_of(name) else {
            return EXT_NONE;
        };
        if let Some(&id) = self.ids.get(ext.as_str()) {
            return id;
        }
        if self.names.len() > u16::MAX as usize {
            return EXT_OTHER;
        }
        let id = self.names.len() as u16;
        let boxed: Box<str> = ext.into();
        self.names.push(boxed.clone());
        self.ids.insert(boxed, id);
        id
    }
}

/// Lower-cased extension for names like `photo.JPG`; `None` for `.gitignore`, `README`,
/// `archive.` and extensions that are long or contain non-ASCII-alphanumeric bytes.
fn extension_of(name: &[u8]) -> Option<String> {
    let dot = name.iter().rposition(|&b| b == b'.')?;
    if dot == 0 {
        return None;
    }
    let ext = &name[dot + 1..];
    if ext.is_empty()
        || ext.len() > MAX_EXT_LEN
        || !ext
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
    {
        return None;
    }
    Some(ext.iter().map(|b| b.to_ascii_lowercase() as char).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions() {
        assert_eq!(extension_of(b"a.TXT").as_deref(), Some("txt"));
        assert_eq!(extension_of(b"archive.tar.gz").as_deref(), Some("gz"));
        assert_eq!(extension_of(b".gitignore"), None);
        assert_eq!(extension_of(b"README"), None);
        assert_eq!(extension_of(b"trailing."), None);
        assert_eq!(extension_of("x.日本".as_bytes()), None);
        assert_eq!(extension_of(b"x.averyveryverylongextension"), None);
    }

    #[test]
    fn interning_is_stable() {
        let mut t = ExtTable::new();
        let a = t.intern_from_name(b"a.png");
        let b = t.intern_from_name(b"B.PNG");
        assert_eq!(a, b);
        assert_eq!(t.name(a), "png");
        assert_eq!(t.intern_from_name(b"noext"), EXT_NONE);
    }
}
