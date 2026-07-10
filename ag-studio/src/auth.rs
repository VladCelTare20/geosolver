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

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

/// A fresh 256-bit session id, hex-encoded.
pub fn new_session_id() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn set_cookie_header(id: &str, secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!("sid={id}; HttpOnly; SameSite=Lax; Path=/; Max-Age={SESSION_TTL_SECS}{secure_flag}")
}

pub fn clear_cookie_header() -> &'static str {
    "sid=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
}

/// Read the `sid` value out of the request's `Cookie` header, if present.
pub fn parse_sid(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie
        .split(';')
        .find_map(|kv| kv.trim().strip_prefix("sid=").map(str::to_string))
}

/// Resolve the logged-in user for a request, if its `sid` cookie names a live session.
pub fn current_user(db: &db::Db, headers: &HeaderMap) -> Option<db::User> {
    let sid = parse_sid(headers)?;
    let conn = db.lock().ok()?;
    db::lookup_session(&conn, &sid).ok()?
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
        assert!(set_cookie_header("abc123", true).contains("; Secure"));
        assert!(set.contains("Path=/"));
        assert!(clear_cookie_header().contains("Max-Age=0"));
        assert!(clear_cookie_header().contains("Path=/"));
    }

    #[test]
    fn parse_sid_reads_cookie_header() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, "foo=bar; sid=deadbeef; other=1".parse().unwrap());
        assert_eq!(parse_sid(&headers).as_deref(), Some("deadbeef"));
        assert_eq!(parse_sid(&HeaderMap::new()), None);
    }
}
