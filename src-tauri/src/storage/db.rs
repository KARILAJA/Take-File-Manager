use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::str::FromStr;

use crate::manager::state::DownloadStatus;

pub const INITIAL_MIGRATION: &str = include_str!("migrations/001_initial.sql");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: String,
    pub url: String,
    pub final_url: Option<String>,
    pub file_name: String,
    pub save_dir: String,
    pub total_bytes: Option<u64>,
    pub downloaded: u64,
    pub status: DownloadStatus,
    pub error: Option<String>,
    pub resumable: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub category_id: Option<String>,
    pub queue_id: Option<String>,
    pub priority: i32,
    pub segments_cfg: u32,
    pub speed_limit: Option<u64>,
    pub headers_json: Option<String>,
    pub checksum_kind: Option<String>,
    pub checksum_value: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentRecord {
    pub download_id: String,
    pub idx: u32,
    pub start_byte: u64,
    pub end_byte: u64,
    pub written: u64,
}

pub fn init_db<P: AsRef<Path>>(path: P) -> Result<Connection> {
    let conn = Connection::open(path)?;
    run_migrations(&conn)?;
    Ok(conn)
}

pub fn init_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    run_migrations(&conn)?;
    Ok(conn)
}

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(INITIAL_MIGRATION)?;
    Ok(())
}

pub fn insert_download(conn: &Connection, d: &DownloadRecord) -> Result<()> {
    conn.execute(
        "INSERT INTO downloads (
            id, url, final_url, file_name, save_dir, total_bytes, downloaded,
            status, error, resumable, etag, last_modified, category_id, queue_id,
            priority, segments_cfg, speed_limit, headers_json, checksum_kind,
            checksum_value, created_at, started_at, completed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
        params![
            d.id,
            d.url,
            d.final_url,
            d.file_name,
            d.save_dir,
            d.total_bytes.map(|v| v as i64),
            d.downloaded as i64,
            d.status.to_string(),
            d.error,
            if d.resumable { 1 } else { 0 },
            d.etag,
            d.last_modified,
            d.category_id,
            d.queue_id,
            d.priority,
            d.segments_cfg as i64,
            d.speed_limit.map(|v| v as i64),
            d.headers_json,
            d.checksum_kind,
            d.checksum_value,
            d.created_at,
            d.started_at,
            d.completed_at
        ],
    )?;
    Ok(())
}

pub fn update_download_status(
    conn: &Connection,
    id: &str,
    status: DownloadStatus,
    error: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE downloads SET status = ?1, error = ?2 WHERE id = ?3",
        params![status.to_string(), error, id],
    )?;
    Ok(())
}

pub fn update_download_url(conn: &Connection, id: &str, new_url: &str) -> Result<()> {
    conn.execute(
        "UPDATE downloads SET url = ?1, error = NULL WHERE id = ?2",
        params![new_url, id],
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct DownloadMetadataUpdate<'a> {
    pub final_url: &'a str,
    pub file_name: &'a str,
    pub total_bytes: Option<u64>,
    pub resumable: bool,
    pub etag: Option<&'a str>,
    pub last_modified: Option<&'a str>,
}

pub fn update_download_meta(
    conn: &Connection,
    id: &str,
    meta: &DownloadMetadataUpdate,
) -> Result<()> {
    conn.execute(
        "UPDATE downloads SET 
            final_url = ?1, file_name = ?2, total_bytes = ?3,
            resumable = ?4, etag = ?5, last_modified = ?6
         WHERE id = ?7",
        params![
            meta.final_url,
            meta.file_name,
            meta.total_bytes.map(|v| v as i64),
            if meta.resumable { 1 } else { 0 },
            meta.etag,
            meta.last_modified,
            id
        ],
    )?;
    Ok(())
}

pub fn checkpoint_download(
    conn: &mut Connection,
    id: &str,
    downloaded: u64,
    segments: &[SegmentRecord],
) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE downloads SET downloaded = ?1 WHERE id = ?2",
        params![downloaded as i64, id],
    )?;

    for seg in segments {
        tx.execute(
            "INSERT INTO segments (download_id, idx, start_byte, end_byte, written)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(download_id, idx) DO UPDATE SET
                start_byte = excluded.start_byte,
                end_byte = excluded.end_byte,
                written = excluded.written",
            params![
                seg.download_id,
                seg.idx as i64,
                seg.start_byte as i64,
                seg.end_byte as i64,
                seg.written as i64
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}

pub fn get_download(conn: &Connection, id: &str) -> Result<Option<DownloadRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, final_url, file_name, save_dir, total_bytes, downloaded,
                status, error, resumable, etag, last_modified, category_id, queue_id,
                priority, segments_cfg, speed_limit, headers_json, checksum_kind,
                checksum_value, created_at, started_at, completed_at
         FROM downloads WHERE id = ?1",
    )?;

    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        let status_str: String = row.get(7)?;
        let status = DownloadStatus::from_str(&status_str).unwrap_or(DownloadStatus::Failed);

        let total_bytes: Option<i64> = row.get(5)?;
        let downloaded: i64 = row.get(6)?;
        let resumable_i: i64 = row.get(9)?;
        let segments_cfg: i64 = row.get(15)?;
        let speed_limit: Option<i64> = row.get(16)?;

        Ok(Some(DownloadRecord {
            id: row.get(0)?,
            url: row.get(1)?,
            final_url: row.get(2)?,
            file_name: row.get(3)?,
            save_dir: row.get(4)?,
            total_bytes: total_bytes.map(|v| v as u64),
            downloaded: downloaded as u64,
            status,
            error: row.get(8)?,
            resumable: resumable_i != 0,
            etag: row.get(10)?,
            last_modified: row.get(11)?,
            category_id: row.get(12)?,
            queue_id: row.get(13)?,
            priority: row.get(14)?,
            segments_cfg: segments_cfg as u32,
            speed_limit: speed_limit.map(|v| v as u64),
            headers_json: row.get(17)?,
            checksum_kind: row.get(18)?,
            checksum_value: row.get(19)?,
            created_at: row.get(20)?,
            started_at: row.get(21)?,
            completed_at: row.get(22)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn list_downloads(conn: &Connection, filter_status: Option<DownloadStatus>) -> Result<Vec<DownloadRecord>> {
    let mut sql = "SELECT id, url, final_url, file_name, save_dir, total_bytes, downloaded,
                          status, error, resumable, etag, last_modified, category_id, queue_id,
                          priority, segments_cfg, speed_limit, headers_json, checksum_kind,
                          checksum_value, created_at, started_at, completed_at
                   FROM downloads".to_string();

    if filter_status.is_some() {
        sql.push_str(" WHERE status = ?1");
    }
    sql.push_str(" ORDER BY created_at DESC");

    let mut stmt = conn.prepare(&sql)?;
    let rows = if let Some(status) = filter_status {
        stmt.query_map(params![status.to_string()], map_row)?
    } else {
        stmt.query_map([], map_row)?
    };

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<DownloadRecord> {
    let status_str: String = row.get(7)?;
    let status = DownloadStatus::from_str(&status_str).unwrap_or(DownloadStatus::Failed);

    let total_bytes: Option<i64> = row.get(5)?;
    let downloaded: i64 = row.get(6)?;
    let resumable_i: i64 = row.get(9)?;
    let segments_cfg: i64 = row.get(15)?;
    let speed_limit: Option<i64> = row.get(16)?;

    Ok(DownloadRecord {
        id: row.get(0)?,
        url: row.get(1)?,
        final_url: row.get(2)?,
        file_name: row.get(3)?,
        save_dir: row.get(4)?,
        total_bytes: total_bytes.map(|v| v as u64),
        downloaded: downloaded as u64,
        status,
        error: row.get(8)?,
        resumable: resumable_i != 0,
        etag: row.get(10)?,
        last_modified: row.get(11)?,
        category_id: row.get(12)?,
        queue_id: row.get(13)?,
        priority: row.get(14)?,
        segments_cfg: segments_cfg as u32,
        speed_limit: speed_limit.map(|v| v as u64),
        headers_json: row.get(17)?,
        checksum_kind: row.get(18)?,
        checksum_value: row.get(19)?,
        created_at: row.get(20)?,
        started_at: row.get(21)?,
        completed_at: row.get(22)?,
    })
}

pub fn get_segments(conn: &Connection, download_id: &str) -> Result<Vec<SegmentRecord>> {
    let mut stmt = conn.prepare(
        "SELECT download_id, idx, start_byte, end_byte, written
         FROM segments WHERE download_id = ?1 ORDER BY idx ASC",
    )?;

    let rows = stmt.query_map(params![download_id], |row| {
        let idx: i64 = row.get(1)?;
        let start: i64 = row.get(2)?;
        let end: i64 = row.get(3)?;
        let written: i64 = row.get(4)?;
        Ok(SegmentRecord {
            download_id: row.get(0)?,
            idx: idx as u32,
            start_byte: start as u64,
            end_byte: end as u64,
            written: written as u64,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn delete_download(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn log_event(
    conn: &Connection,
    level: &str,
    download_id: Option<&str>,
    message: &str,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "INSERT INTO logs (ts, level, download_id, message) VALUES (?1, ?2, ?3, ?4)",
        params![now, level, download_id, message],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Queues CRUD
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueRecord {
    pub id: String,
    pub name: String,
    pub max_concurrent: u32,
    pub speed_limit: Option<u64>,
    pub enabled: bool,
}

pub fn insert_queue(conn: &Connection, q: &QueueRecord) -> Result<()> {
    conn.execute(
        "INSERT INTO queues (id, name, max_concurrent, speed_limit, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            q.id,
            q.name,
            q.max_concurrent as i64,
            q.speed_limit.map(|v| v as i64),
            if q.enabled { 1 } else { 0 },
        ],
    )?;
    Ok(())
}

pub fn update_queue(conn: &Connection, q: &QueueRecord) -> Result<()> {
    conn.execute(
        "UPDATE queues SET name = ?1, max_concurrent = ?2, speed_limit = ?3, enabled = ?4
         WHERE id = ?5",
        params![
            q.name,
            q.max_concurrent as i64,
            q.speed_limit.map(|v| v as i64),
            if q.enabled { 1 } else { 0 },
            q.id,
        ],
    )?;
    Ok(())
}

pub fn get_queue(conn: &Connection, id: &str) -> Result<Option<QueueRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, max_concurrent, speed_limit, enabled FROM queues WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        let max_concurrent: i64 = row.get(2)?;
        let speed_limit: Option<i64> = row.get(3)?;
        let enabled: i64 = row.get(4)?;
        Ok(Some(QueueRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            max_concurrent: max_concurrent as u32,
            speed_limit: speed_limit.map(|v| v as u64),
            enabled: enabled != 0,
        }))
    } else {
        Ok(None)
    }
}

pub fn list_queues(conn: &Connection) -> Result<Vec<QueueRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, max_concurrent, speed_limit, enabled FROM queues ORDER BY name ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let max_concurrent: i64 = row.get(2)?;
        let speed_limit: Option<i64> = row.get(3)?;
        let enabled: i64 = row.get(4)?;
        Ok(QueueRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            max_concurrent: max_concurrent as u32,
            speed_limit: speed_limit.map(|v| v as u64),
            enabled: enabled != 0,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn delete_queue(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM queues WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn seed_default_queue_if_empty(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM queues", [], |r| r.get(0))?;
    if count == 0 {
        let default_q = QueueRecord {
            id: "default".to_string(),
            name: "Antrean Utama".to_string(),
            max_concurrent: 3,
            speed_limit: None,
            enabled: true,
        };
        insert_queue(conn, &default_q)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Schedules CRUD
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleRecord {
    pub id: String,
    pub queue_id: String,
    pub days_mask: u8,
    pub start_time: Option<String>,
    pub stop_time: Option<String>,
    pub after_action: String,
    pub after_command: Option<String>,
    pub enabled: bool,
}

pub fn insert_schedule(conn: &Connection, s: &ScheduleRecord) -> Result<()> {
    conn.execute(
        "INSERT INTO schedules (id, queue_id, days_mask, start_time, stop_time, after_action, after_command, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            s.id,
            s.queue_id,
            s.days_mask as i64,
            s.start_time,
            s.stop_time,
            s.after_action,
            s.after_command,
            if s.enabled { 1 } else { 0 },
        ],
    )?;
    Ok(())
}

pub fn update_schedule(conn: &Connection, s: &ScheduleRecord) -> Result<()> {
    conn.execute(
        "UPDATE schedules SET queue_id = ?1, days_mask = ?2, start_time = ?3, stop_time = ?4,
         after_action = ?5, after_command = ?6, enabled = ?7 WHERE id = ?8",
        params![
            s.queue_id,
            s.days_mask as i64,
            s.start_time,
            s.stop_time,
            s.after_action,
            s.after_command,
            if s.enabled { 1 } else { 0 },
            s.id,
        ],
    )?;
    Ok(())
}

pub fn get_schedule(conn: &Connection, id: &str) -> Result<Option<ScheduleRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, queue_id, days_mask, start_time, stop_time, after_action, after_command, enabled
         FROM schedules WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        let days_mask: i64 = row.get(2)?;
        let enabled: i64 = row.get(7)?;
        Ok(Some(ScheduleRecord {
            id: row.get(0)?,
            queue_id: row.get(1)?,
            days_mask: days_mask as u8,
            start_time: row.get(3)?,
            stop_time: row.get(4)?,
            after_action: row.get(5)?,
            after_command: row.get(6)?,
            enabled: enabled != 0,
        }))
    } else {
        Ok(None)
    }
}

pub fn list_schedules(conn: &Connection) -> Result<Vec<ScheduleRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, queue_id, days_mask, start_time, stop_time, after_action, after_command, enabled
         FROM schedules",
    )?;
    let rows = stmt.query_map([], |row| {
        let days_mask: i64 = row.get(2)?;
        let enabled: i64 = row.get(7)?;
        Ok(ScheduleRecord {
            id: row.get(0)?,
            queue_id: row.get(1)?,
            days_mask: days_mask as u8,
            start_time: row.get(3)?,
            stop_time: row.get(4)?,
            after_action: row.get(5)?,
            after_command: row.get(6)?,
            enabled: enabled != 0,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn delete_schedule(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM schedules WHERE id = ?1", params![id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Categories CRUD
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryRecord {
    pub id: String,
    pub name: String,
    pub extensions: String,
    pub save_dir: String,
}

pub fn insert_category(conn: &Connection, c: &CategoryRecord) -> Result<()> {
    conn.execute(
        "INSERT INTO categories (id, name, extensions, save_dir) VALUES (?1, ?2, ?3, ?4)",
        params![c.id, c.name, c.extensions, c.save_dir],
    )?;
    Ok(())
}

pub fn update_category(conn: &Connection, c: &CategoryRecord) -> Result<()> {
    conn.execute(
        "UPDATE categories SET name = ?1, extensions = ?2, save_dir = ?3 WHERE id = ?4",
        params![c.name, c.extensions, c.save_dir, c.id],
    )?;
    Ok(())
}

pub fn get_category(conn: &Connection, id: &str) -> Result<Option<CategoryRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, extensions, save_dir FROM categories WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(CategoryRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            extensions: row.get(2)?,
            save_dir: row.get(3)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn list_categories(conn: &Connection) -> Result<Vec<CategoryRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, extensions, save_dir FROM categories ORDER BY name ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CategoryRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            extensions: row.get(2)?,
            save_dir: row.get(3)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn delete_category(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM categories WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn seed_default_categories_if_empty(conn: &Connection, base_save_dir: &str) -> Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))?;
    let defaults = [
        ("compressed", "Compressed", "zip,rar,7z,tar,gz,bz2,xz,iso,tgz", "Compressed"),
        ("documents", "Dokumen", "pdf,doc,docx,xls,xlsx,ppt,pptx,txt,rtf,odt,epub", "Documents"),
        ("music", "Musik", "mp3,wav,flac,aac,ogg,m4a,wma", "Music"),
        ("programs", "Program", "exe,msi,apk,dmg,pkg,deb,rpm,appimage", "Programs"),
        ("video", "Video", "mp4,mkv,avi,mov,wmv,flv,webm,m4v,ts", "Video"),
        ("images", "Gambar", "jpg,jpeg,png,gif,bmp,webp,svg,tiff,ico", "Images"),
        ("torrent", "Torrent", "torrent", "Torrent"),
        ("others", "Lainnya", "*", "Others"),
    ];

    if count == 0 {
        for (id, name, exts, subfolder) in defaults {
            let full_save = format!("{}/{}", base_save_dir.trim_end_matches(['/', '\\']), subfolder);
            insert_category(conn, &CategoryRecord {
                id: id.to_string(),
                name: name.to_string(),
                extensions: exts.to_string(),
                save_dir: full_save,
            })?;
        }
    } else {
        // Ensure newly added built-in categories exist for upgraded databases
        let has_torrent: i64 = conn.query_row(
            "SELECT COUNT(*) FROM categories WHERE id = 'torrent'",
            [],
            |r| r.get(0),
        )?;
        if has_torrent == 0 {
            let full_save = format!("{}/Torrent", base_save_dir.trim_end_matches(['/', '\\']));
            insert_category(conn, &CategoryRecord {
                id: "torrent".to_string(),
                name: "Torrent".to_string(),
                extensions: "torrent".to_string(),
                save_dir: full_save,
            })?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Settings & Audit Logs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecord {
    pub id: i64,
    pub ts: i64,
    pub level: String,
    pub download_id: Option<String>,
    pub message: String,
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn list_logs(conn: &Connection, limit: usize) -> Result<Vec<LogRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, ts, level, download_id, message FROM logs ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(LogRecord {
            id: row.get(0)?,
            ts: row.get(1)?,
            level: row.get(2)?,
            download_id: row.get(3)?,
            message: row.get(4)?,
        })
    })?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrations_create_tables() {
        let conn = init_in_memory().expect("in-memory db initialization failed");
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert!(tables.contains(&"downloads".to_string()));
        assert!(tables.contains(&"segments".to_string()));
    }

    #[test]
    fn test_crud_download_and_checkpoint() {
        let mut conn = init_in_memory().unwrap();
        let record = DownloadRecord {
            id: "dl-1".to_string(),
            url: "https://example.com/file.zip".to_string(),
            final_url: None,
            file_name: "file.zip".to_string(),
            save_dir: "C:/Downloads".to_string(),
            total_bytes: Some(10_000_000),
            downloaded: 0,
            status: DownloadStatus::Queued,
            error: None,
            resumable: true,
            etag: Some("etag1".into()),
            last_modified: None,
            category_id: None,
            queue_id: None,
            priority: 0,
            segments_cfg: 4,
            speed_limit: None,
            headers_json: None,
            checksum_kind: None,
            checksum_value: None,
            created_at: 1000,
            started_at: None,
            completed_at: None,
        };

        insert_download(&conn, &record).unwrap();
        let retrieved = get_download(&conn, "dl-1").unwrap().expect("download must exist");
        assert_eq!(retrieved.status, DownloadStatus::Queued);

        update_download_status(&conn, "dl-1", DownloadStatus::Downloading, None).unwrap();

        // Checkpoint
        let segments = vec![
            SegmentRecord {
                download_id: "dl-1".into(),
                idx: 0,
                start_byte: 0,
                end_byte: 4_999_999,
                written: 2_000_000,
            },
            SegmentRecord {
                download_id: "dl-1".into(),
                idx: 1,
                start_byte: 5_000_000,
                end_byte: 9_999_999,
                written: 1_500_000,
            },
        ];
        checkpoint_download(&mut conn, "dl-1", 3_500_000, &segments).unwrap();

        let updated = get_download(&conn, "dl-1").unwrap().unwrap();
        assert_eq!(updated.downloaded, 3_500_000);

        let segs = get_segments(&conn, "dl-1").unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].written, 2_000_000);
        assert_eq!(segs[1].written, 1_500_000);
    }
}
