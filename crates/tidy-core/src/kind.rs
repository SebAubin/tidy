//! What a file is, from its name: enough to colour a map and group a summary.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Folder,
    App,
    Video,
    Image,
    Audio,
    Document,
    Archive,
    DiskImage,
    Code,
    Library,
    Other,
}

impl Kind {
    pub const ALL: [Kind; 11] = [
        Kind::Folder,
        Kind::App,
        Kind::Video,
        Kind::Image,
        Kind::Audio,
        Kind::Document,
        Kind::Archive,
        Kind::DiskImage,
        Kind::Code,
        Kind::Library,
        Kind::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Folder => "Folders",
            Kind::App => "Apps",
            Kind::Video => "Videos",
            Kind::Image => "Images",
            Kind::Audio => "Audio",
            Kind::Document => "Documents",
            Kind::Archive => "Archives",
            Kind::DiskImage => "Disk images & installers",
            Kind::Code => "Code",
            Kind::Library => "System & libraries",
            Kind::Other => "Other",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| serde_json::to_value(k).ok().and_then(|v| v.as_str().map(|x| x == s)).unwrap_or(false))
    }

    fn ext(name: &str) -> String {
        name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
    }

    pub fn of(name: &str) -> Kind {
        match Kind::ext(name).as_str() {
            "mp4" | "mov" | "m4v" | "mkv" | "avi" | "webm" | "wmv" | "flv" | "mts" | "braw" | "r3d" => Kind::Video,
            "jpg" | "jpeg" | "png" | "gif" | "heic" | "heif" | "tif" | "tiff" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "webp"
            | "psd" | "svg" | "bmp" | "avif" => Kind::Image,
            "mp3" | "m4a" | "aac" | "wav" | "flac" | "aiff" | "aif" | "ogg" | "opus" | "caf" | "logicx" => Kind::Audio,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "pages" | "numbers" | "key" | "txt" | "md" | "rtf" | "csv"
            | "epub" | "odt" => Kind::Document,
            "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "lz4" => Kind::Archive,
            "dmg" | "iso" | "pkg" | "img" | "msi" | "exe" | "vmdk" | "vdi" | "qcow2" | "raw-img" | "ipsw" | "xip" => Kind::DiskImage,
            "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "go" | "c" | "h" | "cpp" | "hpp" | "cs" | "swift" | "kt" | "java" | "rb"
            | "php" | "json" | "yaml" | "yml" | "toml" | "html" | "css" | "scss" | "sql" | "sh" | "map" | "lock" => Kind::Code,
            "dylib" | "so" | "a" | "o" | "rlib" | "rmeta" | "dll" | "framework" | "bundle" | "kext" | "plist" | "db" | "sqlite"
            | "sqlite3" | "dat" | "bin" | "pack" | "idx" => Kind::Library,
            _ => Kind::Other,
        }
    }

    /// Folders that macOS shows as a single item.
    pub fn of_folder(name: &str) -> Kind {
        match Kind::ext(name).as_str() {
            "app" => Kind::App,
            "photoslibrary" | "photolibrary" => Kind::Image,
            "fcpbundle" | "imovielibrary" | "tvlibrary" => Kind::Video,
            "musiclibrary" | "logicx" | "band" => Kind::Audio,
            "framework" | "bundle" | "xcarchive" | "kext" => Kind::Library,
            "pages" | "numbers" | "key" => Kind::Document,
            _ => Kind::Folder,
        }
    }
}
