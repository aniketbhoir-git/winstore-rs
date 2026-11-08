use serde::{Deserialize, Serialize};

use super::package::SourceKind;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCapabilities {
    pub search: bool,
    pub detail: bool,
    pub featured: bool,
    pub categories: bool,
    pub install: bool,
    pub update: bool,
    pub pin: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceConfig {
    pub kind: SourceKind,
    pub enabled: bool,
    pub priority: i32,
    pub display_name: String,
    pub extra_config: serde_json::Value,
    pub last_sync: Option<i64>,
}

impl SourceConfig {
    /// Default row for a source; only the community catalog starts enabled.
    pub fn default_for(kind: SourceKind) -> Self {
        let priority = SourceKind::ALL.iter().position(|k| *k == kind).unwrap_or(0) as i32;
        SourceConfig {
            kind,
            enabled: kind == SourceKind::WingetCommunity,
            priority,
            display_name: kind.display_name().to_string(),
            extra_config: serde_json::Value::Null,
            last_sync: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_default_all_false() {
        let c = SourceCapabilities::default();
        let flags = [
            c.search,
            c.detail,
            c.featured,
            c.categories,
            c.install,
            c.update,
            c.pin,
        ];
        assert!(flags.iter().all(|f| !f));
    }

    #[test]
    fn defaults_per_kind() {
        for (i, k) in SourceKind::ALL.iter().enumerate() {
            let c = SourceConfig::default_for(*k);
            assert_eq!(c.priority, i as i32);
            assert_eq!(c.enabled, *k == SourceKind::WingetCommunity);
            assert_eq!(c.display_name, k.display_name());
            assert!(c.extra_config.is_null());
            assert_eq!(c.last_sync, None);
        }
    }
}
