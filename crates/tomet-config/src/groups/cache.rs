//! Cache configuration ([`CacheConfig`]).

use tomet_spec_derive::Config;

/// Cache configuration for vault-level `$cache.*` expression resolution.
#[derive(Debug, Clone, Default, PartialEq, Config)]
#[config(group = "cache")]
pub struct CacheConfig {
    /// Directory for repository-shared cache files, relative to the vault root.
    /// There is no default path; `$cache.*` cannot resolve without an explicit `::cache` declaration.
    #[config(key = "cache.dir", type = "string")]
    pub dir: Option<String>,

    /// Whether local-only cache references (`$cache.<kind>(..., local: true)`) are permitted.
    #[config(key = "cache.local", default = false)]
    pub local: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_config_from_str;

    #[test]
    fn undeclared_cache_has_no_dir_and_disallows_local() {
        let cfg = load_config_from_str("::format(section: (title: \"bare\"))\n").unwrap();
        assert_eq!(cfg.cache, CacheConfig::default());
        assert_eq!(cfg.cache.dir, None);
        assert!(!cfg.cache.local);
    }

    #[test]
    fn declared_cache_parses_dir_and_local() {
        let cfg = load_config_from_str("::cache(dir: \".tomet/cache\", local: true)\n").unwrap();
        assert_eq!(cfg.cache.dir.as_deref(), Some(".tomet/cache"));
        assert!(cfg.cache.local);
    }

    #[test]
    fn cache_allows_local_without_dir() {
        let cfg = load_config_from_str("::cache(local: true)\n").unwrap();
        assert_eq!(cfg.cache.dir, None);
        assert!(cfg.cache.local);
    }
}
