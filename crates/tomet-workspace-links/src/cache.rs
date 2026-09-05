//! SQLite-backed cache of each source file's extracted links, keyed by
//! path + mtime. `links_for` only re-parses a file when its mtime no
//! longer matches what's cached -- everything else is a plain SQLite
//! read.

use std::fmt;
use std::path::Path;
use std::time::SystemTime;

use rusqlite::{Connection, OptionalExtension, params};

use crate::collect::{DocumentLink, LinkKind, collect_links};

#[derive(Debug)]
pub enum LinkCacheError {
    Io {
        path: String,
        source: std::io::Error,
    },
    Parse {
        path: String,
        source: tomet_parser::Error,
    },
    Sqlite(rusqlite::Error),
}

impl fmt::Display for LinkCacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkCacheError::Io { path, source } => write!(f, "failed to read {path}: {source}"),
            LinkCacheError::Parse { path, source } => {
                write!(f, "failed to parse {path}: {source}")
            }
            LinkCacheError::Sqlite(source) => write!(f, "link cache error: {source}"),
        }
    }
}

impl std::error::Error for LinkCacheError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LinkCacheError::Io { source, .. } => Some(source),
            LinkCacheError::Parse { source, .. } => Some(source),
            LinkCacheError::Sqlite(source) => Some(source),
        }
    }
}

impl From<rusqlite::Error> for LinkCacheError {
    fn from(e: rusqlite::Error) -> Self {
        LinkCacheError::Sqlite(e)
    }
}

/// Whether `links_for` reused the cached links (`Hit`) or had to
/// re-parse the file because its mtime changed or it wasn't cached yet
/// (`Miss`). Exists so callers/tests can observe cache behavior directly
/// instead of inferring it from timing.
#[derive(Debug, Clone)]
pub enum CacheOutcome {
    Hit(Vec<DocumentLink>),
    Miss(Vec<DocumentLink>),
}

impl CacheOutcome {
    pub fn links(&self) -> &[DocumentLink] {
        match self {
            CacheOutcome::Hit(links) | CacheOutcome::Miss(links) => links,
        }
    }

    pub fn is_hit(&self) -> bool {
        matches!(self, CacheOutcome::Hit(_))
    }
}

pub struct LinkCache {
    conn: Connection,
}

impl LinkCache {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS files (
                path        TEXT PRIMARY KEY,
                mtime_secs  INTEGER NOT NULL,
                mtime_nanos INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS links (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path  TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
                kind       TEXT NOT NULL CHECK (kind IN ('file', 'embed', 'tm', 'ref')),
                target     TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                start_col  INTEGER NOT NULL,
                end_line   INTEGER NOT NULL,
                end_col    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_links_file_path ON links(file_path);",
        )?;
        Ok(Self { conn })
    }

    /// In-memory cache, for tests that don't need a real file on disk.
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            "CREATE TABLE files (
                path        TEXT PRIMARY KEY,
                mtime_secs  INTEGER NOT NULL,
                mtime_nanos INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE links (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path  TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
                kind       TEXT NOT NULL CHECK (kind IN ('file', 'embed', 'tm', 'ref')),
                target     TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                start_col  INTEGER NOT NULL,
                end_line   INTEGER NOT NULL,
                end_col    INTEGER NOT NULL
            );
            CREATE INDEX idx_links_file_path ON links(file_path);",
        )?;
        Ok(Self { conn })
    }

    pub fn links_for(&mut self, source: &Path) -> Result<CacheOutcome, LinkCacheError> {
        // One database serves every vault, so the key has to be absolute:
        // two vaults each holding `docs/README.tmt` would otherwise be the
        // same row. `canonicalize` also resolves symlinks, which is what
        // keeps a vault reached by two paths from being cached twice.
        let path_str = std::fs::canonicalize(source)
            .unwrap_or_else(|_| source.to_path_buf())
            .to_string_lossy()
            .to_string();
        let metadata = std::fs::metadata(source).map_err(|e| LinkCacheError::Io {
            path: path_str.clone(),
            source: e,
        })?;
        let (secs, nanos) = mtime_parts(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));

        let cached_mtime: Option<(i64, i64)> = self
            .conn
            .query_row(
                "SELECT mtime_secs, mtime_nanos FROM files WHERE path = ?1",
                params![path_str],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        if cached_mtime == Some((secs, nanos)) {
            let links = self.read_links(&path_str)?;
            return Ok(CacheOutcome::Hit(links));
        }

        let src = std::fs::read_to_string(source).map_err(|e| LinkCacheError::Io {
            path: path_str.clone(),
            source: e,
        })?;
        let doc = tomet_parser::parse_document(&src).map_err(|e| LinkCacheError::Parse {
            path: path_str.clone(),
            source: e,
        })?;
        let links = collect_links(&doc);

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO files (path, mtime_secs, mtime_nanos) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET mtime_secs = excluded.mtime_secs, mtime_nanos = excluded.mtime_nanos",
            params![path_str, secs, nanos],
        )?;
        tx.execute("DELETE FROM links WHERE file_path = ?1", params![path_str])?;
        for link in &links {
            tx.execute(
                "INSERT INTO links (file_path, kind, target, start_line, start_col, end_line, end_col)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    path_str,
                    link.kind.as_str(),
                    link.target,
                    link.span.start.line as i64,
                    link.span.start.column as i64,
                    link.span.end.line as i64,
                    link.span.end.column as i64,
                ],
            )?;
        }
        tx.commit()?;

        Ok(CacheOutcome::Miss(links))
    }

    /// Drops every row whose file is no longer on disk, returning how
    /// many went. A shared cache accumulates these as vaults are moved or
    /// deleted; nothing else notices them, because a row is only ever
    /// read back by the path it is keyed on.
    pub fn prune_missing(&mut self) -> rusqlite::Result<usize> {
        let stale: Vec<String> = {
            let mut stmt = self.conn.prepare("SELECT path FROM files")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.filter_map(|row| row.ok())
                .filter(|path| !Path::new(path).exists())
                .collect()
        };

        let tx = self.conn.transaction()?;
        for path in &stale {
            tx.execute("DELETE FROM files WHERE path = ?1", params![path])?;
        }
        tx.commit()?;
        Ok(stale.len())
    }

    fn read_links(&self, path_str: &str) -> rusqlite::Result<Vec<DocumentLink>> {
        let mut stmt = self.conn.prepare(
            "SELECT kind, target, start_line, start_col, end_line, end_col
             FROM links WHERE file_path = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![path_str], |row| {
            let kind: String = row.get(0)?;
            let target: String = row.get(1)?;
            let start_line: i64 = row.get(2)?;
            let start_col: i64 = row.get(3)?;
            let end_line: i64 = row.get(4)?;
            let end_col: i64 = row.get(5)?;
            Ok((kind, target, start_line, start_col, end_line, end_col))
        })?;

        let mut links = Vec::new();
        for row in rows {
            let (kind, target, start_line, start_col, end_line, end_col) = row?;
            let kind: LinkKind = kind.parse().unwrap_or(LinkKind::File);
            links.push(DocumentLink {
                kind,
                target,
                span: tomet_ast::Span::new(
                    tomet_ast::Position::new(start_line as usize, start_col as usize, 0),
                    tomet_ast::Position::new(end_line as usize, end_col as usize, 0),
                ),
            });
        }
        Ok(links)
    }
}

fn mtime_parts(t: SystemTime) -> (i64, i64) {
    match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => (d.as_secs() as i64, d.subsec_nanos() as i64),
        Err(_) => (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tm_link_cache_{name}_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn first_pass_is_a_miss_second_pass_is_a_hit() {
        let dir = temp_dir("hit_miss");
        let a = dir.join("a.tmt");
        let b = dir.join("b.tmt");
        fs::write(&a, "@link(x.md)\n[A]\n").unwrap();
        fs::write(&b, "@link(y.md)\n[B]\n").unwrap();

        let mut cache = LinkCache::open_in_memory().unwrap();
        assert!(!cache.links_for(&a).unwrap().is_hit());
        assert!(!cache.links_for(&b).unwrap().is_hit());

        let outcome_a = cache.links_for(&a).unwrap();
        let outcome_b = cache.links_for(&b).unwrap();
        assert!(outcome_a.is_hit());
        assert!(outcome_b.is_hit());
        assert_eq!(outcome_a.links()[0].target, "x.md");
        assert_eq!(outcome_b.links()[0].target, "y.md");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pruning_drops_a_deleted_files_row_and_its_links() {
        let dir = temp_dir("prune");
        let kept = dir.join("kept.tmt");
        let gone = dir.join("gone.tmt");
        fs::write(&kept, "@link(x.md)\n[Kept]\n").unwrap();
        fs::write(&gone, "@link(y.md)\n[Gone]\n").unwrap();

        let mut cache = LinkCache::open_in_memory().unwrap();
        cache.links_for(&kept).unwrap();
        cache.links_for(&gone).unwrap();

        fs::remove_file(&gone).unwrap();
        assert_eq!(cache.prune_missing().unwrap(), 1);

        let files: i64 = cache
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(files, 1);

        // The cascade is the half that could silently not happen: it
        // needs `PRAGMA foreign_keys = ON`, which is set per connection
        // and defaults to off.
        let links: i64 = cache
            .conn
            .query_row("SELECT COUNT(*) FROM links", [], |r| r.get(0))
            .unwrap();
        assert_eq!(links, 1);

        assert!(cache.links_for(&kept).unwrap().is_hit());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn touched_file_is_reparsed_untouched_file_stays_cached() {
        let dir = temp_dir("touch");
        let a = dir.join("a.tmt");
        let b = dir.join("b.tmt");
        fs::write(&a, "@link(x.md)\n[A]\n").unwrap();
        fs::write(&b, "@link(y.md)\n[B]\n").unwrap();

        let mut cache = LinkCache::open_in_memory().unwrap();
        cache.links_for(&a).unwrap();
        cache.links_for(&b).unwrap();

        // Edit `a` and bump its mtime well past filesystem mtime
        // granularity (avoids 1-second-resolution flakiness).
        fs::write(&a, "@link(z.md)\n[A updated]\n").unwrap();
        let new_mtime = SystemTime::now() + std::time::Duration::from_secs(2);
        let file = fs::File::open(&a).unwrap();
        file.set_modified(new_mtime).unwrap();

        let outcome_a = cache.links_for(&a).unwrap();
        assert!(!outcome_a.is_hit(), "edited file should be a cache miss");
        assert_eq!(outcome_a.links()[0].target, "z.md");

        let outcome_b = cache.links_for(&b).unwrap();
        assert!(outcome_b.is_hit(), "untouched file should stay cached");
        assert_eq!(outcome_b.links()[0].target, "y.md");

        let _ = fs::remove_dir_all(&dir);
    }
}
