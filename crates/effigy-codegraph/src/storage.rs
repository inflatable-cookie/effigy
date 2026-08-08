use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, MappedRows, OptionalExtension};

use crate::error::CodeGraphError;
use crate::json::GraphCountsPayload;
use crate::model::{
    DiagnosticRecord, EdgeRecord, ExtractorRecord, FileRecord, IndexRunRecord, ReferenceRecord,
    SymbolRecord, GRAPH_STORAGE_SCHEMA_VERSION,
};
use crate::paths::GraphPaths;

const STORAGE_SCHEMA_KEY: &str = "storage_schema_version";
const SOURCE_SEARCH_MAX_BYTES: usize = 131_072;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileScanStateRecord {
    pub path: String,
    pub content_hash: String,
    pub language_id: String,
    pub modified_unix_ms: u128,
    pub byte_size: u64,
}

pub struct GraphStore {
    paths: GraphPaths,
    connection: Connection,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceSearchMatch {
    pub file_id: String,
    pub rank: Option<f64>,
}

impl GraphStore {
    pub fn open(repo_root: &Path) -> Result<Self, CodeGraphError> {
        let paths = GraphPaths::for_repo(repo_root);
        std::fs::create_dir_all(&paths.graph_dir)?;
        let connection = Connection::open(&paths.db_path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let store = Self { paths, connection };
        store.initialize()?;
        Ok(store)
    }

    pub fn paths(&self) -> &GraphPaths {
        &self.paths
    }

    pub fn storage_schema_version(&self) -> Result<u32, CodeGraphError> {
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM metadata WHERE key = ?1",
                [STORAGE_SCHEMA_KEY],
                |row| row.get(0),
            )
            .optional()?;
        match value {
            Some(value) => value.parse::<u32>().map_err(|error| {
                CodeGraphError::validation(format!(
                    "invalid storage schema version in metadata: {error}"
                ))
            }),
            None => Ok(0),
        }
    }

    pub fn counts(&self) -> Result<GraphCountsPayload, CodeGraphError> {
        Ok(GraphCountsPayload {
            files: self.count_rows("files")?,
            symbols: self.count_rows("symbols")?,
            edges: self.count_rows("edges")?,
            references: self.count_rows("graph_references")?,
            diagnostics: self.count_rows("diagnostics")?,
            extractors: self.count_rows("extractors")?,
            index_runs: self.count_rows("index_runs")?,
        })
    }

    pub fn clear_graph_data(&self) -> Result<(), CodeGraphError> {
        self.connection.execute_batch(
            "
            DELETE FROM graph_search;
            DELETE FROM diagnostics;
            DELETE FROM graph_references;
            DELETE FROM edges;
            DELETE FROM symbols;
            DELETE FROM files;
            DELETE FROM index_runs;
            DELETE FROM extractors;
            DELETE FROM file_scan_state;
            ",
        )?;
        Ok(())
    }

    pub fn save_extractor(&self, record: &ExtractorRecord) -> Result<(), CodeGraphError> {
        record.validate()?;
        self.connection.execute(
            "INSERT OR REPLACE INTO extractors (id, version, languages_json, capabilities_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                record.id.as_str(),
                record.version.as_str(),
                serde_json::to_string(&record.language_ids)?,
                serde_json::to_string(&record.capabilities)?,
            ],
        )?;
        Ok(())
    }

    pub fn save_index_run(&self, record: &IndexRunRecord) -> Result<(), CodeGraphError> {
        record.validate()?;
        self.connection.execute(
            "INSERT OR REPLACE INTO index_runs
             (id, repo_root, schema_version, started_at, finished_at, file_count, symbol_count, edge_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.id.as_str(),
                record.repo_root.as_str(),
                record.schema_version,
                record.started_at.as_str(),
                record.finished_at.as_deref(),
                record.file_count as i64,
                record.symbol_count as i64,
                record.edge_count as i64,
            ],
        )?;
        Ok(())
    }

    pub fn save_file(&self, record: &FileRecord) -> Result<(), CodeGraphError> {
        record.validate()?;
        self.connection.execute(
            "INSERT OR REPLACE INTO files
             (id, path, content_hash, language_id, byte_size, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.id.as_str(),
                record.path.as_str(),
                record.content_hash.as_str(),
                record.language_id.as_str(),
                record.byte_size as i64,
                serde_json::to_string(&record.status)?,
            ],
        )?;
        Ok(())
    }

    pub fn save_symbol(&self, record: &SymbolRecord) -> Result<(), CodeGraphError> {
        record.validate()?;
        self.connection.execute(
            "INSERT OR REPLACE INTO symbols
             (id, kind, display_name, canonical_name, file_id, span_json, provenance_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.id.as_str(),
                record.kind.as_str(),
                record.display_name.as_str(),
                record.canonical_name.as_str(),
                record.file_id.as_str(),
                serde_json::to_string(&record.span)?,
                serde_json::to_string(&record.provenance)?,
            ],
        )?;
        Ok(())
    }

    pub fn save_edge(&self, record: &EdgeRecord) -> Result<(), CodeGraphError> {
        record.validate()?;
        self.connection.execute(
            "INSERT OR REPLACE INTO edges
             (id, kind, from_id, to_id, unresolved_target, provenance_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.id.as_str(),
                record.kind.as_str(),
                record.from_id.as_str(),
                record.to_id.as_ref().map(|id| id.as_str()),
                record.unresolved_target.as_deref(),
                serde_json::to_string(&record.provenance)?,
            ],
        )?;
        Ok(())
    }

    pub fn save_reference(&self, record: &ReferenceRecord) -> Result<(), CodeGraphError> {
        record.validate()?;
        self.connection.execute(
            "INSERT OR REPLACE INTO graph_references
             (id, file_id, kind, target_id, unresolved_target, span_json, provenance_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.id.as_str(),
                record.file_id.as_str(),
                record.kind.as_str(),
                record.target_id.as_ref().map(|id| id.as_str()),
                record.unresolved_target.as_deref(),
                serde_json::to_string(&record.span)?,
                serde_json::to_string(&record.provenance)?,
            ],
        )?;
        Ok(())
    }

    pub fn save_diagnostic(&self, record: &DiagnosticRecord) -> Result<(), CodeGraphError> {
        record.validate()?;
        self.connection.execute(
            "INSERT OR REPLACE INTO diagnostics
             (id, severity, message, file_id, span_json, provenance_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.id.as_str(),
                serde_json::to_string(&record.severity)?,
                record.message.as_str(),
                record.file_id.as_ref().map(|id| id.as_str()),
                record
                    .span
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                serde_json::to_string(&record.provenance)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_extractors(&self) -> Result<Vec<ExtractorRecord>, CodeGraphError> {
        let mut statement = self.connection.prepare(
            "SELECT id, version, languages_json, capabilities_json
             FROM extractors
             ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ExtractorRecord {
                id: crate::ExtractorId::new(row.get::<_, String>(0)?)
                    .map_err(to_sql_conversion_error)?,
                version: row.get(1)?,
                language_ids: serde_json::from_str(&row.get::<_, String>(2)?)
                    .map_err(to_sql_conversion_error)?,
                capabilities: serde_json::from_str(&row.get::<_, String>(3)?)
                    .map_err(to_sql_conversion_error)?,
            })
        })?;
        collect_rows(rows)
    }

    pub fn list_index_runs(&self) -> Result<Vec<IndexRunRecord>, CodeGraphError> {
        let mut statement = self.connection.prepare(
            "SELECT id, repo_root, schema_version, started_at, finished_at, file_count, symbol_count, edge_count
             FROM index_runs
             ORDER BY started_at, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(IndexRunRecord {
                id: crate::GraphId::new(row.get::<_, String>(0)?)
                    .map_err(to_sql_conversion_error)?,
                repo_root: row.get(1)?,
                schema_version: row.get(2)?,
                started_at: row.get(3)?,
                finished_at: row.get(4)?,
                file_count: row.get::<_, i64>(5)? as u64,
                symbol_count: row.get::<_, i64>(6)? as u64,
                edge_count: row.get::<_, i64>(7)? as u64,
            })
        })?;
        collect_rows(rows)
    }

    pub fn list_files(&self) -> Result<Vec<FileRecord>, CodeGraphError> {
        let mut statement = self.connection.prepare(
            "SELECT id, path, content_hash, language_id, byte_size, status
             FROM files
             ORDER BY path",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(FileRecord {
                id: crate::GraphId::new(row.get::<_, String>(0)?)
                    .map_err(to_sql_conversion_error)?,
                path: row.get(1)?,
                content_hash: row.get(2)?,
                language_id: row.get(3)?,
                byte_size: row.get::<_, i64>(4)? as u64,
                status: serde_json::from_str(&row.get::<_, String>(5)?)
                    .map_err(to_sql_conversion_error)?,
            })
        })?;
        collect_rows(rows)
    }

    pub fn list_symbols(&self) -> Result<Vec<SymbolRecord>, CodeGraphError> {
        let mut statement = self.connection.prepare(
            "SELECT id, kind, display_name, canonical_name, file_id, span_json, provenance_json
             FROM symbols
             ORDER BY canonical_name, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SymbolRecord {
                id: crate::GraphId::new(row.get::<_, String>(0)?)
                    .map_err(to_sql_conversion_error)?,
                kind: row.get(1)?,
                display_name: row.get(2)?,
                canonical_name: row.get(3)?,
                file_id: crate::GraphId::new(row.get::<_, String>(4)?)
                    .map_err(to_sql_conversion_error)?,
                span: serde_json::from_str(&row.get::<_, String>(5)?)
                    .map_err(to_sql_conversion_error)?,
                provenance: serde_json::from_str(&row.get::<_, String>(6)?)
                    .map_err(to_sql_conversion_error)?,
            })
        })?;
        collect_rows(rows)
    }

    pub fn list_edges(&self) -> Result<Vec<EdgeRecord>, CodeGraphError> {
        let mut statement = self.connection.prepare(
            "SELECT id, kind, from_id, to_id, unresolved_target, provenance_json
             FROM edges
             ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            let to_id: Option<String> = row.get(3)?;
            Ok(EdgeRecord {
                id: crate::GraphId::new(row.get::<_, String>(0)?)
                    .map_err(to_sql_conversion_error)?,
                kind: row.get(1)?,
                from_id: crate::GraphId::new(row.get::<_, String>(2)?)
                    .map_err(to_sql_conversion_error)?,
                to_id: to_id
                    .map(crate::GraphId::new)
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                unresolved_target: row.get(4)?,
                provenance: serde_json::from_str(&row.get::<_, String>(5)?)
                    .map_err(to_sql_conversion_error)?,
            })
        })?;
        collect_rows(rows)
    }

    pub fn list_references(&self) -> Result<Vec<ReferenceRecord>, CodeGraphError> {
        let mut statement = self.connection.prepare(
            "SELECT id, file_id, kind, target_id, unresolved_target, span_json, provenance_json
             FROM graph_references
             ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            let target_id: Option<String> = row.get(3)?;
            Ok(ReferenceRecord {
                id: crate::GraphId::new(row.get::<_, String>(0)?)
                    .map_err(to_sql_conversion_error)?,
                file_id: crate::GraphId::new(row.get::<_, String>(1)?)
                    .map_err(to_sql_conversion_error)?,
                kind: row.get(2)?,
                target_id: target_id
                    .map(crate::GraphId::new)
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                unresolved_target: row.get(4)?,
                span: serde_json::from_str(&row.get::<_, String>(5)?)
                    .map_err(to_sql_conversion_error)?,
                provenance: serde_json::from_str(&row.get::<_, String>(6)?)
                    .map_err(to_sql_conversion_error)?,
            })
        })?;
        collect_rows(rows)
    }

    pub fn list_diagnostics(&self) -> Result<Vec<DiagnosticRecord>, CodeGraphError> {
        let mut statement = self.connection.prepare(
            "SELECT id, severity, message, file_id, span_json, provenance_json
             FROM diagnostics
             ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            let file_id: Option<String> = row.get(3)?;
            let span_json: Option<String> = row.get(4)?;
            Ok(DiagnosticRecord {
                id: crate::GraphId::new(row.get::<_, String>(0)?)
                    .map_err(to_sql_conversion_error)?,
                severity: serde_json::from_str(&row.get::<_, String>(1)?)
                    .map_err(to_sql_conversion_error)?,
                message: row.get(2)?,
                file_id: file_id
                    .map(crate::GraphId::new)
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                span: span_json
                    .map(|value| serde_json::from_str(&value))
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                provenance: serde_json::from_str(&row.get::<_, String>(5)?)
                    .map_err(to_sql_conversion_error)?,
            })
        })?;
        collect_rows(rows)
    }

    pub fn search_table_present(&self) -> Result<bool, CodeGraphError> {
        let present: Option<i64> = self
            .connection
            .query_row(
                "SELECT 1
                 FROM sqlite_master
                 WHERE type = 'table' AND name = 'graph_search'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(present.is_some())
    }

    pub fn save_file_scan_state(&self, record: &FileScanStateRecord) -> Result<(), CodeGraphError> {
        self.connection.execute(
            "INSERT OR REPLACE INTO file_scan_state
             (path, content_hash, language_id, modified_unix_ms, byte_size)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.path.as_str(),
                record.content_hash.as_str(),
                record.language_id.as_str(),
                record.modified_unix_ms.to_string(),
                record.byte_size as i64,
            ],
        )?;
        Ok(())
    }

    pub fn delete_file_scan_state(&self, path: &str) -> Result<(), CodeGraphError> {
        self.connection
            .execute("DELETE FROM file_scan_state WHERE path = ?1", params![path])?;
        Ok(())
    }

    pub fn delete_file_graph(&self, file_id: &str) -> Result<(), CodeGraphError> {
        self.connection.execute(
            "DELETE FROM graph_references
             WHERE file_id = ?1
                OR target_id IN (SELECT id FROM symbols WHERE file_id = ?1)",
            params![file_id],
        )?;
        self.connection.execute(
            "DELETE FROM edges
             WHERE from_id IN (SELECT id FROM symbols WHERE file_id = ?1)
                OR to_id IN (SELECT id FROM symbols WHERE file_id = ?1)",
            params![file_id],
        )?;
        self.connection.execute(
            "DELETE FROM diagnostics WHERE file_id = ?1",
            params![file_id],
        )?;
        self.connection
            .execute("DELETE FROM symbols WHERE file_id = ?1", params![file_id])?;
        self.connection
            .execute("DELETE FROM files WHERE id = ?1", params![file_id])?;
        Ok(())
    }

    pub fn file_scan_state_map(
        &self,
    ) -> Result<std::collections::BTreeMap<String, FileScanStateRecord>, CodeGraphError> {
        let mut statement = self.connection.prepare(
            "SELECT path, content_hash, language_id, modified_unix_ms, byte_size
             FROM file_scan_state
             ORDER BY path",
        )?;
        let rows = statement.query_map([], |row| {
            let modified: String = row.get(3)?;
            Ok(FileScanStateRecord {
                path: row.get(0)?,
                content_hash: row.get(1)?,
                language_id: row.get(2)?,
                modified_unix_ms: modified.parse::<u128>().map_err(to_sql_conversion_error)?,
                byte_size: row.get::<_, i64>(4)? as u64,
            })
        })?;
        let mut map = std::collections::BTreeMap::new();
        for row in rows {
            let record = row?;
            map.insert(record.path.clone(), record);
        }
        Ok(map)
    }

    pub fn extractor_version_map(
        &self,
    ) -> Result<std::collections::BTreeMap<String, String>, CodeGraphError> {
        let mut map = std::collections::BTreeMap::new();
        for extractor in self.list_extractors()? {
            for language in extractor.language_ids {
                map.insert(language, extractor.version.clone());
            }
        }
        Ok(map)
    }

    pub fn failed_diagnostic_paths(&self) -> Result<Vec<String>, CodeGraphError> {
        Ok(self
            .list_diagnostics()?
            .into_iter()
            .filter(|diagnostic| diagnostic.severity == crate::model::DiagnosticSeverity::Error)
            .map(|diagnostic| diagnostic.provenance.source_path)
            .collect())
    }

    pub fn refresh_search_index(&self) -> Result<(), CodeGraphError> {
        self.connection.execute("DELETE FROM graph_search", [])?;
        for file in self.list_files()? {
            self.connection.execute(
                "INSERT INTO graph_search (record_type, record_id, text) VALUES (?1, ?2, ?3)",
                params!["file", file.id.as_str(), file.path],
            )?;
            if let Some(source_text) = source_search_text(&self.paths.repo_root, &file) {
                self.connection.execute(
                    "INSERT INTO graph_search (record_type, record_id, text) VALUES (?1, ?2, ?3)",
                    params!["source", file.id.as_str(), source_text],
                )?;
            }
        }
        for symbol in self.list_symbols()? {
            self.connection.execute(
                "INSERT INTO graph_search (record_type, record_id, text) VALUES (?1, ?2, ?3)",
                params![
                    "symbol",
                    symbol.id.as_str(),
                    format!("{} {}", symbol.display_name, symbol.canonical_name),
                ],
            )?;
        }
        for diagnostic in self.list_diagnostics()? {
            self.connection.execute(
                "INSERT INTO graph_search (record_type, record_id, text) VALUES (?1, ?2, ?3)",
                params!["diagnostic", diagnostic.id.as_str(), diagnostic.message],
            )?;
        }
        Ok(())
    }

    pub fn find_file_by_id(&self, id: &str) -> Result<Option<FileRecord>, CodeGraphError> {
        self.connection
            .query_row(
                "SELECT id, path, content_hash, language_id, byte_size, status
                 FROM files
                 WHERE id = ?1",
                params![id],
                |row| {
                    Ok(FileRecord {
                        id: crate::GraphId::new(row.get::<_, String>(0)?)
                            .map_err(to_sql_conversion_error)?,
                        path: row.get(1)?,
                        content_hash: row.get(2)?,
                        language_id: row.get(3)?,
                        byte_size: row.get::<_, i64>(4)? as u64,
                        status: serde_json::from_str(&row.get::<_, String>(5)?)
                            .map_err(to_sql_conversion_error)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn find_symbol_by_id(&self, id: &str) -> Result<Option<SymbolRecord>, CodeGraphError> {
        self.connection
            .query_row(
                "SELECT id, kind, display_name, canonical_name, file_id, span_json, provenance_json
                 FROM symbols
                 WHERE id = ?1",
                params![id],
                |row| {
                    Ok(SymbolRecord {
                        id: crate::GraphId::new(row.get::<_, String>(0)?)
                            .map_err(to_sql_conversion_error)?,
                        kind: row.get(1)?,
                        display_name: row.get(2)?,
                        canonical_name: row.get(3)?,
                        file_id: crate::GraphId::new(row.get::<_, String>(4)?)
                            .map_err(to_sql_conversion_error)?,
                        span: serde_json::from_str(&row.get::<_, String>(5)?)
                            .map_err(to_sql_conversion_error)?,
                        provenance: serde_json::from_str(&row.get::<_, String>(6)?)
                            .map_err(to_sql_conversion_error)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(String, String, Option<f64>)>, CodeGraphError> {
        let mut statement = self.connection.prepare(
            "SELECT record_type, record_id, bm25(graph_search)
             FROM graph_search
             WHERE graph_search MATCH ?1
               AND record_type != 'source'
             ORDER BY bm25(graph_search), record_id
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![query, limit as i64], |row| {
            Ok((row.get(0)?, row.get(1)?, Some(row.get(2)?)))
        })?;
        collect_rows(rows)
    }

    pub fn source_search(
        &self,
        token: &str,
        limit: usize,
    ) -> Result<Vec<SourceSearchMatch>, CodeGraphError> {
        let mut statement = self.connection.prepare(
            "SELECT record_id, bm25(graph_search)
             FROM graph_search
             WHERE graph_search MATCH ?1
               AND record_type = 'source'
             ORDER BY bm25(graph_search), record_id
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![fts_phrase(token), limit as i64], |row| {
            Ok(SourceSearchMatch {
                file_id: row.get(0)?,
                rank: Some(row.get(1)?),
            })
        })?;
        collect_rows(rows)
    }

    #[cfg(test)]
    pub(crate) fn journal_mode(&self) -> Result<String, CodeGraphError> {
        self.connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(Into::into)
    }

    fn initialize(&self) -> Result<(), CodeGraphError> {
        self.configure_connection()?;
        self.connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS extractors (
                id TEXT PRIMARY KEY,
                version TEXT NOT NULL,
                languages_json TEXT NOT NULL,
                capabilities_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS index_runs (
                id TEXT PRIMARY KEY,
                repo_root TEXT NOT NULL,
                schema_version INTEGER NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                file_count INTEGER NOT NULL,
                symbol_count INTEGER NOT NULL,
                edge_count INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS files (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                content_hash TEXT NOT NULL,
                language_id TEXT NOT NULL,
                byte_size INTEGER NOT NULL,
                status TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS file_scan_state (
                path TEXT PRIMARY KEY,
                content_hash TEXT NOT NULL,
                language_id TEXT NOT NULL,
                modified_unix_ms TEXT NOT NULL,
                byte_size INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS symbols (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                display_name TEXT NOT NULL,
                canonical_name TEXT NOT NULL,
                file_id TEXT NOT NULL,
                span_json TEXT NOT NULL,
                provenance_json TEXT NOT NULL,
                FOREIGN KEY(file_id) REFERENCES files(id)
            );

            CREATE TABLE IF NOT EXISTS edges (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                from_id TEXT NOT NULL,
                to_id TEXT,
                unresolved_target TEXT,
                provenance_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS graph_references (
                id TEXT PRIMARY KEY,
                file_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                target_id TEXT,
                unresolved_target TEXT,
                span_json TEXT NOT NULL,
                provenance_json TEXT NOT NULL,
                FOREIGN KEY(file_id) REFERENCES files(id)
            );

            CREATE TABLE IF NOT EXISTS diagnostics (
                id TEXT PRIMARY KEY,
                severity TEXT NOT NULL,
                message TEXT NOT NULL,
                file_id TEXT,
                span_json TEXT,
                provenance_json TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS graph_search USING fts5(
                record_type,
                record_id,
                text
            );
            ",
        )?;
        self.apply_storage_migrations()?;
        self.write_storage_schema_version(GRAPH_STORAGE_SCHEMA_VERSION)?;
        Ok(())
    }

    fn count_rows(&self, table: &str) -> Result<usize, CodeGraphError> {
        let statement = format!("SELECT COUNT(*) FROM {table}");
        let count = self
            .connection
            .query_row(&statement, [], |row| row.get::<_, i64>(0))?;
        Ok(count.max(0) as usize)
    }
}

impl GraphStore {
    fn configure_connection(&self) -> Result<(), CodeGraphError> {
        self.connection.pragma_update(None, "foreign_keys", "ON")?;
        self.connection
            .pragma_update(None, "synchronous", "NORMAL")?;
        self.connection
            .pragma_update(None, "temp_store", "MEMORY")?;
        let _ = self
            .connection
            .query_row("PRAGMA journal_mode = WAL", [], |row| {
                row.get::<_, String>(0)
            });
        Ok(())
    }

    fn apply_storage_migrations(&self) -> Result<(), CodeGraphError> {
        let stored_version = self.read_stored_storage_schema_version()?;
        if stored_version > GRAPH_STORAGE_SCHEMA_VERSION {
            return Err(CodeGraphError::validation(format!(
                "graph storage schema {stored_version} is newer than supported schema {}",
                GRAPH_STORAGE_SCHEMA_VERSION
            )));
        }
        if stored_version < 2 {
            self.migrate_to_v2_source_search_backfill()?;
        }
        Ok(())
    }

    fn migrate_to_v2_source_search_backfill(&self) -> Result<(), CodeGraphError> {
        if self.count_rows("files")? == 0 || self.count_graph_search_records("source")? > 0 {
            return Ok(());
        }
        self.refresh_search_index()
    }

    fn read_stored_storage_schema_version(&self) -> Result<u32, CodeGraphError> {
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM metadata WHERE key = ?1",
                [STORAGE_SCHEMA_KEY],
                |row| row.get(0),
            )
            .optional()?;
        match value {
            Some(value) => value.parse::<u32>().map_err(|error| {
                CodeGraphError::validation(format!(
                    "invalid storage schema version in metadata: {error}"
                ))
            }),
            None => Ok(0),
        }
    }

    fn write_storage_schema_version(&self, version: u32) -> Result<(), CodeGraphError> {
        self.connection.execute(
            "INSERT INTO metadata (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![STORAGE_SCHEMA_KEY, version.to_string()],
        )?;
        Ok(())
    }

    pub(crate) fn metadata_value(&self, key: &str) -> Result<Option<String>, CodeGraphError> {
        Ok(self
            .connection
            .query_row("SELECT value FROM metadata WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub(crate) fn save_metadata(&self, key: &str, value: &str) -> Result<(), CodeGraphError> {
        self.connection.execute(
            "INSERT INTO metadata (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub(crate) fn delete_metadata(&self, key: &str) -> Result<(), CodeGraphError> {
        self.connection
            .execute("DELETE FROM metadata WHERE key = ?1", [key])?;
        Ok(())
    }

    fn count_graph_search_records(&self, record_type: &str) -> Result<usize, CodeGraphError> {
        let count = self.connection.query_row(
            "SELECT COUNT(*) FROM graph_search WHERE record_type = ?1",
            [record_type],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count.max(0) as usize)
    }
}

fn collect_rows<T, F>(rows: MappedRows<'_, F>) -> Result<Vec<T>, CodeGraphError>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut collected = Vec::new();
    for row in rows {
        collected.push(row?);
    }
    Ok(collected)
}

fn to_sql_conversion_error(error: impl std::fmt::Display) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error.to_string(),
        )),
    )
}

fn source_search_text(repo_root: &Path, file: &FileRecord) -> Option<String> {
    if file.byte_size > SOURCE_SEARCH_MAX_BYTES as u64 {
        return None;
    }
    let content = std::fs::read_to_string(repo_root.join(&file.path)).ok()?;
    let normalized = if strips_comment_only_lines(&file.path, &file.language_id) {
        content
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !(trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*'))
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        content
    };
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn strips_comment_only_lines(path: &str, language_id: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    language_id != "markdown"
        && language_id != "toml"
        && !lower.starts_with("docs/")
        && !lower.ends_with(".md")
}

fn fts_phrase(token: &str) -> String {
    format!("\"{}\"", token.replace('"', "\"\""))
}
