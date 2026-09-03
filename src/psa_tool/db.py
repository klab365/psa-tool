import sqlite3
from datetime import datetime, timezone

from .paths import DB_FILE

_connection: sqlite3.Connection | None = None

SCHEMA = """
CREATE TABLE IF NOT EXISTS time_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    work_date TEXT NOT NULL,
    project_id TEXT,
    project_name TEXT,
    task_id TEXT,
    task_name TEXT,
    hours REAL NOT NULL,
    description TEXT,
    remote_id TEXT,
    status TEXT NOT NULL DEFAULT 'new', -- new | modified | synced | deleted (Aktions-Status)
    entry_status TEXT, -- Genehmigungsstatus aus Dataverse (z.B. "Genehmigt"), nur informativ, nur via 'psa pull' befuellt
    error TEXT, -- zuletzt aufgetretener Sync-Fehler, unabhaengig vom Status; wird bei Erfolg geleert
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"""


def _migrate(conn: sqlite3.Connection) -> None:
    """Fuegt neu hinzugekommene Spalten zu bereits existierenden Datenbanken hinzu."""
    existing_columns = {row[1] for row in conn.execute("PRAGMA table_info(time_entries)").fetchall()}
    if "entry_status" not in existing_columns:
        conn.execute("ALTER TABLE time_entries ADD COLUMN entry_status TEXT")
        conn.commit()


def _now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S")


def get_db() -> sqlite3.Connection:
    global _connection
    if _connection is not None:
        return _connection
    DB_FILE.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    conn = sqlite3.connect(DB_FILE)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode = WAL")
    conn.executescript(SCHEMA)
    conn.commit()
    _migrate(conn)
    _connection = conn
    return conn


def insert_entry(entry: dict) -> int:
    db = get_db()
    now = _now()
    cur = db.execute(
        """
        INSERT INTO time_entries
            (work_date, project_id, project_name, task_id, task_name, hours, description,
             status, created_at, updated_at)
        VALUES (:work_date, :project_id, :project_name, :task_id, :task_name, :hours,
                :description, 'new', :now, :now)
        """,
        {**entry, "now": now},
    )
    db.commit()
    return cur.lastrowid


def update_entry(entry_id: int, fields: dict) -> None:
    db = get_db()
    current = get_entry(entry_id)
    if current is None:
        raise ValueError(f"Eintrag {entry_id} nicht gefunden")
    merged = {**dict(current), **fields}
    next_status = "modified" if current["remote_id"] else "new"
    db.execute(
        """
        UPDATE time_entries
        SET work_date=:work_date, project_id=:project_id, project_name=:project_name,
            task_id=:task_id, task_name=:task_name, hours=:hours, description=:description,
            status=:status, error=NULL, updated_at=:updated_at
        WHERE id=:id
        """,
        {**merged, "status": next_status, "id": entry_id, "updated_at": _now()},
    )
    db.commit()


def mark_deleted(entry_id: int) -> None:
    db = get_db()
    entry = get_entry(entry_id)
    if entry is None:
        raise ValueError(f"Eintrag {entry_id} nicht gefunden")
    if not entry["remote_id"]:
        db.execute("DELETE FROM time_entries WHERE id = ?", (entry_id,))
    else:
        db.execute(
            "UPDATE time_entries SET status='deleted', error=NULL, updated_at=? WHERE id=?",
            (_now(), entry_id),
        )
    db.commit()


def get_entry(entry_id: int) -> sqlite3.Row | None:
    return get_db().execute("SELECT * FROM time_entries WHERE id = ?", (entry_id,)).fetchone()


def get_by_remote_id(remote_id: str) -> sqlite3.Row | None:
    return get_db().execute("SELECT * FROM time_entries WHERE remote_id = ?", (remote_id,)).fetchone()


def insert_synced_entry(entry: dict, remote_id: str) -> int:
    """Fuegt einen Eintrag ein, der 1:1 aus Dataverse stammt (z.B. via 'psa pull')."""
    db = get_db()
    now = _now()
    payload = {**entry, "remote_id": remote_id, "now": now}
    payload.setdefault("entry_status", None)
    cur = db.execute(
        """
        INSERT INTO time_entries
            (work_date, project_id, project_name, task_id, task_name, hours, description,
             remote_id, status, entry_status, created_at, updated_at)
        VALUES (:work_date, :project_id, :project_name, :task_id, :task_name, :hours,
                :description, :remote_id, 'synced', :entry_status, :now, :now)
        """,
        payload,
    )
    db.commit()
    return cur.lastrowid


def update_synced_fields(entry_id: int, fields: dict) -> None:
    """Aktualisiert einen bereits synchronisierten Eintrag mit dem Remote-Stand,
    ohne den Sync-Status zu veraendern (bleibt 'synced')."""
    db = get_db()
    current = get_entry(entry_id)
    if current is None:
        raise ValueError(f"Eintrag {entry_id} nicht gefunden")
    merged = {**dict(current), **fields}
    db.execute(
        """
        UPDATE time_entries
        SET work_date=:work_date, project_id=:project_id, project_name=:project_name,
            task_id=:task_id, task_name=:task_name, hours=:hours, description=:description,
            entry_status=:entry_status, status='synced', updated_at=:updated_at
        WHERE id=:id
        """,
        {**merged, "id": entry_id, "updated_at": _now()},
    )
    db.commit()


def list_entries(from_date: str | None = None, to_date: str | None = None, include_deleted: bool = False):
    db = get_db()
    clauses = []
    params: list = []
    if from_date:
        clauses.append("work_date >= ?")
        params.append(from_date)
    if to_date:
        clauses.append("work_date <= ?")
        params.append(to_date)
    if not include_deleted:
        clauses.append("status != 'deleted'")
    where = f"WHERE {' AND '.join(clauses)}" if clauses else ""
    return db.execute(
        f"SELECT * FROM time_entries {where} ORDER BY work_date ASC, id ASC", params
    ).fetchall()


def list_pending():
    db = get_db()
    return db.execute(
        "SELECT * FROM time_entries WHERE status IN ('new','modified','deleted') ORDER BY id ASC"
    ).fetchall()


def set_synced(entry_id: int, remote_id: str) -> None:
    db = get_db()
    db.execute(
        "UPDATE time_entries SET remote_id=?, status='synced', error=NULL, updated_at=? WHERE id=?",
        (remote_id, _now(), entry_id),
    )
    db.commit()


def set_sync_error(entry_id: int, message: str) -> None:
    """Speichert die letzte Fehlermeldung, OHNE den Status (new/modified/deleted) zu
    veraendern - so wird beim naechsten 'psa sync' automatisch erneut versucht."""
    db = get_db()
    db.execute(
        "UPDATE time_entries SET error=?, updated_at=? WHERE id=?",
        (message, _now(), entry_id),
    )
    db.commit()


def delete_local(entry_id: int) -> None:
    db = get_db()
    db.execute("DELETE FROM time_entries WHERE id = ?", (entry_id,))
    db.commit()
