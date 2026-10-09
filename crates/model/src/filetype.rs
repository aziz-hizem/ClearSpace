//! Plain-language file categories for colouring the treemap and the "file types" breakdown.
//!
//! This is presentation only: a category never implies anything about whether a file is safe
//! to remove (that is the rules engine's job, and only for curated rule matches).

/// A broad, user-facing kind of file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Category {
    Documents = 0,
    Images = 1,
    Videos = 2,
    Audio = 3,
    Archives = 4,
    Apps = 5,
    Code = 6,
    Data = 7,
    Other = 8,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Category::Documents,
        Category::Images,
        Category::Videos,
        Category::Audio,
        Category::Archives,
        Category::Apps,
        Category::Code,
        Category::Data,
        Category::Other,
    ];

    /// Stable machine key (used by the UI for colours and translated labels).
    pub fn key(self) -> &'static str {
        match self {
            Category::Documents => "documents",
            Category::Images => "images",
            Category::Videos => "videos",
            Category::Audio => "audio",
            Category::Archives => "archives",
            Category::Apps => "apps",
            Category::Code => "code",
            Category::Data => "data",
            Category::Other => "other",
        }
    }
}

/// Category for a lower-case extension without the dot (`""` = no extension).
pub fn category_of(ext: &str) -> Category {
    use Category::*;
    match ext {
        "pdf" | "doc" | "docx" | "odt" | "rtf" | "txt" | "md" | "xls" | "xlsx" | "ods" | "csv"
        | "ppt" | "pptx" | "odp" | "epub" | "pages" | "numbers" | "key" | "one" | "xps" | "tex" => {
            Documents
        }
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tif" | "tiff" | "webp" | "heic" | "heif"
        | "avif" | "svg" | "ico" | "raw" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "psd" | "ai"
        | "xcf" => Images,
        "mp4" | "mkv" | "mov" | "avi" | "wmv" | "webm" | "m4v" | "mpg" | "mpeg" | "flv" | "3gp"
        | "ts" | "mts" | "m2ts" | "vob" => Videos,
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" | "opus" | "wma" | "aiff" | "mid"
        | "midi" => Audio,
        "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "cab" | "iso"
        | "img" | "vhd" | "vhdx" | "msi" | "msix" | "msixbundle" | "appx" | "appxbundle"
        | "nupkg" | "whl" | "jar" | "esd" | "wim" => Archives,
        "exe" | "dll" | "sys" | "drv" | "ocx" | "cpl" | "mui" | "efi" | "com" | "scr" | "node"
        | "so" | "dylib" => Apps,
        "rs" | "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "java" | "kt" | "py" | "js" | "mjs"
        | "cjs" | "jsx" | "tsx" | "go" | "rb" | "php" | "swift" | "lua" | "sh" | "ps1" | "bat"
        | "cmd" | "html" | "htm" | "css" | "scss" | "vue" | "pdb" | "obj" | "o" | "a" | "lib"
        | "rlib" | "rmeta" | "class" | "pyc" | "map" | "wasm" => Code,
        "json" | "xml" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" | "db" | "sqlite"
        | "sqlite3" | "mdb" | "accdb" | "log" | "etl" | "evtx" | "dat" | "bin" | "pak" | "dmp"
        | "mdmp" | "hdmp" | "tmp" | "cache" | "bak" | "pst" | "ost" | "edb" | "vdm" | "ttf"
        | "otf" | "ttc" | "woff" | "woff2" => Data,
        _ => Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_extensions() {
        assert_eq!(category_of("jpg"), Category::Images);
        assert_eq!(category_of("mp4"), Category::Videos);
        assert_eq!(category_of("dll"), Category::Apps);
        assert_eq!(category_of(""), Category::Other);
        assert_eq!(category_of("weird"), Category::Other);
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<_> = Category::ALL.iter().map(|c| c.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), Category::ALL.len());
    }
}
