//! SQLite-backed storage for accounts, sessions, and solve history.
//!
//! Opened once (from `AGSTUDIO_DB`, see `security::Config`) and shared behind a
//! mutex — SQLite itself serializes writers, and this is login/solve-rate
//! traffic, not a hot path. Migrations are idempotent (`CREATE ... IF NOT
//! EXISTS`) so `open` is safe to call on every startup.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

pub type Db = Arc<Mutex<Connection>>;

/// Keep at most this many history rows per user (oldest are dropped on insert).
const HISTORY_MAX_PER_USER: i64 = 500;

pub const SCHEMA_VERSION: i64 = 1;

/// Session tokens are stored hashed, so a leaked/backed-up DB file cannot be
/// replayed as a login. The client-side cookie keeps the preimage.
fn hash_sid(id: &str) -> String {
    let digest = Sha256::digest(id.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
}

pub struct HistoryEntry {
    pub id: i64,
    pub input: String,
    pub title: Option<String>,
    pub proved: bool,
    pub method: Option<String>,
    /// The solve's verdict (`proved`, `holds-numerically`, `refuted`,
    /// `not-proved`); `None` for rows recorded before verdicts were stored.
    pub status: Option<String>,
    /// The goal as the reader saw it (display names), for list titles.
    pub goal: Option<String>,
    /// Whether the full solution was stored, so reopening needs no re-solve.
    pub has_solution: bool,
    /// The stored solution's note key (`time_limit`, …), when there is one.
    pub note: Option<String>,
    /// The stored proof covers only the drawn configuration.
    pub as_drawn: bool,
    pub created_at: i64,
}

/// Lock the shared connection. A panic elsewhere while holding the lock must not
/// turn every later request into "logged out" or a 500, and no multi-statement
/// transaction is ever left open across a panic, so poison is recovered.
pub fn lock(db: &Db) -> MutexGuard<'_, Connection> {
    db.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

/// Open (creating if absent) and migrate the database at `path`.
///
/// The DB holds password hashes and session tokens, so it is owner-only on
/// disk: the main file is created 0600 *before* SQLite opens it (no window at
/// the umask default), and SQLite creates new `-wal`/`-shm` files with the main
/// file's mode. Sidecars left over from an earlier run with looser modes are
/// tightened explicitly, before and after the WAL is established.
pub fn open(path: &Path) -> rusqlite::Result<Db> {
    #[cfg(unix)]
    restrict_to_owner(path).map_err(|e| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
            Some(format!("cannot create {} owner-only: {e}", path.display())),
        )
    })?;
    let conn = Connection::open(path)?;
    init(&conn)?;
    #[cfg(unix)]
    let _ = restrict_to_owner(path);
    Ok(Arc::new(Mutex::new(conn)))
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(unix)]
fn restrict_to_owner(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    let owner_only = || std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, owner_only())?;
    for suffix in ["-wal", "-shm", "-journal"] {
        let p = sidecar(path, suffix);
        if p.exists() {
            std::fs::set_permissions(&p, owner_only())?;
        }
    }
    Ok(())
}

/// Foreign-key enforcement is a per-connection SQLite pragma (not persisted in
/// the file), so it must be set here — every connection that touches this
/// schema needs it, not just the one that ran `migrate`. `busy_timeout` rides
/// out rare writer collisions instead of surfacing SQLITE_BUSY, and WAL keeps
/// the file consistent across crashes without blocking readers on writes.
fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;
         PRAGMA synchronous = NORMAL;",
    )?;
    let found: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if found > SCHEMA_VERSION {
        return Err(failure(
            rusqlite::ffi::SQLITE_CANTOPEN,
            format!(
                "the database has schema version {found}, newer than this build's {SCHEMA_VERSION}: \
                 it was written by a newer GeoSolver. Run that version again, or restore the backup \
                 taken before the upgrade"
            ),
        ));
    }
    // `PRAGMA journal_mode` returns a row, so it can't ride in execute_batch.
    // (In-memory test DBs report `memory` here — also fine.)
    let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    migrate(conn)?;
    if found < SCHEMA_VERSION {
        conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
    }
    Ok(())
}

fn failure(code: std::ffi::c_int, msg: String) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), Some(msg))
}

pub fn checkpoint(conn: &Connection, mode: &str) -> rusqlite::Result<bool> {
    let busy: i64 = conn.query_row(&format!("PRAGMA wal_checkpoint({mode})"), [], |r| r.get(0))?;
    Ok(busy == 0)
}

#[derive(Debug)]
pub struct BackupSummary {
    pub users: i64,
    pub history: i64,
    pub bytes: u64,
}

pub fn backup(src: &Path, dest: &Path) -> Result<BackupSummary, String> {
    use rusqlite::OpenFlags;
    if !src.is_file() {
        return Err(format!("no database at {}", src.display()));
    }
    let dest_str = dest.to_str().ok_or_else(|| format!("{} is not valid UTF-8", dest.display()))?;
    let mut create = std::fs::OpenOptions::new();
    create.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut create, 0o600);
    create.open(dest).map_err(|e| format!("cannot create {}: {e}", dest.display()))?;
    let made = (|| -> rusqlite::Result<BackupSummary> {
        let live = Connection::open_with_flags(src, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        live.busy_timeout(std::time::Duration::from_secs(10))?;
        let version: i64 = live.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(failure(
                rusqlite::ffi::SQLITE_CANTOPEN,
                format!("schema version {version} is newer than this build's {SCHEMA_VERSION}; back it up with the newer agstudio"),
            ));
        }
        live.execute("VACUUM INTO ?1", [dest_str])?;
        drop(live);
        let copy = Connection::open_with_flags(dest, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        let _mode: String = copy.query_row("PRAGMA journal_mode = DELETE", [], |r| r.get(0))?;
        let check: String = copy.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(failure(rusqlite::ffi::SQLITE_CORRUPT, format!("integrity check of the copy: {check}")));
        }
        let count = |table: &str| -> rusqlite::Result<i64> {
            let exists = copy
                .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?
                .exists([table])?;
            if !exists {
                return Ok(0);
            }
            copy.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        };
        let (users, history) = (count("users")?, count("history")?);
        Ok(BackupSummary { users, history, bytes: 0 })
    })();
    match made {
        Ok(mut s) => {
            s.bytes = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
            Ok(s)
        }
        Err(e) => {
            let _ = std::fs::remove_file(dest);
            Err(format!("backup of {} failed: {e}", src.display()))
        }
    }
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY,
            username TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            created_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            user_id INTEGER NOT NULL REFERENCES users(id),
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_sessions_expiry ON sessions(expires_at);
         CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL REFERENCES users(id),
            input TEXT NOT NULL,
            title TEXT,
            proved INTEGER NOT NULL,
            method TEXT,
            created_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_history_user ON history(user_id, created_at DESC);
         CREATE TABLE IF NOT EXISTS server_secrets (
            name TEXT PRIMARY KEY,
            value BLOB NOT NULL
         );",
    )?;
    // Verdicts arrived after the history table; add the column to older DBs.
    let has_status = conn
        .prepare("SELECT 1 FROM pragma_table_info('history') WHERE name = 'status'")?
        .exists([])?;
    if !has_status {
        conn.execute_batch("ALTER TABLE history ADD COLUMN status TEXT;")?;
    }
    for (col, ddl) in [
        ("goal", "ALTER TABLE history ADD COLUMN goal TEXT;"),
        ("solution", "ALTER TABLE history ADD COLUMN solution TEXT;"),
    ] {
        let has = conn
            .prepare("SELECT 1 FROM pragma_table_info('history') WHERE name = ?1")?
            .exists([col])?;
        if !has {
            conn.execute_batch(ddl)?;
        }
    }
    // Usernames are case-insensitive. A DB from before that rule may already
    // hold `Alice` and `alice`; the index then fails to build and `create_user`'s
    // own check still prevents any new collision.
    if let Err(e) = conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_users_name_nocase ON users(username COLLATE NOCASE);",
    ) {
        eprintln!("db: case-insensitive username index not created (legacy duplicates?): {e}");
    }
    Ok(())
}

/// Whether the database takes a write: an immediate transaction that rewrites
/// the schema version and rolls back, so nothing changes.
pub fn write_probe(conn: &mut Connection) -> rusqlite::Result<()> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let v: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    tx.execute_batch(&format!("PRAGMA user_version = {v}"))?;
    tx.rollback()
}

/// The 32-byte server secret `name`, stored as `fresh` on first use. A row of
/// the wrong length is replaced (logged), which revokes whatever it signed.
pub fn server_secret(conn: &Connection, name: &str, fresh: [u8; 32]) -> rusqlite::Result<[u8; 32]> {
    conn.execute(
        "INSERT OR IGNORE INTO server_secrets (name, value) VALUES (?1, ?2)",
        rusqlite::params![name, &fresh[..]],
    )?;
    let stored: Vec<u8> =
        conn.query_row("SELECT value FROM server_secrets WHERE name = ?1", [name], |r| r.get(0))?;
    match <[u8; 32]>::try_from(stored.as_slice()) {
        Ok(k) => Ok(k),
        Err(_) => {
            eprintln!("db: server secret `{name}` had {} bytes, not 32; replacing it", stored.len());
            conn.execute(
                "UPDATE server_secrets SET value = ?2 WHERE name = ?1",
                rusqlite::params![name, &fresh[..]],
            )?;
            Ok(fresh)
        }
    }
}

fn row_to_user(r: &rusqlite::Row) -> rusqlite::Result<User> {
    Ok(User {
        id: r.get(0)?,
        username: r.get(1)?,
        password_hash: r.get(2)?,
    })
}

/// Create a user; fails (`SQLITE_CONSTRAINT`) on a duplicate username.
pub fn create_user(conn: &Connection, username: &str, password_hash: &str) -> rusqlite::Result<i64> {
    if find_user_by_name(conn, username)?.is_some() {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE),
            Some("username taken".into()),
        ));
    }
    conn.execute(
        "INSERT INTO users (username, password_hash, created_at) VALUES (?1, ?2, ?3)",
        params![username, password_hash, now()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn find_user_by_name(conn: &Connection, username: &str) -> rusqlite::Result<Option<User>> {
    conn.query_row(
        "SELECT id, username, password_hash FROM users \
         WHERE username = ?1 COLLATE NOCASE ORDER BY id LIMIT 1",
        params![username],
        row_to_user,
    )
    .optional()
}

/// Create a session for `user_id` that expires `ttl_secs` from now.
/// `id` is the client-facing token; only its hash touches the database.
pub fn insert_session(conn: &Connection, id: &str, user_id: i64, ttl_secs: i64) -> rusqlite::Result<()> {
    let created = now();
    conn.execute(
        "INSERT INTO sessions (id, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        params![hash_sid(id), user_id, created, created + ttl_secs],
    )?;
    Ok(())
}

/// Look up the user for a live (non-expired) session id.
pub fn lookup_session(conn: &Connection, id: &str) -> rusqlite::Result<Option<User>> {
    conn.query_row(
        "SELECT u.id, u.username, u.password_hash
         FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.id = ?1 AND s.expires_at > ?2",
        params![hash_sid(id), now()],
        row_to_user,
    )
    .optional()
}

pub fn delete_session(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![hash_sid(id)])?;
    Ok(())
}

/// Delete all rows past their `expires_at`, bounding table growth from
/// abandoned logins — the DB-backed counterpart to `RateLimiter::sweep`'s
/// in-memory cleanup. Returns the number of rows removed.
pub fn sweep_expired_sessions(conn: &Connection) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM sessions WHERE expires_at <= ?1", params![now()])
}

/// A solve to record: the verdict columns, plus optionally the display goal and
/// the full solution JSON.
pub struct NewHistory<'a> {
    pub input: &'a str,
    pub title: Option<&'a str>,
    pub proved: bool,
    pub method: Option<&'a str>,
    pub status: Option<&'a str>,
    pub goal: Option<&'a str>,
    pub solution: Option<&'a str>,
}

#[cfg(test)]
pub fn insert_history(
    conn: &Connection,
    user_id: i64,
    input: &str,
    title: Option<&str>,
    proved: bool,
    method: Option<&str>,
    status: Option<&str>,
) -> rusqlite::Result<i64> {
    insert_history_full(
        conn,
        user_id,
        &NewHistory { input, title, proved, method, status, goal: None, solution: None },
    )
}

pub fn insert_history_full(conn: &Connection, user_id: i64, h: &NewHistory) -> rusqlite::Result<i64> {
    conn.execute(
        "DELETE FROM history WHERE user_id = ?1 AND input = ?2 AND title IS ?3 AND method IS ?4 AND status IS ?5",
        params![user_id, h.input, h.title, h.method, h.status],
    )?;
    conn.execute(
        "INSERT INTO history (user_id, input, title, proved, method, status, goal, solution, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![user_id, h.input, h.title, h.proved as i64, h.method, h.status, h.goal, h.solution, now()],
    )?;
    let id = conn.last_insert_rowid();
    // Bound per-user growth: drop the oldest rows past the cap.
    conn.execute(
        "DELETE FROM history WHERE user_id = ?1 AND id NOT IN (
            SELECT id FROM history WHERE user_id = ?1
            ORDER BY created_at DESC, id DESC LIMIT ?2)",
        params![user_id, HISTORY_MAX_PER_USER],
    )?;
    Ok(id)
}

/// A page of a user's history, newest first: at most `limit` rows, strictly
/// older than the row `before` when given (keyset pagination by id).
pub fn list_history(
    conn: &Connection,
    user_id: i64,
    limit: i64,
    before: Option<i64>,
) -> rusqlite::Result<Vec<HistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, input, title, proved, method, created_at, status, goal, \
         solution IS NOT NULL, CASE WHEN json_valid(solution) THEN json_extract(solution, '$.view.note.key') END, \
         CASE WHEN json_valid(solution) THEN json_extract(solution, '$.view.as_drawn') END \
         FROM history WHERE user_id = ?1 AND id < ?2 ORDER BY id DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![user_id, before.unwrap_or(i64::MAX), limit], |r| {
        Ok(HistoryEntry {
            id: r.get(0)?,
            input: r.get(1)?,
            title: r.get(2)?,
            proved: r.get::<_, i64>(3)? != 0,
            method: r.get(4)?,
            created_at: r.get(5)?,
            status: r.get(6)?,
            goal: r.get(7)?,
            has_solution: r.get::<_, i64>(8)? != 0,
            note: r.get::<_, Option<String>>(9).ok().flatten(),
            as_drawn: r.get::<_, Option<i64>>(10).ok().flatten().unwrap_or(0) != 0,
        })
    })?;
    rows.collect()
}

/// The stored solution JSON of one of the user's own rows: `None` if no such
/// row, `Some(None)` if the row predates stored solutions.
/// Swap the stored result of one of `user_id`'s entries for `h` (same input
/// only), keeping its id and position. False when no such row matches.
pub fn replace_history(conn: &Connection, user_id: i64, id: i64, h: &NewHistory) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE history SET title = ?1, proved = ?2, method = ?3, status = ?4, goal = ?5, solution = ?6 \
         WHERE id = ?7 AND user_id = ?8 AND input = ?9",
        params![h.title, h.proved as i64, h.method, h.status, h.goal, h.solution, id, user_id, h.input],
    )?;
    Ok(n > 0)
}

pub fn history_solution(conn: &Connection, user_id: i64, id: i64) -> rusqlite::Result<Option<Option<String>>> {
    conn.query_row(
        "SELECT solution FROM history WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()
}

/// Delete one of the user's own history entries. Returns whether a row was
/// removed (an id belonging to someone else deletes nothing).
pub fn delete_history(conn: &Connection, user_id: i64, id: i64) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "DELETE FROM history WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
    )?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        conn
    }

    #[test]
    fn user_roundtrip() {
        let conn = mem();
        let id = create_user(&conn, "alice", "hash1").unwrap();
        let u = find_user_by_name(&conn, "alice").unwrap().unwrap();
        assert_eq!(u.id, id);
        assert_eq!(u.password_hash, "hash1");
        assert!(find_user_by_name(&conn, "bob").unwrap().is_none());
    }

    #[test]
    fn history_rows_carry_the_note_key() {
        let conn = mem();
        let uid = create_user(&conn, "tl", "h").unwrap();
        let sol = r#"{"view":{"note":{"key":"time_limit","secs":60}}}"#;
        let h = |s: Option<&'static str>| NewHistory {
            input: "x",
            title: None,
            proved: false,
            method: Some("aux-search"),
            status: Some("not-proved"),
            goal: None,
            solution: s,
        };
        insert_history_full(&conn, uid, &h(Some(sol))).unwrap();
        insert_history_full(&conn, uid, &NewHistory { input: "y", ..h(Some("not json")) }).unwrap();
        let rows = list_history(&conn, uid, 10, None).unwrap();
        assert_eq!(rows[1].note.as_deref(), Some("time_limit"));
        assert_eq!(rows[0].note, None);
    }

    #[test]
    fn duplicate_username_rejected() {
        let conn = mem();
        create_user(&conn, "alice", "h").unwrap();
        assert!(create_user(&conn, "alice", "h2").is_err());
    }

    #[test]
    fn session_lifecycle() {
        let conn = mem();
        let uid = create_user(&conn, "alice", "h").unwrap();
        insert_session(&conn, "sid1", uid, 3600).unwrap();
        let u = lookup_session(&conn, "sid1").unwrap().unwrap();
        assert_eq!(u.username, "alice");
        delete_session(&conn, "sid1").unwrap();
        assert!(lookup_session(&conn, "sid1").unwrap().is_none());
    }

    #[test]
    fn expired_session_is_invisible() {
        let conn = mem();
        let uid = create_user(&conn, "alice", "h").unwrap();
        insert_session(&conn, "sid1", uid, -1).unwrap(); // already expired
        assert!(lookup_session(&conn, "sid1").unwrap().is_none());
    }

    /// A database created before verdicts were stored gains the column on
    /// open, and its old rows read back with no status.
    #[test]
    fn legacy_history_gains_a_status_column() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, username TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL, created_at INTEGER NOT NULL);
             CREATE TABLE history (id INTEGER PRIMARY KEY,
                user_id INTEGER NOT NULL REFERENCES users(id), input TEXT NOT NULL,
                title TEXT, proved INTEGER NOT NULL, method TEXT, created_at INTEGER NOT NULL);
             INSERT INTO users VALUES (1, 'old', 'h', 0);
             INSERT INTO history VALUES (1, 1, 'p', NULL, 1, 'euclidean', 0);",
        )
        .unwrap();
        init(&conn).unwrap();
        init(&conn).unwrap(); // idempotent
        let hist = list_history(&conn, 1, 10, None).unwrap();
        assert_eq!(hist.len(), 1);
        assert!(hist[0].status.is_none());
        insert_history(&conn, 1, "q", None, false, None, Some("refuted")).unwrap();
        let hist = list_history(&conn, 1, 10, None).unwrap();
        assert_eq!(hist[0].status.as_deref(), Some("refuted"));
    }

    #[test]
    fn history_scoped_and_ordered() {
        let conn = mem();
        let a = create_user(&conn, "alice", "h").unwrap();
        let b = create_user(&conn, "bob", "h").unwrap();
        insert_history(&conn, a, "p1", Some("t1"), true, Some("ddar"), Some("proved")).unwrap();
        insert_history(&conn, a, "p2", None, false, Some("euclidean"), Some("holds-numerically"))
            .unwrap();
        insert_history(&conn, b, "p3", None, true, None, None).unwrap();
        let hist = list_history(&conn, a, 1000, None).unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].input, "p2"); // most recent first
        assert!(!hist[0].proved);
        assert_eq!(hist[0].status.as_deref(), Some("holds-numerically"));
        assert!(hist[1].proved);
        assert_eq!(hist[1].status.as_deref(), Some("proved"));
        assert_eq!(list_history(&conn, b, 1000, None).unwrap().len(), 1);
    }

    #[test]
    fn resolving_the_same_problem_replaces_its_entry() {
        let conn = mem();
        let a = create_user(&conn, "alice", "h").unwrap();
        let b = create_user(&conn, "bob", "h").unwrap();
        insert_history(&conn, a, "p1", None, true, Some("ddar"), Some("proved")).unwrap();
        insert_history(&conn, a, "p2", None, true, Some("ddar"), Some("proved")).unwrap();
        insert_history(&conn, b, "p1", None, true, Some("ddar"), Some("proved")).unwrap();
        insert_history(&conn, a, "p1", None, true, Some("ddar"), Some("proved")).unwrap();
        let hist = list_history(&conn, a, 1000, None).unwrap();
        assert_eq!(hist.iter().map(|h| h.input.as_str()).collect::<Vec<_>>(), ["p1", "p2"]);
        assert_eq!(list_history(&conn, b, 1000, None).unwrap().len(), 1);
        insert_history(&conn, a, "p1", None, true, Some("ddar+aux"), Some("proved")).unwrap();
        insert_history(&conn, a, "p1", None, false, Some("ddar"), Some("not-proved")).unwrap();
        assert_eq!(list_history(&conn, a, 1000, None).unwrap().len(), 4);
    }

    #[test]
    fn foreign_key_violation_rejected() {
        let conn = mem();
        assert!(insert_session(&conn, "sid1", 999, 3600).is_err());
        assert!(insert_history(&conn, 999, "p", None, true, None, None).is_err());
    }

    #[test]
    fn sweep_expired_sessions_removes_only_expired_rows() {
        let conn = mem();
        let uid = create_user(&conn, "alice", "h").unwrap();
        insert_session(&conn, "live", uid, 3600).unwrap();
        insert_session(&conn, "dead1", uid, -1).unwrap();
        insert_session(&conn, "dead2", uid, -100).unwrap();
        assert_eq!(sweep_expired_sessions(&conn).unwrap(), 2);
        assert!(lookup_session(&conn, "live").unwrap().is_some());
        // A second sweep with nothing left to remove is a no-op.
        assert_eq!(sweep_expired_sessions(&conn).unwrap(), 0);
    }

    #[test]
    fn session_tokens_are_hashed_at_rest() {
        let conn = mem();
        let uid = create_user(&conn, "alice", "h").unwrap();
        let sid = "supersecrettoken";
        insert_session(&conn, sid, uid, 3600).unwrap();
        // The plaintext token never appears in the table…
        let raw: Option<String> = conn
            .query_row("SELECT id FROM sessions WHERE id = ?1", params![sid], |r| r.get(0))
            .optional()
            .unwrap();
        assert!(raw.is_none());
        // …but the normal lookup (which hashes) still resolves it.
        assert!(lookup_session(&conn, sid).unwrap().is_some());
        delete_session(&conn, sid).unwrap();
        assert!(lookup_session(&conn, sid).unwrap().is_none());
    }

    #[test]
    fn history_is_capped_per_user() {
        let conn = mem();
        let uid = create_user(&conn, "alice", "h").unwrap();
        for i in 0..(HISTORY_MAX_PER_USER + 25) {
            insert_history(&conn, uid, &format!("p{i}"), None, true, None, None).unwrap();
        }
        let hist = list_history(&conn, uid, 1000, None).unwrap();
        assert_eq!(hist.len(), HISTORY_MAX_PER_USER as usize);
        // The survivors are the newest rows.
        assert_eq!(hist[0].input, format!("p{}", HISTORY_MAX_PER_USER + 24));
    }

    #[cfg(unix)]
    fn mode_of(p: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn fresh_db_and_wal_sidecars_are_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.db");
        let db = open(&path).unwrap();
        create_user(&lock(&db), "alice", "h").unwrap();
        assert_eq!(mode_of(&path), 0o600);
        for suffix in ["-wal", "-shm"] {
            let p = sidecar(&path, suffix);
            assert!(p.exists(), "{suffix} should exist in WAL mode");
            assert_eq!(mode_of(&p), 0o600, "{suffix}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn existing_world_readable_db_files_are_tightened() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.db");
        // A live connection keeps non-empty WAL/SHM files around; SQLite only
        // re-chmods sidecars it finds empty, so these would stay 0644.
        let first = open(&path).unwrap();
        create_user(&lock(&first), "alice", "h").unwrap();
        for p in [path.clone(), sidecar(&path, "-wal"), sidecar(&path, "-shm")] {
            assert!(std::fs::metadata(&p).unwrap().len() > 0, "{}", p.display());
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        let db = open(&path).unwrap();
        create_user(&lock(&db), "bob", "h").unwrap();
        for p in [path.clone(), sidecar(&path, "-wal"), sidecar(&path, "-shm")] {
            assert_eq!(mode_of(&p), 0o600, "{}", p.display());
        }
    }

    fn standalone_copy(path: &std::path::Path, dir: &std::path::Path) -> Connection {
        let copy = dir.join("copy-of-main-file-only.db");
        std::fs::copy(path, &copy).unwrap();
        assert!(!sidecar(&copy, "-wal").exists());
        Connection::open(&copy).unwrap()
    }

    fn usernames(conn: &Connection) -> Vec<String> {
        let Ok(mut st) = conn.prepare("SELECT username FROM users ORDER BY id") else {
            return Vec::new();
        };
        st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn without_a_checkpoint_the_main_file_alone_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live.db");
        let db = open(&path).unwrap();
        create_user(&lock(&db), "alice", "h").unwrap();
        assert!(usernames(&standalone_copy(&path, dir.path())).is_empty());
    }

    #[test]
    fn a_truncating_checkpoint_puts_everything_in_the_main_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live.db");
        let db = open(&path).unwrap();
        create_user(&lock(&db), "alice", "h").unwrap();
        server_secret(&lock(&db), "gate_key", [7; 32]).unwrap();
        assert!(checkpoint(&lock(&db), "TRUNCATE").unwrap());
        assert_eq!(std::fs::metadata(sidecar(&path, "-wal")).unwrap().len(), 0);
        let copy = standalone_copy(&path, dir.path());
        assert_eq!(usernames(&copy), ["alice"]);
        let n: i64 = copy.query_row("SELECT count(*) FROM server_secrets", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn backup_of_a_live_database_is_one_self_contained_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live.db");
        let db = open(&path).unwrap();
        let uid = create_user(&lock(&db), "alice", "h").unwrap();
        insert_history(&lock(&db), uid, "program", None, true, Some("ddar"), Some("proved")).unwrap();
        let dest = dir.path().join("backup.db");
        let s = backup(&path, &dest).unwrap();
        assert_eq!((s.users, s.history), (1, 1));
        assert!(s.bytes > 0);
        assert!(!sidecar(&dest, "-wal").exists());
        #[cfg(unix)]
        assert_eq!(mode_of(&dest), 0o600);
        let copy = standalone_copy(&dest, dir.path());
        assert_eq!(usernames(&copy), ["alice"]);
        let v: i64 = copy.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        create_user(&lock(&db), "bob", "h").unwrap();
        let again = backup(&path, &dest).unwrap_err();
        assert!(again.contains("cannot create"), "{again}");
        assert_eq!(usernames(&Connection::open(&dest).unwrap()), ["alice"]);
    }

    #[test]
    fn backup_refuses_a_missing_database_and_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent.db");
        let dest = dir.path().join("backup.db");
        let e = backup(&path, &dest).unwrap_err();
        assert!(e.contains("no database"), "{e}");
        assert!(!path.exists() && !dest.exists());
    }

    #[test]
    fn a_fresh_database_records_the_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.db");
        drop(open(&path).unwrap());
        let conn = Connection::open(&path).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn a_legacy_unversioned_database_is_upgraded_and_versioned() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, username TEXT UNIQUE NOT NULL,
                    password_hash TEXT NOT NULL, created_at INTEGER NOT NULL);
                 INSERT INTO users VALUES (1, 'old', 'h', 0);",
            )
            .unwrap();
        }
        let db = open(&path).unwrap();
        let conn = lock(&db);
        assert!(find_user_by_name(&conn, "old").unwrap().is_some());
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn a_database_from_a_newer_build_is_refused_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("newer.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(&format!(
                "CREATE TABLE future (x INTEGER); PRAGMA user_version = {};",
                SCHEMA_VERSION + 1
            ))
            .unwrap();
        }
        let e = open(&path).err().expect("a newer schema must not be opened").to_string();
        assert!(e.contains("newer"), "{e}");
        let conn = Connection::open(&path).unwrap();
        let has_users = conn
            .prepare("SELECT 1 FROM sqlite_master WHERE name = 'users'")
            .unwrap()
            .exists([])
            .unwrap();
        assert!(!has_users, "no migration may run on a newer schema");
        let b = backup(&path, &dir.path().join("b.db")).unwrap_err();
        assert!(b.contains("newer"), "{b}");
    }

    #[test]
    fn lock_recovers_from_a_poisoned_mutex() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir.path().join("p.db")).unwrap();
        let d2 = db.clone();
        let _ = std::thread::spawn(move || {
            let _g = d2.lock().unwrap();
            panic!("poison the db mutex");
        })
        .join();
        assert!(db.is_poisoned());
        create_user(&lock(&db), "alice", "h").unwrap();
        assert!(find_user_by_name(&lock(&db), "alice").unwrap().is_some());
    }

    #[test]
    fn list_history_is_paginated() {
        let conn = mem();
        let uid = create_user(&conn, "alice", "h").unwrap();
        for i in 0..10 {
            insert_history(&conn, uid, &format!("p{i}"), None, true, None, None).unwrap();
        }
        let page = list_history(&conn, uid, 4, None).unwrap();
        assert_eq!(page.len(), 4);
        assert_eq!(page[0].input, "p9");
        let next = list_history(&conn, uid, 4, Some(page[3].id)).unwrap();
        assert_eq!(next.len(), 4);
        assert_eq!(next[0].input, "p5");
    }

    #[test]
    fn usernames_are_unique_and_found_case_insensitively() {
        let conn = mem();
        create_user(&conn, "alice", "h").unwrap();
        assert!(create_user(&conn, "Alice", "h2").is_err());
        assert_eq!(find_user_by_name(&conn, "ALICE").unwrap().unwrap().username, "alice");
    }

    #[test]
    fn delete_history_scoped_to_owner() {
        let conn = mem();
        let alice = create_user(&conn, "alice", "h").unwrap();
        let bob = create_user(&conn, "bob", "h").unwrap();
        let id = insert_history(&conn, alice, "prog", None, true, None, None).unwrap();
        // Bob cannot delete Alice's entry.
        assert!(!delete_history(&conn, bob, id).unwrap());
        assert_eq!(list_history(&conn, alice, 1000, None).unwrap().len(), 1);
        // Alice can; a repeat delete reports nothing removed.
        assert!(delete_history(&conn, alice, id).unwrap());
        assert!(list_history(&conn, alice, 1000, None).unwrap().is_empty());
        assert!(!delete_history(&conn, alice, id).unwrap());
    }
}
