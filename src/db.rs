use crate::{model::TimeEntry, paths::AppPaths};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS time_entries (
id INTEGER PRIMARY KEY AUTOINCREMENT, work_date TEXT NOT NULL, project_id TEXT, project_name TEXT,
task_id TEXT, task_name TEXT, hours REAL NOT NULL, description TEXT, remote_id TEXT,
status TEXT NOT NULL DEFAULT 'new', entry_status TEXT, error TEXT,
created_at TEXT NOT NULL, updated_at TEXT NOT NULL);";

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("could not initialize local database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("could not create application directory: {0}")]
    Io(#[from] std::io::Error),
}
fn now() -> String {
    Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn open(paths: &AppPaths) -> Result<Connection, DatabaseError> {
    paths.ensure_app_dir()?;
    let c = Connection::open(&paths.db_file)?;
    c.pragma_update(None, "journal_mode", "WAL")?;
    c.execute_batch(SCHEMA)?;
    migrate(&c)?;
    Ok(c)
}
fn migrate(c: &Connection) -> rusqlite::Result<()> {
    let mut s = c.prepare("PRAGMA table_info(time_entries)")?;
    let cols = s
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !cols.iter().any(|x| x == "entry_status") {
        c.execute("ALTER TABLE time_entries ADD COLUMN entry_status TEXT", [])?;
    }
    Ok(())
}
pub fn get(c: &Connection, id: i64) -> rusqlite::Result<Option<TimeEntry>> {
    c.query_row(
        "SELECT * FROM time_entries WHERE id=?",
        [id],
        TimeEntry::from_row,
    )
    .optional()
}
pub fn by_remote_id(c: &Connection, id: &str) -> rusqlite::Result<Option<TimeEntry>> {
    c.query_row(
        "SELECT * FROM time_entries WHERE remote_id=?",
        [id],
        TimeEntry::from_row,
    )
    .optional()
}
pub fn list(
    c: &Connection,
    from: Option<&str>,
    to: Option<&str>,
    deleted: bool,
) -> rusqlite::Result<Vec<TimeEntry>> {
    let mut q = "SELECT * FROM time_entries WHERE 1=1".to_string();
    if from.is_some() {
        q.push_str(" AND work_date >= ?1")
    };
    if to.is_some() {
        q.push_str(if from.is_some() {
            " AND work_date <= ?2"
        } else {
            " AND work_date <= ?1"
        })
    };
    if !deleted {
        q.push_str(" AND status != 'deleted'")
    };
    q.push_str(" ORDER BY work_date DESC,id DESC");
    let mut s = c.prepare(&q)?;
    let rows = match (from, to) {
        (Some(a), Some(b)) => s.query_map(params![a, b], TimeEntry::from_row)?,
        (Some(a), None) => s.query_map([a], TimeEntry::from_row)?,
        (None, Some(b)) => s.query_map([b], TimeEntry::from_row)?,
        (None, None) => s.query_map([], TimeEntry::from_row)?,
    };
    rows.collect()
}
pub fn pending(c: &Connection) -> rusqlite::Result<Vec<TimeEntry>> {
    let mut s = c.prepare(
        "SELECT * FROM time_entries WHERE status IN ('new','modified','deleted') ORDER BY id",
    )?;
    s.query_map([], TimeEntry::from_row)?.collect()
}
pub fn insert(
    c: &Connection,
    e: &TimeEntry,
    remote: Option<&str>,
    status: &str,
) -> rusqlite::Result<i64> {
    let n = now();
    c.execute("INSERT INTO time_entries (work_date,project_id,project_name,task_id,task_name,hours,description,remote_id,status,entry_status,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",params![e.work_date,e.project_id,e.project_name,e.task_id,e.task_name,e.hours,e.description,remote,status,e.entry_status,n,n])?;
    Ok(c.last_insert_rowid())
}
pub fn update_synced(c: &Connection, id: i64, e: &TimeEntry) -> rusqlite::Result<()> {
    c.execute("UPDATE time_entries SET work_date=?,project_id=?,project_name=?,task_id=?,task_name=?,hours=?,description=?,entry_status=?,status='synced',error=NULL,updated_at=? WHERE id=?",params![e.work_date,e.project_id,e.project_name,e.task_id,e.task_name,e.hours,e.description,e.entry_status,now(),id])?;
    Ok(())
}

/// Persists a local edit. Entries that were never synced stay `new`; already
/// synced entries become `modified` so the next `psa sync` pushes the change.
pub fn update_local(c: &Connection, id: i64, e: &TimeEntry) -> rusqlite::Result<()> {
    c.execute("UPDATE time_entries SET work_date=?,project_id=?,project_name=?,task_id=?,task_name=?,hours=?,description=?,status=CASE WHEN remote_id IS NULL THEN 'new' ELSE 'modified' END,error=NULL,updated_at=? WHERE id=?",params![e.work_date,e.project_id,e.project_name,e.task_id,e.task_name,e.hours,e.description,now(),id])?;
    Ok(())
}
pub fn mark_deleted(c: &Connection, id: i64) -> rusqlite::Result<bool> {
    let Some(e) = get(c, id)? else {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    };
    if e.remote_id.is_some() {
        c.execute(
            "UPDATE time_entries SET status='deleted',error=NULL,updated_at=? WHERE id=?",
            params![now(), id],
        )?;
        Ok(false)
    } else {
        delete(c, id)?;
        Ok(true)
    }
}
pub fn delete(c: &Connection, id: i64) -> rusqlite::Result<()> {
    c.execute("DELETE FROM time_entries WHERE id=?", [id])?;
    Ok(())
}

/// Undoes a pending deletion. Returns `false` when the entry is not marked as
/// deleted, and an error when the entry no longer exists.
pub fn restore(c: &Connection, id: i64) -> rusqlite::Result<bool> {
    let Some(entry) = get(c, id)? else {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    };
    if entry.status != "deleted" {
        return Ok(false);
    }
    c.execute(
        "UPDATE time_entries SET status='synced', error=NULL, updated_at=? WHERE id=?",
        params![now(), id],
    )?;
    Ok(true)
}
pub fn synced(c: &Connection, id: i64, remote: &str) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE time_entries SET remote_id=?,status='synced',error=NULL,updated_at=? WHERE id=?",
        params![remote, now(), id],
    )?;
    Ok(())
}
pub fn error(c: &Connection, id: i64, msg: &str) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE time_entries SET error=?,updated_at=? WHERE id=?",
        params![msg, now(), id],
    )?;
    Ok(())
}
