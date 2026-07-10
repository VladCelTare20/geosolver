//! SQLite-backed storage for accounts, sessions, and solve history.
//!
//! Opened once (from `AGSTUDIO_DB`, see `security::Config`) and shared behind a
//! mutex — SQLite itself serializes writers, and this is login/solve-rate
//! traffic, not a hot path. Migrations are idempotent (`CREATE ... IF NOT
//! EXISTS`) so `open` is safe to call on every startup. Not wired into the
//! server yet; that lands with `AppState` in a later task.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

pub type Db = Arc<Mutex<Connection>>;

/// Keep at most this many history rows per user (oldest are dropped on insert).
const HISTORY_MAX_PER_USER: i64 = 500;

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
    pub created_at: i64,
}

pub struct HistoryEntry {
    pub id: i64,
    pub user_id: i64,
    pub input: String,
    pub title: Option<String>,
    pub proved: bool,
    pub method: Option<String>,
    pub created_at: i64,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

/// Open (creating if absent) and migrate the database at `path`.
pub fn open(path: &Path) -> rusqlite::Result<Db> {
    let conn = Connection::open(path)?;
    // The DB holds password hashes and session tokens — owner-only on disk.
    // (WAL/SHM sidecars inherit the containing directory's protection; this
    // covers the main file, which is the one that persists.)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    init(&conn)?;
    Ok(Arc::new(Mutex::new(conn)))
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
    // `PRAGMA journal_mode` returns a row, so it can't ride in execute_batch.
    // (In-memory test DBs report `memory` here — also fine.)
    let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    migrate(conn)
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
         CREATE INDEX IF NOT EXISTS idx_history_user ON history(user_id, created_at DESC);",
    )
}

fn row_to_user(r: &rusqlite::Row) -> rusqlite::Result<User> {
    Ok(User {
        id: r.get(0)?,
        username: r.get(1)?,
        password_hash: r.get(2)?,
        created_at: r.get(3)?,
    })
}

/// Create a user; fails (`SQLITE_CONSTRAINT`) on a duplicate username.
pub fn create_user(conn: &Connection, username: &str, password_hash: &str) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO users (username, password_hash, created_at) VALUES (?1, ?2, ?3)",
        params![username, password_hash, now()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn find_user_by_name(conn: &Connection, username: &str) -> rusqlite::Result<Option<User>> {
    conn.query_row(
        "SELECT id, username, password_hash, created_at FROM users WHERE username = ?1",
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
        "SELECT u.id, u.username, u.password_hash, u.created_at
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

pub fn insert_history(
    conn: &Connection,
    user_id: i64,
    input: &str,
    title: Option<&str>,
    proved: bool,
    method: Option<&str>,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO history (user_id, input, title, proved, method, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![user_id, input, title, proved as i64, method, now()],
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

/// A user's history, newest first.
pub fn list_history(conn: &Connection, user_id: i64) -> rusqlite::Result<Vec<HistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, input, title, proved, method, created_at \
         FROM history WHERE user_id = ?1 ORDER BY created_at DESC, id DESC",
    )?;
    let rows = stmt.query_map(params![user_id], |r| {
        Ok(HistoryEntry {
            id: r.get(0)?,
            user_id: r.get(1)?,
            input: r.get(2)?,
            title: r.get(3)?,
            proved: r.get::<_, i64>(4)? != 0,
            method: r.get(5)?,
            created_at: r.get(6)?,
        })
    })?;
    rows.collect()
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

    #[test]
    fn history_scoped_and_ordered() {
        let conn = mem();
        let a = create_user(&conn, "alice", "h").unwrap();
        let b = create_user(&conn, "bob", "h").unwrap();
        insert_history(&conn, a, "p1", Some("t1"), true, Some("ddar")).unwrap();
        insert_history(&conn, a, "p2", None, false, Some("aux-search")).unwrap();
        insert_history(&conn, b, "p3", None, true, None).unwrap();
        let hist = list_history(&conn, a).unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].input, "p2"); // most recent first
        assert!(!hist[0].proved);
        assert!(hist[1].proved);
        assert_eq!(list_history(&conn, b).unwrap().len(), 1);
    }

    #[test]
    fn foreign_key_violation_rejected() {
        let conn = mem();
        assert!(insert_session(&conn, "sid1", 999, 3600).is_err());
        assert!(insert_history(&conn, 999, "p", None, true, None).is_err());
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
            insert_history(&conn, uid, &format!("p{i}"), None, true, None).unwrap();
        }
        let hist = list_history(&conn, uid).unwrap();
        assert_eq!(hist.len(), HISTORY_MAX_PER_USER as usize);
        // The survivors are the newest rows.
        assert_eq!(hist[0].input, format!("p{}", HISTORY_MAX_PER_USER + 24));
    }

    #[test]
    fn delete_history_scoped_to_owner() {
        let conn = mem();
        let alice = create_user(&conn, "alice", "h").unwrap();
        let bob = create_user(&conn, "bob", "h").unwrap();
        let id = insert_history(&conn, alice, "prog", None, true, None).unwrap();
        // Bob cannot delete Alice's entry.
        assert!(!delete_history(&conn, bob, id).unwrap());
        assert_eq!(list_history(&conn, alice).unwrap().len(), 1);
        // Alice can; a repeat delete reports nothing removed.
        assert!(delete_history(&conn, alice, id).unwrap());
        assert!(list_history(&conn, alice).unwrap().is_empty());
        assert!(!delete_history(&conn, alice, id).unwrap());
    }
}
