use rusqlite::{params, Connection, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize, Clone)]
pub struct AppInfo {
    pub id: i64,
    pub name: String,
    pub exe_path: String,
    pub icon_base64: Option<String>,
    pub entry_count: i64,
    pub is_favorite: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct ClipboardEntry {
    pub id: i64,
    pub app_id: i64,
    pub content_type: String,
    pub text_content: Option<String>,
    pub image_path: Option<String>,
    pub created_at: String,
    pub source_url: Option<String>,
    pub is_favorite: bool,
    pub is_sensitive: bool,
    pub html_content: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeletedEntry {
    pub id: i64,
    pub app_id: i64,
    pub content_type: String,
    pub text_content: Option<String>,
    pub image_path: Option<String>,
    pub created_at: String,
    pub content_hash: Option<String>,
    pub source_url: Option<String>,
    pub is_favorite: i64,
    pub is_sensitive: i64,
    pub html_content: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SourceInfo {
    pub domain: String,
    pub count: i64,
}

pub fn extract_domain(url: &str) -> String {
    let url = url.trim();
    let after_scheme = if let Some(pos) = url.find("://") {
        &url[pos + 3..]
    } else {
        url
    };
    let host = after_scheme
        .split('/')
        .next()
        .unwrap_or(after_scheme)
        .split('?')
        .next()
        .unwrap_or(after_scheme)
        .split(':')
        .next()
        .unwrap_or(after_scheme);
    let host = host.strip_prefix("www.").unwrap_or(host);
    extract_base_domain(host)
}

fn extract_base_domain(host: &str) -> String {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() <= 2 {
        return host.to_lowercase();
    }

    static MULTI_PART_TLDS: &[&str] = &[
        "co.uk", "co.jp", "co.kr", "co.nz", "co.za", "co.in", "co.id", "co.th",
        "com.cn", "com.tw", "com.hk", "com.sg", "com.au", "com.br", "com.mx",
        "com.ar", "com.tr", "com.ua", "com.my", "com.ph", "com.vn", "com.pk",
        "org.cn", "org.uk", "org.au", "org.tw", "org.hk",
        "net.cn", "net.au", "net.tw",
        "gov.cn", "gov.uk", "gov.au",
        "edu.cn", "edu.au", "edu.tw", "edu.hk",
        "ac.uk", "ac.jp", "ac.kr", "ac.cn",
    ];

    let len = parts.len();
    let last_two = format!("{}.{}", parts[len - 2], parts[len - 1]).to_lowercase();

    for tld in MULTI_PART_TLDS {
        if last_two == *tld && len >= 3 {
            return parts[len - 3..].join(".").to_lowercase();
        }
    }

    parts[len - 2..].join(".").to_lowercase()
}

const DOMAIN_FILTER_SQL: &str = "(source_url LIKE '%://' || ?{d} || '/%' OR source_url LIKE '%://' || ?{d} OR source_url LIKE '%://%.' || ?{d} || '/%' OR source_url LIKE '%://%.' || ?{d})";

pub struct Database {
    conn: Connection,
    data_dir: std::path::PathBuf,
}

impl Database {
    pub fn new(data_dir: &Path) -> Result<Self> {
        let db_path = data_dir.join("cutboard.db");
        let images_dir = data_dir.join("images");
        std::fs::create_dir_all(&images_dir)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS apps (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                exe_path TEXT NOT NULL UNIQUE,
                icon_base64 TEXT
            );
            CREATE TABLE IF NOT EXISTS clipboard_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                app_id INTEGER NOT NULL REFERENCES apps(id),
                content_type TEXT NOT NULL,
                text_content TEXT,
                image_path TEXT,
                content_hash TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
            );
            CREATE INDEX IF NOT EXISTS idx_entries_app ON clipboard_entries(app_id);
            CREATE INDEX IF NOT EXISTS idx_entries_type ON clipboard_entries(content_type);
            CREATE INDEX IF NOT EXISTS idx_entries_created ON clipboard_entries(created_at);",
        )?;

        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(clipboard_entries)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>>>()?;

        if !columns.iter().any(|c| c == "content_hash") {
            conn.execute("ALTER TABLE clipboard_entries ADD COLUMN content_hash TEXT", [])?;
        }
        if !columns.iter().any(|c| c == "source_url") {
            conn.execute("ALTER TABLE clipboard_entries ADD COLUMN source_url TEXT", [])?;
        }
        if !columns.iter().any(|c| c == "is_favorite") {
            conn.execute("ALTER TABLE clipboard_entries ADD COLUMN is_favorite INTEGER DEFAULT 0", [])?;
        }
        if !columns.iter().any(|c| c == "is_sensitive") {
            conn.execute("ALTER TABLE clipboard_entries ADD COLUMN is_sensitive INTEGER DEFAULT 0", [])?;
        }
        if !columns.iter().any(|c| c == "html_content") {
            conn.execute("ALTER TABLE clipboard_entries ADD COLUMN html_content TEXT", [])?;
        }

        // Migrate apps table
        let app_columns: Vec<String> = conn
            .prepare("PRAGMA table_info(apps)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>>>()?;
        if !app_columns.iter().any(|c| c == "is_favorite") {
            conn.execute("ALTER TABLE apps ADD COLUMN is_favorite INTEGER DEFAULT 0", [])?;
        }

        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_entries_hash ON clipboard_entries(content_hash);
             CREATE INDEX IF NOT EXISTS idx_entries_app_type_hash ON clipboard_entries(app_id, content_type, content_hash);",
        )?;

        Ok(Self {
            conn,
            data_dir: data_dir.to_path_buf(),
        })
    }

    pub fn db_path(&self) -> std::path::PathBuf {
        self.data_dir.join("cutboard.db")
    }

    pub fn images_dir(&self) -> std::path::PathBuf {
        self.data_dir.join("images")
    }

    pub fn get_or_create_app(
        &self,
        name: &str,
        exe_path: &str,
        icon_base64: Option<&str>,
    ) -> Result<i64> {
        if let Ok(id) = self.conn.query_row(
            "SELECT id FROM apps WHERE exe_path = ?1",
            params![exe_path],
            |row| row.get::<_, i64>(0),
        ) {
            if let Some(icon) = icon_base64 {
                self.conn.execute(
                    "UPDATE apps SET icon_base64 = ?1 WHERE id = ?2 AND icon_base64 IS NULL",
                    params![icon, id],
                )?;
            }
            return Ok(id);
        }

        self.conn.execute(
            "INSERT INTO apps (name, exe_path, icon_base64) VALUES (?1, ?2, ?3)",
            params![name, exe_path, icon_base64],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn upsert_text_entry(&self, app_id: i64, text: &str, hash: &str, source_url: Option<&str>) -> Result<i64> {
        if let Ok(id) = self.conn.query_row(
            "SELECT id FROM clipboard_entries WHERE app_id = ?1 AND content_type = 'text' AND content_hash = ?2",
            params![app_id, hash],
            |row| row.get::<_, i64>(0),
        ) {
            self.conn.execute(
                "UPDATE clipboard_entries SET created_at = datetime('now', 'localtime'), source_url = COALESCE(?2, source_url) WHERE id = ?1",
                params![id, source_url],
            )?;
            return Ok(id);
        }

        self.conn.execute(
            "INSERT INTO clipboard_entries (app_id, content_type, text_content, content_hash, source_url) VALUES (?1, 'text', ?2, ?3, ?4)",
            params![app_id, text, hash, source_url],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn upsert_image_entry(&self, app_id: i64, image_filename: &str, hash: &str, source_url: Option<&str>) -> Result<(i64, bool)> {
        if let Ok(id) = self.conn.query_row(
            "SELECT id FROM clipboard_entries WHERE app_id = ?1 AND content_type = 'image' AND content_hash = ?2",
            params![app_id, hash],
            |row| row.get::<_, i64>(0),
        ) {
            self.conn.execute(
                "UPDATE clipboard_entries SET created_at = datetime('now', 'localtime'), source_url = COALESCE(?2, source_url) WHERE id = ?1",
                params![id, source_url],
            )?;
            return Ok((id, true));
        }

        self.conn.execute(
            "INSERT INTO clipboard_entries (app_id, content_type, image_path, content_hash, source_url) VALUES (?1, 'image', ?2, ?3, ?4)",
            params![app_id, image_filename, hash, source_url],
        )?;
        Ok((self.conn.last_insert_rowid(), false))
    }

    pub fn get_apps(&self) -> Result<Vec<AppInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.name, a.exe_path, a.icon_base64, COUNT(e.id) as cnt, COALESCE(a.is_favorite, 0)
             FROM apps a
             LEFT JOIN clipboard_entries e ON e.app_id = a.id
             GROUP BY a.id
             ORDER BY a.is_favorite DESC, cnt DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AppInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                exe_path: row.get(2)?,
                icon_base64: row.get(3)?,
                entry_count: row.get(4)?,
                is_favorite: row.get::<_, i64>(5)? != 0,
            })
        })?;
        rows.collect()
    }

    pub fn get_entry_counts(&self, app_id: i64, source_domain: &str) -> Result<(i64, i64)> {
        if source_domain.is_empty() {
            self.conn.query_row(
                "SELECT
                    SUM(CASE WHEN content_type = 'text' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN content_type = 'image' THEN 1 ELSE 0 END)
                 FROM clipboard_entries WHERE app_id = ?1",
                params![app_id],
                |row| Ok((row.get::<_, Option<i64>>(0)?.unwrap_or(0), row.get::<_, Option<i64>>(1)?.unwrap_or(0))),
            )
        } else {
            self.conn.query_row(
                &format!("SELECT
                    SUM(CASE WHEN content_type = 'text' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN content_type = 'image' THEN 1 ELSE 0 END)
                 FROM clipboard_entries WHERE app_id = ?1 AND {}", DOMAIN_FILTER_SQL.replace("{d}", "2")),
                params![app_id, source_domain],
                |row| Ok((row.get::<_, Option<i64>>(0)?.unwrap_or(0), row.get::<_, Option<i64>>(1)?.unwrap_or(0))),
            )
        }
    }

    pub fn get_entries(
        &self,
        app_id: i64,
        content_type: &str,
        search: &str,
        source_domain: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<ClipboardEntry>> {
        let base = "SELECT id, app_id, content_type, text_content, image_path, created_at, source_url, COALESCE(is_favorite,0), COALESCE(is_sensitive,0), html_content FROM clipboard_entries WHERE app_id = ?1 AND content_type = ?2";
        let domain_filter = &format!(" AND {}", DOMAIN_FILTER_SQL);
        let order = " ORDER BY is_favorite DESC, created_at DESC";
        let offset = (page - 1) * page_size;

        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<ClipboardEntry> {
            Ok(ClipboardEntry {
                id: row.get(0)?,
                app_id: row.get(1)?,
                content_type: row.get(2)?,
                text_content: row.get(3)?,
                image_path: row.get(4)?,
                created_at: row.get(5)?,
                source_url: row.get(6)?,
                is_favorite: row.get::<_, i64>(7)? != 0,
                is_sensitive: row.get::<_, i64>(8)? != 0,
                html_content: row.get(9)?,
            })
        };

        match (search.is_empty(), source_domain.is_empty()) {
            (true, true) => {
                let q = format!("{}{} LIMIT ?3 OFFSET ?4", base, order);
                self.conn.prepare(&q)?.query_map(params![app_id, content_type, page_size, offset], map_row)?.collect()
            }
            (false, true) => {
                let q = format!("{} AND text_content LIKE '%' || ?3 || '%'{} LIMIT ?4 OFFSET ?5", base, order);
                self.conn.prepare(&q)?.query_map(params![app_id, content_type, search, page_size, offset], map_row)?.collect()
            }
            (true, false) => {
                let q = format!("{}{}{} LIMIT ?4 OFFSET ?5", base, domain_filter.replace("{d}", "3"), order);
                self.conn.prepare(&q)?.query_map(params![app_id, content_type, source_domain, page_size, offset], map_row)?.collect()
            }
            (false, false) => {
                let q = format!("{} AND text_content LIKE '%' || ?3 || '%'{}{} LIMIT ?5 OFFSET ?6", base, domain_filter.replace("{d}", "4"), order);
                self.conn.prepare(&q)?.query_map(params![app_id, content_type, search, source_domain, page_size, offset], map_row)?.collect()
            }
        }
    }

    pub fn get_entry_by_id(&self, id: i64) -> Result<ClipboardEntry> {
        self.conn.query_row(
            "SELECT id, app_id, content_type, text_content, image_path, created_at, source_url, COALESCE(is_favorite,0), COALESCE(is_sensitive,0), html_content
             FROM clipboard_entries WHERE id = ?1",
            params![id],
            |row| {
                Ok(ClipboardEntry {
                    id: row.get(0)?,
                    app_id: row.get(1)?,
                    content_type: row.get(2)?,
                    text_content: row.get(3)?,
                    image_path: row.get(4)?,
                    created_at: row.get(5)?,
                    source_url: row.get(6)?,
                    is_favorite: row.get::<_, i64>(7)? != 0,
                    is_sensitive: row.get::<_, i64>(8)? != 0,
                    html_content: row.get(9)?,
                })
            },
        )
    }

    pub fn get_source_urls(&self, app_id: i64) -> Result<Vec<SourceInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT source_url, COUNT(*) as cnt FROM clipboard_entries
             WHERE app_id = ?1 AND source_url IS NOT NULL AND source_url != ''
             GROUP BY source_url ORDER BY cnt DESC",
        )?;
        let rows = stmt
            .query_map(params![app_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>>>()?;

        let mut domain_counts: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for (url, count) in rows {
            let domain = extract_domain(&url);
            *domain_counts.entry(domain).or_insert(0) += count;
        }

        let mut result: Vec<SourceInfo> = domain_counts
            .into_iter()
            .map(|(domain, count)| SourceInfo { domain, count })
            .collect();
        result.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.domain.cmp(&b.domain)));
        Ok(result)
    }

    pub fn get_entry_full(&self, id: i64) -> Result<Option<DeletedEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, app_id, content_type, text_content, image_path, created_at, \
             content_hash, source_url, is_favorite, is_sensitive, html_content \
             FROM clipboard_entries WHERE id = ?1"
        )?;
        let entry = stmt.query_row(params![id], |row| {
            Ok(DeletedEntry {
                id: row.get(0)?,
                app_id: row.get(1)?,
                content_type: row.get(2)?,
                text_content: row.get(3)?,
                image_path: row.get(4)?,
                created_at: row.get(5)?,
                content_hash: row.get(6)?,
                source_url: row.get(7)?,
                is_favorite: row.get(8)?,
                is_sensitive: row.get(9)?,
                html_content: row.get(10)?,
            })
        }).ok();
        Ok(entry)
    }

    pub fn delete_entry(&self, id: i64) -> Result<Option<String>> {
        let image_path: Option<String> = self
            .conn
            .query_row(
                "SELECT image_path FROM clipboard_entries WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .ok();

        self.conn.execute(
            "DELETE FROM clipboard_entries WHERE id = ?1",
            params![id],
        )?;

        self.cleanup_empty_apps()?;
        Ok(image_path)
    }

    pub fn restore_entry(&self, entry: &DeletedEntry) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO clipboard_entries \
             (id, app_id, content_type, text_content, image_path, created_at, \
              content_hash, source_url, is_favorite, is_sensitive, html_content) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                entry.id, entry.app_id, entry.content_type, entry.text_content,
                entry.image_path, entry.created_at, entry.content_hash,
                entry.source_url, entry.is_favorite, entry.is_sensitive, entry.html_content,
            ],
        )?;
        Ok(())
    }

    pub fn delete_entries_by_domain(&self, app_id: i64, domain: &str) -> Result<Vec<String>> {
        let filter = DOMAIN_FILTER_SQL.replace("{d}", "2");
        let select_q = format!(
            "SELECT image_path FROM clipboard_entries WHERE app_id = ?1 AND image_path IS NOT NULL AND {}",
            filter
        );
        let mut stmt = self.conn.prepare(&select_q)?;
        let paths: Vec<String> = stmt
            .query_map(params![app_id, domain], |row| row.get(0))?
            .collect::<Result<Vec<_>>>()?;

        let delete_q = format!(
            "DELETE FROM clipboard_entries WHERE app_id = ?1 AND {}",
            filter
        );
        self.conn.execute(&delete_q, params![app_id, domain])?;
        self.cleanup_empty_apps()?;
        Ok(paths)
    }

    pub fn clear_app_entries(&self, app_id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT image_path FROM clipboard_entries WHERE app_id = ?1 AND image_path IS NOT NULL",
        )?;
        let paths: Vec<String> = stmt
            .query_map(params![app_id], |row| row.get(0))?
            .collect::<Result<Vec<_>>>()?;

        self.conn.execute(
            "DELETE FROM clipboard_entries WHERE app_id = ?1",
            params![app_id],
        )?;
        self.cleanup_empty_apps()?;
        Ok(paths)
    }

    pub fn clear_all_entries(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT image_path FROM clipboard_entries WHERE image_path IS NOT NULL",
        )?;
        let paths: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>>>()?;

        self.conn.execute_batch(
            "BEGIN;
             DELETE FROM clipboard_entries;
             DELETE FROM apps;
             COMMIT;"
        )?;
        Ok(paths)
    }

    pub fn toggle_entry_favorite(&self, id: i64) -> Result<bool> {
        let current: i64 = self.conn.query_row(
            "SELECT COALESCE(is_favorite, 0) FROM clipboard_entries WHERE id = ?1",
            params![id], |row| row.get(0),
        )?;
        let new_val = if current != 0 { 0 } else { 1 };
        self.conn.execute("UPDATE clipboard_entries SET is_favorite = ?1 WHERE id = ?2", params![new_val, id])?;
        Ok(new_val != 0)
    }

    pub fn toggle_app_favorite(&self, id: i64) -> Result<bool> {
        let current: i64 = self.conn.query_row(
            "SELECT COALESCE(is_favorite, 0) FROM apps WHERE id = ?1",
            params![id], |row| row.get(0),
        )?;
        let new_val = if current != 0 { 0 } else { 1 };
        self.conn.execute("UPDATE apps SET is_favorite = ?1 WHERE id = ?2", params![new_val, id])?;
        Ok(new_val != 0)
    }

    pub fn toggle_sensitive(&self, id: i64) -> Result<bool> {
        let current: i64 = self.conn.query_row(
            "SELECT COALESCE(is_sensitive, 0) FROM clipboard_entries WHERE id = ?1",
            params![id], |row| row.get(0),
        )?;
        let new_val = if current != 0 { 0 } else { 1 };
        self.conn.execute("UPDATE clipboard_entries SET is_sensitive = ?1 WHERE id = ?2", params![new_val, id])?;
        Ok(new_val != 0)
    }

    pub fn get_favorite_entries(&self, content_type: &str, page: i64, page_size: i64) -> Result<Vec<ClipboardEntry>> {
        let offset = (page - 1) * page_size;
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.app_id, e.content_type, e.text_content, e.image_path, e.created_at, e.source_url, COALESCE(e.is_favorite,0), COALESCE(e.is_sensitive,0), e.html_content
             FROM clipboard_entries e
             LEFT JOIN apps a ON e.app_id = a.id
             WHERE (e.is_favorite = 1 OR COALESCE(a.is_favorite,0) = 1) AND e.content_type = ?1
             ORDER BY e.created_at DESC LIMIT ?2 OFFSET ?3",
        )?;
        let result: Vec<ClipboardEntry> = stmt.query_map(params![content_type, page_size, offset], |row| {
            Ok(ClipboardEntry {
                id: row.get(0)?,
                app_id: row.get(1)?,
                content_type: row.get(2)?,
                text_content: row.get(3)?,
                image_path: row.get(4)?,
                created_at: row.get(5)?,
                source_url: row.get(6)?,
                is_favorite: row.get::<_, i64>(7)? != 0,
                is_sensitive: row.get::<_, i64>(8)? != 0,
                html_content: row.get(9)?,
            })
        })?.collect::<Result<Vec<_>>>()?;
        Ok(result)
    }

    pub fn get_favorite_counts(&self) -> Result<(i64, i64)> {
        self.conn.query_row(
            "SELECT
                SUM(CASE WHEN e.content_type = 'text' THEN 1 ELSE 0 END),
                SUM(CASE WHEN e.content_type = 'image' THEN 1 ELSE 0 END)
             FROM clipboard_entries e
             LEFT JOIN apps a ON e.app_id = a.id
             WHERE e.is_favorite = 1 OR COALESCE(a.is_favorite,0) = 1",
            [],
            |row| Ok((row.get::<_, Option<i64>>(0)?.unwrap_or(0), row.get::<_, Option<i64>>(1)?.unwrap_or(0))),
        )
    }

    pub fn upsert_text_entry_with_html(&self, app_id: i64, text: &str, hash: &str, source_url: Option<&str>, html: Option<&str>, is_sensitive: bool) -> Result<i64> {
        if let Ok(id) = self.conn.query_row(
            "SELECT id FROM clipboard_entries WHERE app_id = ?1 AND content_type = 'text' AND content_hash = ?2",
            params![app_id, hash],
            |row| row.get::<_, i64>(0),
        ) {
            self.conn.execute(
                "UPDATE clipboard_entries SET created_at = datetime('now', 'localtime'), source_url = COALESCE(?2, source_url), html_content = COALESCE(?3, html_content) WHERE id = ?1",
                params![id, source_url, html],
            )?;
            return Ok(id);
        }

        let sensitive_val: i64 = if is_sensitive { 1 } else { 0 };
        self.conn.execute(
            "INSERT INTO clipboard_entries (app_id, content_type, text_content, content_hash, source_url, html_content, is_sensitive) VALUES (?1, 'text', ?2, ?3, ?4, ?5, ?6)",
            params![app_id, text, hash, source_url, html, sensitive_val],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn apply_retention_policy(&self, policy: &str) -> Result<Vec<String>> {
        let tx = self.conn.unchecked_transaction()?;
        let result = match policy {
            "1d" | "3d" | "7d" | "30d" => {
                let days: i64 = policy.trim_end_matches('d').parse().unwrap_or(1);
                let cutoff = format!("-{} days", days);
                let mut stmt = tx.prepare(
                    "SELECT image_path FROM clipboard_entries WHERE image_path IS NOT NULL AND is_favorite = 0 AND created_at < datetime('now', 'localtime', ?1)",
                )?;
                let paths: Vec<String> = stmt.query_map(params![cutoff], |row| row.get(0))?.collect::<Result<Vec<_>>>()?;
                tx.execute("DELETE FROM clipboard_entries WHERE is_favorite = 0 AND created_at < datetime('now', 'localtime', ?1)", params![cutoff])?;
                Ok(paths)
            }
            "500" | "1000" | "5000" => {
                let max: i64 = policy.parse().unwrap_or(1000);
                let total: i64 = tx.query_row("SELECT COUNT(*) FROM clipboard_entries WHERE is_favorite = 0", [], |row| row.get(0))?;
                if total <= max {
                    return Ok(vec![]);
                }
                let to_delete = total - max;
                let mut stmt = tx.prepare(
                    "SELECT image_path FROM clipboard_entries WHERE image_path IS NOT NULL AND is_favorite = 0 ORDER BY created_at ASC LIMIT ?1",
                )?;
                let paths: Vec<String> = stmt.query_map(params![to_delete], |row| row.get(0))?.collect::<Result<Vec<_>>>()?;
                tx.execute(
                    "DELETE FROM clipboard_entries WHERE is_favorite = 0 AND id IN (SELECT id FROM clipboard_entries WHERE is_favorite = 0 ORDER BY created_at ASC LIMIT ?1)",
                    params![to_delete],
                )?;
                Ok(paths)
            }
            "midnight" => {
                let mut stmt = tx.prepare(
                    "SELECT image_path FROM clipboard_entries WHERE image_path IS NOT NULL AND is_favorite = 0",
                )?;
                let paths: Vec<String> = stmt.query_map([], |row| row.get(0))?.collect::<Result<Vec<_>>>()?;
                tx.execute("DELETE FROM clipboard_entries WHERE is_favorite = 0", [])?;
                Ok(paths)
            }
            _ => Ok(vec![]),
        };
        if result.is_ok() {
            tx.execute(
                "DELETE FROM apps WHERE id NOT IN (SELECT DISTINCT app_id FROM clipboard_entries)",
                [],
            )?;
            tx.commit()?;
        }
        result
    }

    fn cleanup_empty_apps(&self) -> Result<()> {
        self.conn.execute(
            "DELETE FROM apps WHERE id NOT IN (SELECT DISTINCT app_id FROM clipboard_entries)",
            [],
        )?;
        Ok(())
    }
}
