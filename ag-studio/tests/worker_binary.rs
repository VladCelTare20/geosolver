//! The production `agstudio` binary: its hidden `__solve-worker` subcommand,
//! and a real `serve` whose solves go through it.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use wait_timeout::ChildExt;

const BIN: &str = env!("CARGO_BIN_EXE_agstudio");
const CIRCUMCENTER: &str = "A B C = triangle\nO = circumcenter(A, B, C)\nprove cong(O, A, O, B)";

fn request(input: &str) -> String {
    serde_json::json!({
        "input": input, "kind": "geo", "light": true, "want_proof": true,
        "title": null, "panel": true, "mode": "solve", "secs": 30.0
    })
    .to_string()
}

fn run_worker(stdin: &str) -> (Option<i32>, String) {
    let mut child = Command::new(BIN)
        .arg("__solve-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let mut out = String::new();
    let mut stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let _ = stdout.read_to_string(&mut out);
        out
    });
    let status = match child.wait_timeout(Duration::from_secs(60)).unwrap() {
        Some(s) => s,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the worker did not exit");
        }
    };
    (status.code(), reader.join().unwrap())
}

#[test]
fn the_worker_subcommand_solves_one_request_and_exits() {
    let (code, out) = run_worker(&request(CIRCUMCENTER));
    assert_eq!(code, Some(0), "{out}");
    let reply: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(reply["result"]["Ok"]["proved"], true, "{reply}");
    assert_eq!(out.trim().lines().count(), 1, "stdout carries exactly the reply");

    let (code, out) = run_worker(&request("not geo"));
    assert_eq!(code, Some(0));
    let reply: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert!(reply["result"]["Err"].is_string(), "{reply}");

    let (code, out) = run_worker("{garbage");
    assert_eq!(code, Some(2));
    assert!(out.is_empty());
}

#[test]
fn worker_spawn_overhead_is_small() {
    let runs = 20;
    let t = Instant::now();
    for _ in 0..runs {
        let (code, _) = run_worker(&request("not geo"));
        assert_eq!(code, Some(0));
    }
    let mean = t.elapsed() / runs;
    eprintln!("worker round trip (spawn + request + reply + exit): {mean:?} mean over {runs}");
    assert!(mean < Duration::from_millis(200), "{mean:?}");
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    (18800..18850)
        .find(|p| std::net::TcpListener::bind(("127.0.0.1", *p)).is_ok())
        .expect("no free port in 18800-18849")
}

fn http(port: u16, method: &str, path: &str, cookie: Option<&str>, body: Option<&str>) -> (u16, String, String) {
    let mut sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(90))).unwrap();
    let body = body.unwrap_or("");
    let cookie = cookie.map(|c| format!("Cookie: {c}\r\n")).unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n{cookie}\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    sock.write_all(req.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    sock.read_to_end(&mut bytes).unwrap();
    let raw = String::from_utf8_lossy(&bytes).into_owned();
    let (head, rest) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    (status, head.to_string(), rest.to_string())
}

#[test]
fn a_real_server_solves_through_worker_processes() {
    let db = tempfile::tempdir().unwrap();
    let port = free_port();
    let server = Server(
        Command::new(BIN)
            .args(["serve", "--port", &port.to_string()])
            .env("AGSTUDIO_DB", db.path().join("t.db"))
            .env("AGSTUDIO_DISABLE_TRANSLATE", "1")
            .env("AGSTUDIO_MAX_CONCURRENT", "2")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let until = Instant::now() + Duration::from_secs(20);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < until, "server did not start");
        std::thread::sleep(Duration::from_millis(50));
    }
    let (st, _, body) = http(port, "GET", "/healthz", None, None);
    assert_eq!(st, 200, "{body}");

    let creds = serde_json::json!({"username": "itest", "password": "password123"}).to_string();
    let (st, head, body) = http(port, "POST", "/api/auth/register", None, Some(&creds));
    assert_eq!(st, 200, "{body}");
    let cookie = head
        .lines()
        .find_map(|l| l.strip_prefix("set-cookie: ").or_else(|| l.strip_prefix("Set-Cookie: ")))
        .and_then(|v| v.split(';').next())
        .expect("session cookie")
        .to_string();

    let solve = serde_json::json!({"input": CIRCUMCENTER}).to_string();
    let (st, _, body) = http(port, "POST", "/api/solve", Some(&cookie), Some(&solve));
    assert_eq!(st, 200, "{body}");
    assert!(body.contains("\"proved\":true"), "{body}");

    let export = serde_json::json!({"input": CIRCUMCENTER, "format": "pdf"}).to_string();
    let (st, _, body) = http(port, "POST", "/api/export", Some(&cookie), Some(&export));
    assert_eq!(st, 200);
    assert!(body.contains("%PDF"), "{}", &body[..body.len().min(200)]);
    drop(server);
}
