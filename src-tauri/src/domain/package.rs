use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::install::PackageVersion;
use crate::error::{AppError, AppResult};

string_enum!(
    /// Which catalog a package comes from.
    SourceKind, "source kind", {
        WingetCommunity => "winget_community",
        MsStore => "ms_store",
        Scoop => "scoop",
        Chocolatey => "chocolatey",
    }
);

string_enum!(
    /// How a package is installed.
    PackagingKind, "packaging kind", {
        Msix => "msix",
        MsixBundle => "msix_bundle",
        Exe => "exe",
        Msi => "msi",
        Portable => "portable",
        Unknown => "unknown",
    }
);

impl SourceKind {
    pub const ALL: [SourceKind; 4] = [
        SourceKind::WingetCommunity,
        SourceKind::MsStore,
        SourceKind::Scoop,
        SourceKind::Chocolatey,
    ];

    pub fn display_name(&self) -> &'static str {
        match self {
            SourceKind::WingetCommunity => "winget community",
            SourceKind::MsStore => "Store catalog (msstore)",
            SourceKind::Scoop => "Scoop",
            SourceKind::Chocolatey => "Chocolatey",
        }
    }
}

/// Stable package identifier of the form `{source}/{package_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PackageId(String);

impl PackageId {
    pub fn new(source: SourceKind, package_id: &str) -> Self {
        PackageId(format!("{source}/{package_id}"))
    }

    /// Splits on the first '/' into the source and the source-local id.
    pub fn parse(s: &str) -> AppResult<(SourceKind, String)> {
        let (src, rest) = s
            .split_once('/')
            .ok_or_else(|| AppError::InvalidConfig(format!("invalid package id: {s}")))?;
        Ok((src.parse::<SourceKind>()?, rest.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Unicode format characters (general category Cf, from the Unicode character database; no
/// crate) and the noncharacters U+FFFE and U+FFFF. They are invisible or reorder text.
fn is_format_or_noncharacter(c: char) -> bool {
    matches!(u32::from(c),
        0x00AD | 0x0600..=0x0605 | 0x061C | 0x06DD | 0x070F | 0x0890..=0x0891 | 0x08E2
        | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0x2066..=0x206F
        | 0xFEFF | 0xFFF9..=0xFFFB | 0xFFFE | 0xFFFF | 0x110BD | 0x110CD | 0x13430..=0x1343F
        | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0xE0001 | 0xE0020..=0xE007F)
}

/// Checks a winget `PackageIdentifier` before any call uses it (hardening design §4).
///
/// The rules follow the `PackageIdentifier` pattern of the winget manifest schema
/// (`schemas/JSON/manifests`, manifest version 1.10.0, the latest on `microsoft/winget-cli`
/// master, read 2026-11-25; the 1.9.0 schema has the same line):
/// a segment is 1 to 32 characters other than `.`, white space, a backslash,
/// `/ : * ? " < > |` and the controls `U+0001` to `U+001F`; an id has two to eight segments
/// and at most 128 characters. Two rules go beyond the schema: no leading `-` (the id could pass for a flag), and no `U+007F`.
pub fn validate_winget_id(id: &str) -> AppResult<()> {
    let bad = |why: &str| AppError::InvalidConfig(format!("not a valid package id ({why})"));
    let len = id.chars().count();
    if !(3..=128).contains(&len) {
        return Err(bad("length"));
    }
    if id.starts_with('-') {
        return Err(bad("leading dash"));
    }
    let forbidden = |c: char| {
        c.is_whitespace()
            || c.is_control()
            || is_format_or_noncharacter(c)
            || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
    };
    let mut segments = 0;
    for seg in id.split('.') {
        segments += 1;
        if seg.is_empty() || seg.chars().count() > 32 {
            return Err(bad("segment"));
        }
        if seg.chars().any(forbidden) {
            return Err(bad("character"));
        }
    }
    if !(2..=8).contains(&segments) {
        return Err(bad("segments"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageSummary {
    pub id: PackageId,
    pub name: String,
    pub publisher: String,
    pub latest_version: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub rating: Option<f32>,
    pub rating_count: Option<u32>,
    pub source: SourceKind,
    pub is_paid: bool,
    pub packaging: PackagingKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallerMetadata {
    pub architecture: Option<String>,
    pub install_type: PackagingKind,
    pub size_bytes: Option<u64>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageDetail {
    pub summary: PackageSummary,
    pub full_description: String,
    pub screenshots: Vec<String>,
    pub website: Option<String>,
    pub license: Option<String>,
    /// Link to the license terms; https only. It only feeds `agreement_url` (SPEC §6.6).
    #[serde(default)]
    pub license_url: Option<String>,
    pub release_notes: Option<String>,
    pub installer: InstallerMetadata,
    pub versions: BTreeMap<String, PackageVersion>,
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    pub fn summary(source: SourceKind, pkg: &str, name: &str) -> PackageSummary {
        PackageSummary {
            id: PackageId::new(source, pkg),
            name: name.into(),
            publisher: "Example Publisher".into(),
            latest_version: "1.2.3".into(),
            description: "A package".into(),
            icon_url: Some("https://example.org/i.png".into()),
            rating: Some(4.5),
            rating_count: Some(120),
            source,
            is_paid: false,
            packaging: PackagingKind::Msi,
        }
    }

    pub fn detail(source: SourceKind, pkg: &str, name: &str) -> PackageDetail {
        let mut versions = BTreeMap::new();
        versions.insert(
            "1.2.3".to_string(),
            PackageVersion {
                version: "1.2.3".into(),
                release_date: Some("2026-01-02".into()),
                installer_url: Some("https://example.org/a.msi".into()),
                sha256: Some("abc".into()),
            },
        );
        versions.insert(
            "1.0.0".to_string(),
            PackageVersion {
                version: "1.0.0".into(),
                release_date: None,
                installer_url: None,
                sha256: None,
            },
        );
        PackageDetail {
            summary: summary(source, pkg, name),
            full_description: "Long text".into(),
            screenshots: vec![
                "https://example.org/1.png".into(),
                "https://example.org/2.png".into(),
            ],
            website: Some("https://example.org".into()),
            license: Some("MIT".into()),
            license_url: None,
            release_notes: None,
            installer: InstallerMetadata {
                architecture: Some("x64".into()),
                install_type: PackagingKind::Msi,
                size_bytes: Some(1_048_576),
                sha256: Some("abc".into()),
            },
            versions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winget_id_accepts_real_ids() {
        for id in [
            "VideoLAN.VLC",
            "Microsoft.VisualStudioCode",
            "Git.Git",
            "7zip.7zip",
        ] {
            assert!(validate_winget_id(id).is_ok(), "{id}");
        }
        assert!(validate_winget_id(&["a"; 8].join(".")).is_ok());
        assert!(validate_winget_id(&format!("{}.b", "a".repeat(32))).is_ok());
    }

    #[test]
    fn winget_id_refuses_bad_ids() {
        let long_segment = format!("a.{}", "b".repeat(33));
        let nine = ["a"; 9].join(".");
        let too_long = format!(
            "{}.{}.{}.{}.b",
            "a".repeat(32),
            "a".repeat(32),
            "a".repeat(32),
            "a".repeat(32)
        );
        assert!(too_long.chars().count() > 128);
        for id in [
            "",
            "VLC",
            "-x.y",
            "--accept-package-agreements",
            "a b.c",
            "a.b\n",
            "a.b\u{7f}",
            "a..b",
            ".a.b",
            "a.b.",
            "a.b/c",
            "a.b\\c",
            "a:b.c",
            "a.b*",
            "a.\"b\".c",
            "a.<b>",
            "a.b|c",
            "a.b?",
            "a.b\u{200b}c",
            "a.\u{200f}b",
            "a.b\u{202e}c",
            "a.\u{2060}b",
            "\u{feff}a.b",
            "a.b\u{fffe}",
            "a.b\u{ffff}",
            "a.b\u{ad}c",
            long_segment.as_str(),
            nine.as_str(),
            too_long.as_str(),
        ] {
            assert!(
                matches!(validate_winget_id(id), Err(AppError::InvalidConfig(_))),
                "{id:?}"
            );
        }
    }

    #[test]
    fn source_kind_round_trip() {
        for k in SourceKind::ALL {
            assert_eq!(k.to_string().parse::<SourceKind>().unwrap(), k);
            assert_eq!(serde_json::to_value(k).unwrap(), k.to_string());
        }
        assert_eq!(SourceKind::VARIANTS, &SourceKind::ALL);
        assert_eq!(SourceKind::MsStore.to_string(), "ms_store");
        assert!("nope".parse::<SourceKind>().is_err());
        assert_eq!(
            SourceKind::MsStore.display_name(),
            "Store catalog (msstore)"
        );
        assert_eq!(
            SourceKind::WingetCommunity.display_name(),
            "winget community"
        );
    }

    #[test]
    fn packaging_kind_round_trip() {
        assert_eq!(PackagingKind::VARIANTS.len(), 6);
        for k in PackagingKind::VARIANTS {
            assert_eq!(k.to_string().parse::<PackagingKind>().unwrap(), *k);
            assert_eq!(serde_json::to_value(k).unwrap(), k.to_string());
            let back: PackagingKind =
                serde_json::from_value(serde_json::to_value(k).unwrap()).unwrap();
            assert_eq!(back, *k);
        }
        assert_eq!(PackagingKind::MsixBundle.to_string(), "msix_bundle");
        assert!("".parse::<PackagingKind>().is_err());
        assert!("MSI".parse::<PackagingKind>().is_err());
    }

    #[test]
    fn package_id_round_trip() {
        let id = PackageId::new(SourceKind::WingetCommunity, "Vendor.App/x");
        assert_eq!(id.as_str(), "winget_community/Vendor.App/x");
        assert_eq!(id.to_string(), id.as_str());
        let (src, rest) = PackageId::parse(id.as_str()).unwrap();
        assert_eq!(src, SourceKind::WingetCommunity);
        assert_eq!(rest, "Vendor.App/x");
        assert_eq!(
            serde_json::to_value(&id).unwrap(),
            serde_json::json!("winget_community/Vendor.App/x")
        );
    }

    #[test]
    fn package_id_parse_errors() {
        assert!(PackageId::parse("nosep").is_err());
        assert!(PackageId::parse("bogus/pkg").is_err());
    }

    #[test]
    fn detail_serde_round_trip() {
        let d = fixtures::detail(SourceKind::Scoop, "p", "P");
        let back: PackageDetail =
            serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(back, d);
    }
}
