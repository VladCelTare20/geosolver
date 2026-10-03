//! Password hashing, session ids, and cookie helpers for account auth.
//!
//! Sessions are opaque random ids looked up against `db::sessions`; nothing is
//! encoded in the id itself. `current_user` is the single place that turns a
//! request's `Cookie` header into an authenticated `db::User`.

use argon2::{
    password_hash::{
        rand_core::{OsRng, RngCore},
        PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
    },
    Argon2,
};
use std::sync::OnceLock;

use axum::http::{header, HeaderMap};

use crate::db;

/// How long a session cookie/row stays valid.
pub const SESSION_TTL_SECS: i64 = 60 * 60 * 24 * 30; // 30 days

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

#[cfg(test)]
thread_local! {
    static ARGON2_VERIFIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    #[cfg(test)]
    ARGON2_VERIFIES.with(|c| c.set(c.get() + 1));
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

/// A real argon2 hash of a random secret nobody knows, built once.
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| {
        hash_password(&new_session_id()).expect("argon2 hashes a fixed-size random secret")
    })
}

/// Verify `password` against the stored hash, or against a dummy hash when the
/// user does not exist, so an unknown username costs the same argon2 work as a
/// wrong password and cannot be told apart by timing.
pub fn verify_password_or_dummy(password: &str, hash: Option<&str>) -> bool {
    match hash {
        Some(h) => verify_password(password, h),
        None => {
            let _ = verify_password(password, dummy_hash());
            false
        }
    }
}

/// A fresh 256-bit session id, hex-encoded.
pub fn new_session_id() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Over HTTPS the cookie carries the `__Host-` prefix: browsers then refuse it
/// unless it is `Secure`, `Path=/` and has no `Domain`, so a sibling subdomain
/// cannot plant or overwrite it.
pub fn cookie_name(secure: bool) -> &'static str {
    if secure {
        "__Host-sid"
    } else {
        "sid"
    }
}

pub fn set_cookie_header(id: &str, secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    let name = cookie_name(secure);
    format!("{name}={id}; HttpOnly; SameSite=Lax; Path=/; Max-Age={SESSION_TTL_SECS}{secure_flag}")
}

pub fn clear_cookie_header(secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    let name = cookie_name(secure);
    format!("{name}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{secure_flag}")
}

/// Read the session id out of the request's `Cookie` header(s). Anything
/// ambiguous (the cookie sent twice, e.g. a tossed duplicate) or not shaped like
/// one of our ids is treated as no session at all.
pub fn parse_sid(headers: &HeaderMap, secure: bool) -> Option<String> {
    let prefix = format!("{}=", cookie_name(secure));
    let mut found = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|c| c.split(';'))
        .filter_map(|kv| kv.trim().strip_prefix(prefix.as_str()));
    let sid = found.next()?;
    if found.next().is_some() {
        return None;
    }
    let well_formed = sid.len() == 64 && sid.bytes().all(|b| b.is_ascii_hexdigit());
    well_formed.then(|| sid.to_string())
}

/// Resolve the logged-in user for a request, if its session cookie names a live
/// session. Blocking (SQLite) — async callers use [`current_user_async`].
pub fn current_user(db: &db::Db, headers: &HeaderMap, secure: bool) -> Option<db::User> {
    let sid = parse_sid(headers, secure)?;
    db::lookup_session(&db::lock(db), &sid).ok()?
}

/// [`current_user`] on the blocking pool, so a slow DB lock never stalls an
/// async worker. Requests without a session cookie skip the pool entirely.
pub async fn current_user_async(db: &db::Db, headers: &HeaderMap, secure: bool) -> Option<db::User> {
    parse_sid(headers, secure)?;
    let (db, headers) = (db.clone(), headers.clone());
    tokio::task::spawn_blocking(move || current_user(&db, &headers, secure))
        .await
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_roundtrip() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("wrong password", &hash));
    }

    #[test]
    fn hash_is_not_plaintext_and_salts_differ() {
        let h1 = hash_password("same-password").unwrap();
        let h2 = hash_password("same-password").unwrap();
        assert_ne!(h1, "same-password");
        assert_ne!(h1, h2); // random salt per hash
    }

    #[test]
    fn session_ids_are_256_bit_hex_and_unique() {
        let a = new_session_id();
        let b = new_session_id();
        assert_eq!(a.len(), 64); // 32 bytes hex-encoded
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn cookie_headers_carry_expected_attributes() {
        let set = set_cookie_header("abc123", false);
        assert!(set.starts_with("sid=abc123;"));
        assert!(set.contains("HttpOnly"));
        assert!(set.contains("SameSite=Lax"));
        assert!(!set.contains("Secure"));
        assert!(set.contains("Path=/"));
        assert!(clear_cookie_header(false).starts_with("sid=;"));
        assert!(clear_cookie_header(false).contains("Max-Age=0"));
        assert!(clear_cookie_header(false).contains("Path=/"));
    }

    #[test]
    fn secure_cookies_use_the_host_prefix() {
        let set = set_cookie_header("abc123", true);
        assert!(set.starts_with("__Host-sid=abc123;"));
        assert!(set.contains("; Secure"));
        assert!(set.contains("HttpOnly"));
        assert!(set.contains("SameSite=Lax"));
        assert!(set.contains("Path=/"));
        assert!(!set.contains("Domain"));
        let clear = clear_cookie_header(true);
        assert!(clear.starts_with("__Host-sid=;") && clear.contains("Secure") && clear.contains("Max-Age=0"));
    }

    fn cookie(v: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.append(header::COOKIE, v.parse().unwrap());
        h
    }

    const SID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn parse_sid_reads_cookie_header() {
        let h = cookie(&format!("foo=bar; sid={SID}; other=1"));
        assert_eq!(parse_sid(&h, false).as_deref(), Some(SID));
        assert_eq!(parse_sid(&HeaderMap::new(), false), None);
    }

    #[test]
    fn parse_sid_secure_reads_only_the_host_prefixed_cookie() {
        let h = cookie(&format!("sid={SID}"));
        assert_eq!(parse_sid(&h, true), None);
        let h = cookie(&format!("__Host-sid={SID}"));
        assert_eq!(parse_sid(&h, true).as_deref(), Some(SID));
        assert_eq!(parse_sid(&h, false), None);
    }

    #[test]
    fn parse_sid_rejects_duplicates_and_malformed_ids() {
        let other = SID.replace('0', "f");
        assert_eq!(parse_sid(&cookie(&format!("sid={SID}; sid={other}")), false), None);
        let mut split = cookie(&format!("sid={SID}"));
        split.append(header::COOKIE, format!("sid={other}").parse().unwrap());
        assert_eq!(parse_sid(&split, false), None);
        assert_eq!(parse_sid(&cookie("sid=deadbeef"), false), None);
        assert_eq!(parse_sid(&cookie(&format!("sid={}", SID.replace('a', "g"))), false), None);
    }

    #[test]
    fn current_user_survives_a_poisoned_db_mutex() {
        let dir = tempfile::tempdir().unwrap();
        let db = db::open(&dir.path().join("a.db")).unwrap();
        {
            let conn = db::lock(&db);
            let uid = db::create_user(&conn, "alice", "h").unwrap();
            db::insert_session(&conn, SID, uid, 3600).unwrap();
        }
        let d2 = db.clone();
        let _ = std::thread::spawn(move || {
            let _g = d2.lock().unwrap();
            panic!("poison");
        })
        .join();
        let h = cookie(&format!("sid={SID}"));
        assert_eq!(current_user(&db, &h, false).unwrap().username, "alice");
    }

    #[test]
    fn unknown_user_costs_a_full_argon2_verify() {
        let verifies = || ARGON2_VERIFIES.with(|c| c.get());
        let before = verifies();
        assert!(!verify_password_or_dummy("guess", None));
        assert_eq!(verifies(), before + 1, "unknown user must still run argon2");
        let dummy = PasswordHash::new(dummy_hash()).expect("dummy is a real PHC hash");
        assert_eq!(dummy.algorithm.as_str(), "argon2id");
        let real = hash_password("x").unwrap();
        assert_eq!(
            PasswordHash::new(&real).unwrap().params,
            dummy.params,
            "same cost parameters as real hashes"
        );
    }
}
