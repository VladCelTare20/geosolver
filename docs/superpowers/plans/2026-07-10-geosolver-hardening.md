# GeoSolver Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the confirmed crash/OOM/silent-failure gaps in GeoSolver (the shared `agstudio.service` web app at `ag-studio/` + its DDAR engine at `alphageometry-rs/`) so a single user's adversarial or accidental input can't crash the whole process or degrade it for everyone else, and so any future failure leaves a diagnosable server-side log trail.

**Architecture:** No architectural change. This is a hardening pass on the existing Rust workspace: two small, targeted fixes in the DDAR engine's recursive-descent parsers (`alphageometry-rs/src/geo.rs`, `alphageometry-rs/src/metric.rs`) that cap nesting depth and total point count, a logging fix in the web layer (`ag-studio/src/web.rs`), a panic-isolation fix in the MCP server (`ag-studio/src/mcp.rs`), new regression tests proving each fix, and a live-verification pass against the running systemd service.

**Tech Stack:** Rust (stable, workspace at `~/alphageometry-studio/Cargo.toml`), axum/tokio (`ag-studio`), `cargo test` (existing `#[tokio::test]` / `#[test]` conventions — no new test framework).

## Global Constraints

- Do not modify DDAR proof-correctness logic (`alphageometry-rs/`'s deduction/algebra/synthetic modules) — only add guard checks around existing entry points. Existing tests in `alphageometry-rs` and `ag-studio` must keep passing (`cargo test` at the workspace root).
- `panic = "unwind"` in the workspace `Cargo.toml` release profile stays as-is — it is deliberately required for the existing `catch_unwind` safety net (see `Cargo.toml:7-9`) and must not be changed to `"abort"`.
- No new crate dependencies. Every fix uses only what's already imported in the touched files.
- Follow existing code style: `eprintln!` for operator-facing diagnostics (not a new logging crate), doc comments explaining *why* a limit exists (this codebase's existing convention — see e.g. `geo.rs:2919-2922`), and the `catch_unwind(AssertUnwindSafe(...))` pattern already used throughout `engine.rs`/`aux_search.rs` for any new panic boundary.
- Numeric limits chosen below (`MAX_PARSE_DEPTH = 200`, `MAX_POINTS = 100`) are informed by the real corpus: `corpus/imo_ag_30.txt` problems use ~5–15 points and shallow (2–3 level) expression nesting, so both limits leave roughly an order of magnitude of headroom for legitimate use.

---

### Task 1: Cap expression-nesting recursion depth in `geo.rs`'s parser

**Files:**
- Modify: `alphageometry-rs/src/geo.rs:422-425` (the `Parser` struct), `geo.rs:640-646` (`parse_munary`), `geo.rs:526-539` (`parse_expr`), and the three `Parser { toks, pos: 0 }` construction sites at `geo.rs:2899`, `geo.rs:3009`, `geo.rs:3078`
- Test: `alphageometry-rs/src/geo.rs` (inside the existing `#[cfg(test)] mod tests` block starting at `geo.rs:3177`)

**Interfaces:**
- Consumes: nothing new — this is self-contained inside `geo.rs`'s existing `Parser`/`compile` machinery.
- Produces: `Parser::depth: u32` field and the invariant that `parse_munary`/`parse_expr` return `Err("expression nested too deeply (max depth 200)")` instead of recursing past 200 levels. Later tasks don't depend on this directly, but Task 6's HTTP-level test exercises the same behavior end-to-end.

**Context:** `parse_munary` (unary `-` chains) and `parse_expr` (nested construction calls like `reflect(reflect(reflect(...)))`) are genuinely self-/mutually-recursive with **no depth limit**, reachable from a `.geo` program within the existing 16,384-char body cap (`ag-studio/src/security.rs`, `AGSTUDIO_MAX_INPUT_CHARS`) — e.g. `prove dist(A, B) = ` followed by thousands of `-` characters. Because every level of paren-nesting and `sqrt`/`sin`/`cos`/`tan` nesting in the metric-expression grammar routes back through `parse_munary` (`parse_matom`'s `LParen` and function branches both call `parse_mexpr` → `parse_mterm` → `parse_mpow` → `parse_munary`), guarding `parse_munary` alone bounds all of that grammar's nesting, not just the unary-minus case. `parse_expr` is a separate grammar (construction calls) that needs its own guard. This is a **stack overflow**, not a panic — it is NOT caught by the `catch_unwind` wrapping every solver call, and aborts the whole `agstudio` process (every concurrent user's requests included), because tokio's `spawn_blocking` worker threads use the default 2 MiB stack (`ag-studio/src/main.rs:104`, no custom `thread_stack_size`).

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `geo.rs` (near the other `compile`-based tests, e.g. after `arbitrary_metric_constraint_positions_point` around `geo.rs:3240`):

```rust
    /// A long chain of unary `-` (or deeply nested parens) must be rejected
    /// with a clean parse error, not accepted — accepting it means nothing
    /// bounds how deep this recursive-descent parser can go, and a much
    /// longer chain (still well under the 16,384-char request body cap) would
    /// stack-overflow the whole process instead of just this one request.
    #[test]
    fn deeply_nested_unary_minus_is_rejected_not_accepted() {
        let mut src = String::from("prove dist(A, B) = ");
        src.push_str(&"-".repeat(500));
        src.push('5');
        let result = compile(&src);
        assert!(
            result.is_err(),
            "500 levels of unary-minus nesting should be rejected by a depth cap"
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("nested too deeply"),
            "expected a nesting-depth error, got: {msg}"
        );
    }

    /// Same guard, exercised through nested construction calls instead of
    /// unary minus — `parse_expr` is a separate recursive grammar from the
    /// metric-expression parser and needs its own depth cap.
    #[test]
    fn deeply_nested_construction_calls_are_rejected_not_accepted() {
        let mut src = String::from("A = free\nB = ");
        for _ in 0..500 {
            src.push_str("reflect(");
        }
        src.push('A');
        for _ in 0..500 {
            src.push(')');
        }
        src.push_str("\nprove coll(A, A, B)");
        let result = compile(&src);
        assert!(
            result.is_err(),
            "500 levels of construction-call nesting should be rejected by a depth cap"
        );
        assert!(result.unwrap_err().contains("nested too deeply"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd ~/alphageometry-studio && cargo test -p ddar deeply_nested -- --nocapture`
Expected: both tests FAIL — `result.is_err()` is false (today the parser happily recurses 500 levels deep since 500 is nowhere near an actual stack overflow, it just isn't rejected).

- [ ] **Step 3: Add the `depth` field and guard `parse_munary`**

In `geo.rs`, change the `Parser` struct (currently at `geo.rs:422-425`):

```rust
struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    /// Current recursive-descent nesting depth, shared across the
    /// metric-expression grammar (`parse_munary`) and the construction-call
    /// grammar (`parse_expr`). Bounded by `MAX_PARSE_DEPTH` so adversarial
    /// input gets a parse error instead of a stack overflow (which would
    /// abort the whole process — uncatchable, unlike an ordinary panic).
    depth: u32,
}

/// Real `.geo` programs never nest expressions more than a handful of levels
/// deep (see `corpus/imo_ag_30.txt`); 200 leaves generous headroom while
/// staying far below what could exhaust a thread's stack.
const MAX_PARSE_DEPTH: u32 = 200;
```

Update all three construction sites to add the new field — at `geo.rs:2899`, `geo.rs:3009`, and `geo.rs:3078`, change:

```rust
    let mut parser = Parser { toks, pos: 0 };
```

to:

```rust
    let mut parser = Parser { toks, pos: 0, depth: 0 };
```

Then replace `parse_munary` (currently `geo.rs:640-646`):

```rust
    fn parse_munary(&mut self) -> Result<MExpr, String> {
        if self.peek() == Some(&Tok::Minus) {
            self.next();
            return Ok(MExpr::Neg(Box::new(self.parse_munary()?)));
        }
        self.parse_matom()
    }
```

with:

```rust
    fn parse_munary(&mut self) -> Result<MExpr, String> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            self.depth -= 1;
            return Err(format!(
                "expression nested too deeply (max depth {MAX_PARSE_DEPTH})"
            ));
        }
        let result = if self.peek() == Some(&Tok::Minus) {
            self.next();
            self.parse_munary().map(|e| MExpr::Neg(Box::new(e)))
        } else {
            self.parse_matom()
        };
        self.depth -= 1;
        result
    }
```

- [ ] **Step 4: Guard `parse_expr`**

Replace `parse_expr` (currently `geo.rs:526-539`):

```rust
    fn parse_expr(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Tok::Ident(name)) => {
                if self.peek() == Some(&Tok::LParen) {
                    self.next();
                    let args = self.parse_expr_list_until_rparen()?;
                    Ok(Expr::Call(name, args))
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            other => Err(format!("expected expression, found {other:?}")),
        }
    }
```

with:

```rust
    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            self.depth -= 1;
            return Err(format!(
                "expression nested too deeply (max depth {MAX_PARSE_DEPTH})"
            ));
        }
        let result = match self.next() {
            Some(Tok::Ident(name)) => {
                if self.peek() == Some(&Tok::LParen) {
                    self.next();
                    self.parse_expr_list_until_rparen()
                        .map(|args| Expr::Call(name, args))
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            other => Err(format!("expected expression, found {other:?}")),
        };
        self.depth -= 1;
        result
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd ~/alphageometry-studio && cargo test -p ddar deeply_nested -- --nocapture`
Expected: both tests PASS.

- [ ] **Step 6: Run the full engine test suite to check for regressions**

Run: `cd ~/alphageometry-studio && cargo test -p ddar`
Expected: all existing tests still PASS (200 levels of headroom is far beyond anything in the existing corpus-derived tests).

- [ ] **Step 7: Commit**

```bash
cd ~/alphageometry-studio
git add alphageometry-rs/src/geo.rs
git commit -m "geo: cap expression-nesting recursion depth to prevent stack overflow"
```

---

### Task 2: Cap expression-nesting recursion depth in `metric.rs`'s parser

**Files:**
- Modify: `alphageometry-rs/src/metric.rs:152-155` (the `P` struct), `metric.rs:213-219` (`unary`), and the two `P { t: ..., i: 0 }` construction sites at `metric.rs:295-298` and `metric.rs:303-306`
- Test: `alphageometry-rs/src/metric.rs` (add a `#[cfg(test)] mod tests` block if one doesn't already exist near the bottom of the file — check first with `grep -n "mod tests" alphageometry-rs/src/metric.rs`)

**Interfaces:**
- Consumes: nothing new.
- Produces: same guarantee as Task 1 (`Err` instead of unbounded recursion), for the second, independent recursive-descent parser reachable via the classical Euclidean/absolute-length goal path (`ag-studio/src/engine.rs:579-594` → `ddar::metric::solve` → `metric.rs::parse_equation`).

**Context:** `metric.rs` has its own small, separate recursive-descent parser (struct `P`, not `Parser`) for the same class of arithmetic grammar, with an identical unguarded `unary()` (`metric.rs:213-218`) that recurses on `-` with no depth limit, and whose `atom()`'s `LParen`/function branches (`metric.rs:223-226`, `232-243`) route back through `expr → term → power → unary`, so guarding `unary()` alone bounds all of this grammar's nesting too — same reasoning as Task 1's `parse_munary`.

- [ ] **Step 1: Check whether a test module already exists**

Run: `grep -n "mod tests" ~/alphageometry-studio/alphageometry-rs/src/metric.rs`

If it prints a line, note the line number and add the new test inside that existing module (following its existing style). If it prints nothing, add a new module at the end of the file (Step 2 below shows where).

- [ ] **Step 2: Write the failing test**

If `metric.rs` has no test module yet, add this at the end of the file; otherwise add just the `#[test]` function inside the existing module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Same class of bug as `geo.rs`'s parser (see that file's
    /// `deeply_nested_unary_minus_is_rejected_not_accepted`): this is a
    /// second, independent recursive-descent parser and needs its own cap.
    #[test]
    fn deeply_nested_unary_minus_is_rejected_not_accepted() {
        let mut lhs = String::from("dist(A, B)");
        let mut rhs = String::new();
        rhs.push_str(&"-".repeat(500));
        rhs.push('5');
        let src = format!("{lhs} = {rhs}");
        let result = parse_equation(&src);
        assert!(
            result.is_err(),
            "500 levels of unary-minus nesting should be rejected by a depth cap"
        );
        assert!(result.unwrap_err().contains("nested too deeply"));
        lhs.clear(); // silence an unused-mut warning if lhs is otherwise never mutated
    }
}
```

(If a test module already exists, drop the `#[cfg(test)] mod tests { use super::*; ... }` wrapper and just add the `#[test]` function inside it — and drop the `lhs.clear()` line, which only exists to keep the standalone snippet warning-free; use a plain `let lhs = "dist(A, B)";` instead if adding to an existing module.)

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd ~/alphageometry-studio && cargo test -p ddar --lib metric::tests::deeply_nested -- --nocapture`
Expected: FAIL (`result.is_err()` is false).

- [ ] **Step 4: Add the `depth` field and guard `unary`**

Change the `P` struct (currently `metric.rs:152-155`):

```rust
struct P {
    t: Vec<Tok>,
    i: usize,
}
```

to:

```rust
struct P {
    t: Vec<Tok>,
    i: usize,
    /// Same guard as `geo::Parser::depth` — bounds recursive-descent nesting
    /// so adversarial input parse-errors instead of stack-overflowing.
    depth: u32,
}

const MAX_PARSE_DEPTH: u32 = 200;
```

Update both construction sites — `metric.rs:295-298`:

```rust
    let mut lp = P {
        t: toks[..eq].to_vec(),
        i: 0,
    };
```

becomes:

```rust
    let mut lp = P {
        t: toks[..eq].to_vec(),
        i: 0,
        depth: 0,
    };
```

and `metric.rs:303-306`:

```rust
    let mut rp = P {
        t: toks[eq + 1..].to_vec(),
        i: 0,
    };
```

becomes:

```rust
    let mut rp = P {
        t: toks[eq + 1..].to_vec(),
        i: 0,
        depth: 0,
    };
```

Then replace `unary` (currently `metric.rs:213-218`):

```rust
    fn unary(&mut self) -> Result<MExpr, String> {
        if self.peek() == Some(&Tok::Op('-')) {
            self.i += 1;
            return Ok(MExpr::Neg(Box::new(self.unary()?)));
        }
        self.atom()
    }
```

with:

```rust
    fn unary(&mut self) -> Result<MExpr, String> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            self.depth -= 1;
            return Err(format!(
                "expression nested too deeply (max depth {MAX_PARSE_DEPTH})"
            ));
        }
        let result = if self.peek() == Some(&Tok::Op('-')) {
            self.i += 1;
            self.unary().map(|e| MExpr::Neg(Box::new(e)))
        } else {
            self.atom()
        };
        self.depth -= 1;
        result
    }
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd ~/alphageometry-studio && cargo test -p ddar --lib metric::tests::deeply_nested -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Run the full engine test suite to check for regressions**

Run: `cd ~/alphageometry-studio && cargo test -p ddar`
Expected: all existing tests still PASS.

- [ ] **Step 7: Commit**

```bash
cd ~/alphageometry-studio
git add alphageometry-rs/src/metric.rs
git commit -m "metric: cap expression-nesting recursion depth to prevent stack overflow"
```

---

### Task 3: Cap total point count to bound memory/CPU blowup

**Files:**
- Modify: `alphageometry-rs/src/geo.rs` — add a helper near `compile` (before `geo.rs:2897`), and call it from `compile` (`geo.rs:2897-2903`), `build_instances` (`geo.rs:3007-3013`), and `build_algebraic` (`geo.rs:3076-3082`)
- Test: `alphageometry-rs/src/geo.rs` (inside the existing `#[cfg(test)] mod tests` block, alongside Task 1's tests)

**Interfaces:**
- Consumes: `Stmt` (already defined at `geo.rs:407-416`).
- Produces: a `too many points` `Err` from `compile`/`build_instances`/`build_algebraic` for programs with more than `MAX_POINTS` (100) declared points. Task 6's HTTP-level test relies on `compile`'s error message containing `"too many points"`.

**Context:** `Ddar::new_with_slack` (`engine.rs:194-277`) unconditionally allocates several `n×n` tables for `n` points on every solve attempt, and `search_similar`/`search_concyclic` (`engine.rs:1422`, `1524`) are O(n³) over active points on every deductive-closure fixpoint iteration — none of this is capped by point count today, only by the 16,384-char *text* limit in `ag-studio/src/security.rs`. A compact `.geo` program using `pN = free` declarations can define on the order of 1,500–2,000 uniquely-named points within that character budget — and `compile`'s instance-search loop (`geo.rs:2927`, `for seed in 1..=400u64`) would then repeat that O(n²)/O(n³) work up to 400 times per request. This is a genuine memory/CPU exhaustion path reachable from user input, capable of exhausting enough memory that the kernel OOM-kills the whole `agstudio` process — taking down every concurrent user's session, not just the offending request.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `geo.rs` (next to Task 1's tests):

```rust
    /// A program declaring far more points than any real geometry problem
    /// needs must be rejected quickly, not accepted — accepting it means
    /// `Ddar::new`'s O(n^2) allocation and `deduction_closure`'s O(n^3) search
    /// scale with an attacker-chosen `n`, up to ~2,000 points fit in the
    /// 16,384-char request body cap alone.
    #[test]
    fn too_many_points_is_rejected_quickly_not_accepted() {
        let mut src = String::new();
        for i in 0..500 {
            src.push_str(&format!("p{i} = free\n"));
        }
        src.push_str("prove coll(p0, p0, p0)");
        let start = std::time::Instant::now();
        let result = compile(&src);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(1),
            "rejection must happen before the expensive instance-search loop, \
             not after — it took {:?}",
            start.elapsed()
        );
        assert!(result.is_err(), "500 points should be rejected");
        assert!(result.unwrap_err().contains("too many points"));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd ~/alphageometry-studio && cargo test -p ddar too_many_points -- --nocapture`
Expected: FAIL (500 points are accepted today — the test may also simply be slow rather than fail cleanly, since nothing stops the instance-search loop from running; either outcome demonstrates the gap).

- [ ] **Step 3: Add the point-count guard**

In `geo.rs`, immediately before `pub fn compile(src: &str) -> Result<Compiled, String> {` (currently `geo.rs:2897`), add:

```rust
/// IMO/JGEX corpus problems (`corpus/imo_ag_30.txt`, `corpus/jgex_ag_231.txt`)
/// use roughly 5-15 points; 100 leaves an order of magnitude of headroom for
/// legitimately complex constructions while keeping `Ddar::new`'s O(n^2)
/// allocation, `deduction_closure`'s O(n^3) search, and `compile`'s up-to-400
/// re-sampling attempts all bounded well short of a memory/CPU exhaustion DoS
/// from a crafted `.geo` program (see `Ddar::new_with_slack`, `engine.rs:194`).
const MAX_POINTS: usize = 100;

/// Total point count declared across a parsed program — shared by every
/// public entry point (`compile`, `build_instances`, `build_algebraic`) so
/// the guard below applies uniformly.
fn point_count(stmts: &[Stmt]) -> usize {
    stmts
        .iter()
        .map(|s| match s {
            Stmt::Bind { names, .. } => names.len(),
            Stmt::Constrain { .. } => 1,
            _ => 0,
        })
        .sum()
}

fn check_point_count(stmts: &[Stmt]) -> Result<(), String> {
    let n = point_count(stmts);
    if n > MAX_POINTS {
        return Err(format!(
            "too many points ({n}); this engine supports at most {MAX_POINTS} in one problem"
        ));
    }
    Ok(())
}
```

Then in `compile` (currently `geo.rs:2897-2903`):

```rust
pub fn compile(src: &str) -> Result<Compiled, String> {
    let toks = tokenize(src)?;
    let mut parser = Parser { toks, pos: 0, depth: 0 };
    let stmts = parser.parse_program()?;
    if stmts.is_empty() {
        return Err("empty program".to_string());
    }
```

add the check right after the `is_empty` check:

```rust
pub fn compile(src: &str) -> Result<Compiled, String> {
    let toks = tokenize(src)?;
    let mut parser = Parser { toks, pos: 0, depth: 0 };
    let stmts = parser.parse_program()?;
    if stmts.is_empty() {
        return Err("empty program".to_string());
    }
    check_point_count(&stmts)?;
```

Do the same in `build_instances` (currently `geo.rs:3007-3013`) and `build_algebraic` (currently `geo.rs:3076-3082`) — both have the identical `if stmts.is_empty() { return Err("empty construction".to_string()); }` shape; add `check_point_count(&stmts)?;` on the line right after it in each.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd ~/alphageometry-studio && cargo test -p ddar too_many_points -- --nocapture`
Expected: PASS, and fast (well under the 1-second assertion).

- [ ] **Step 5: Run the full engine test suite to check for regressions**

Run: `cd ~/alphageometry-studio && cargo test -p ddar`
Expected: all existing tests still PASS (100-point headroom is far beyond anything in the existing corpus-derived tests).

- [ ] **Step 6: Commit**

```bash
cd ~/alphageometry-studio
git add alphageometry-rs/src/geo.rs
git commit -m "geo: cap total point count to bound memory/CPU blowup from crafted input"
```

---

### Task 4: Log every silently-swallowed error path in `web.rs`

**Files:**
- Modify: `ag-studio/src/web.rs` — the `Err(_)` (JoinError) and previously-unlogged `Ok(Err(e))` arms in `api_solve` (`web.rs:429-436`), `api_export` (`web.rs:758-767`), `api_login` (`web.rs:281-285`), `api_register` (`web.rs:249-253`), `api_history` (`web.rs:491-494`), `api_history_delete` (`web.rs:513-517`), `api_translate` (`web.rs:583-593`), `api_humanize` (`web.rs:657-670`), and `decode_image`'s caller in `api_translate` (`web.rs:562-566`)

**Interfaces:**
- Consumes: nothing new.
- Produces: nothing later tasks depend on programmatically — this is an operator-visibility fix, verified in Task 7's live check by tailing `journalctl -u agstudio.service`.

**Context:** Every one of these handlers runs its real work inside `tokio::task::spawn_blocking` — if that closure panics, `.await` returns `Err(JoinError)`, which today gets turned into a generic 500 with **zero server-side log line** across all eight handlers. This is very likely the root cause of "proof didn't work" reports with no diagnostic trail: today, a panic inside `spawn_blocking` (deliberately caught, not a crash — see `Cargo.toml:7-9`) fires the safety net silently. `api_solve`'s and `api_export`'s `Ok(Err(e))` arms (an ordinary parse/solve error, not a panic) are also unlogged, unlike `api_translate`/`api_humanize`'s already-correct `Ok(Err(e))` arms (`web.rs:585-591`, `web.rs:659-665`) which this task should match, not duplicate.

- [ ] **Step 1: Fix `api_solve` (`web.rs:429-436`)**

Change:

```rust
    match task.await {
        Ok(Ok(sol)) => {
            save_history(&state, user.id, &sol, history_title.as_deref()).await;
            Json(sol).into_response()
        }
        Ok(Err(e)) => err(StatusCode::BAD_REQUEST, e), // compile/parse errors describe the user's input
        Err(_) => err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "solve.failed")),
    }
```

to:

```rust
    match task.await {
        Ok(Ok(sol)) => {
            save_history(&state, user.id, &sol, history_title.as_deref()).await;
            Json(sol).into_response()
        }
        Ok(Err(e)) => {
            eprintln!("solve error: {e}"); // detail to the operator's log, not the client
            err(StatusCode::BAD_REQUEST, e) // compile/parse errors describe the user's input
        }
        Err(e) => {
            eprintln!("solve panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "solve.failed"))
        }
    }
```

- [ ] **Step 2: Fix `api_export` (`web.rs:758-767`)**

Change:

```rust
    match res {
        Ok(Ok((bytes, content_type, disposition))) => Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_DISPOSITION, disposition)
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "export.failed"))),
        Ok(Err(e)) => err(StatusCode::BAD_REQUEST, format!("{e}")),
        Err(_) => err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "export.failed")),
    }
```

to:

```rust
    match res {
        Ok(Ok((bytes, content_type, disposition))) => Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_DISPOSITION, disposition)
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "export.failed"))),
        Ok(Err(e)) => {
            eprintln!("export error: {e}");
            err(StatusCode::BAD_REQUEST, format!("{e}"))
        }
        Err(e) => {
            eprintln!("export panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "export.failed"))
        }
    }
```

- [ ] **Step 3: Fix `api_login` (`web.rs:281-285`)**

Change:

```rust
    match outcome {
        Ok(Some((username, sid))) => auth_ok(&username, &sid, secure),
        Ok(None) => err(StatusCode::UNAUTHORIZED, i18n::t(lang, "auth.bad_creds")),
        Err(_) => err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "auth.login_fail")),
    }
```

to:

```rust
    match outcome {
        Ok(Some((username, sid))) => auth_ok(&username, &sid, secure),
        Ok(None) => err(StatusCode::UNAUTHORIZED, i18n::t(lang, "auth.bad_creds")),
        Err(e) => {
            eprintln!("login panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "auth.login_fail"))
        }
    }
```

- [ ] **Step 4: Fix `api_register` (`web.rs:249-253`)**

Change:

```rust
    match outcome {
        Ok(Ok((username, sid))) => auth_ok(&username, &sid, secure),
        Ok(Err(RegisterErr::Taken)) => err(StatusCode::CONFLICT, i18n::t(lang, "auth.user_taken")),
        _ => err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "auth.create_fail")),
    }
```

to:

```rust
    match outcome {
        Ok(Ok((username, sid))) => auth_ok(&username, &sid, secure),
        Ok(Err(RegisterErr::Taken)) => err(StatusCode::CONFLICT, i18n::t(lang, "auth.user_taken")),
        Ok(Err(RegisterErr::Internal)) => {
            eprintln!("register failed: internal error hashing/creating the account");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "auth.create_fail"))
        }
        Err(e) => {
            eprintln!("register panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "auth.create_fail"))
        }
    }
```

- [ ] **Step 5: Fix `api_history` (`web.rs:491-494`) and `api_history_delete` (`web.rs:513-517`)**

In `api_history`, change:

```rust
    match res {
        Ok(Ok(rows)) => Json(rows.into_iter().map(HistoryItem::from).collect::<Vec<_>>()).into_response(),
        _ => err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "history.load_fail")),
    }
```

to:

```rust
    match res {
        Ok(Ok(rows)) => Json(rows.into_iter().map(HistoryItem::from).collect::<Vec<_>>()).into_response(),
        Ok(Err(())) => {
            eprintln!("history load failed: db error");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "history.load_fail"))
        }
        Err(e) => {
            eprintln!("history load panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "history.load_fail"))
        }
    }
```

In `api_history_delete`, change:

```rust
    match res {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => err(StatusCode::NOT_FOUND, i18n::t(i18n::lang_from_headers(&headers), "history.none")),
        _ => err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "history.del_fail")),
    }
```

to:

```rust
    match res {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => err(StatusCode::NOT_FOUND, i18n::t(i18n::lang_from_headers(&headers), "history.none")),
        Ok(Err(())) => {
            eprintln!("history delete failed: db error");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "history.del_fail"))
        }
        Err(e) => {
            eprintln!("history delete panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "history.del_fail"))
        }
    }
```

- [ ] **Step 6: Fix `api_translate`'s JoinError arm and `decode_image`'s discarded error (`web.rs:562-593`)**

Change the `decode_image` call site:

```rust
    let held = match (req.image_base64, req.text) {
        (Some(b64), _) => match decode_image(&b64, req.filename.as_deref()) {
            Ok(tmp) => Held::Image(tmp),
            Err(_) => return err(StatusCode::BAD_REQUEST, i18n::t(i18n::lang_from_headers(&headers), "translate.bad_image")),
        },
```

to:

```rust
    let held = match (req.image_base64, req.text) {
        (Some(b64), _) => match decode_image(&b64, req.filename.as_deref()) {
            Ok(tmp) => Held::Image(tmp),
            Err(e) => {
                eprintln!("translate: bad image upload: {e}");
                return err(StatusCode::BAD_REQUEST, i18n::t(i18n::lang_from_headers(&headers), "translate.bad_image"));
            }
        },
```

And change the response match at the end of `api_translate`:

```rust
    match res {
        Ok(Ok(t)) => Json(t).into_response(),
        Ok(Err(e)) => {
            eprintln!("translate error: {e}"); // detail to the operator's log, not the client
            err(
                StatusCode::BAD_GATEWAY,
                "translation failed — could not turn that into a geometry problem",
            )
        }
        Err(_) => err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "translate.failed")),
    }
```

to:

```rust
    match res {
        Ok(Ok(t)) => Json(t).into_response(),
        Ok(Err(e)) => {
            eprintln!("translate error: {e}"); // detail to the operator's log, not the client
            err(
                StatusCode::BAD_GATEWAY,
                "translation failed — could not turn that into a geometry problem",
            )
        }
        Err(e) => {
            eprintln!("translate panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "translate.failed"))
        }
    }
```

- [ ] **Step 7: Fix `api_humanize`'s JoinError arm (`web.rs:657-670`)**

Change:

```rust
    match res {
        Ok(Ok(text)) => Json(serde_json::json!({ "proof": text })).into_response(),
        Ok(Err(e)) => {
            eprintln!("humanize error: {e}"); // detail to the operator's log, not the client
            err(
                StatusCode::SERVICE_UNAVAILABLE,
                i18n::t(lang, "humanize.failed"),
            )
        }
        Err(_) => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            i18n::t(lang, "humanize.failed"),
        ),
    }
```

to:

```rust
    match res {
        Ok(Ok(text)) => Json(serde_json::json!({ "proof": text })).into_response(),
        Ok(Err(e)) => {
            eprintln!("humanize error: {e}"); // detail to the operator's log, not the client
            err(
                StatusCode::SERVICE_UNAVAILABLE,
                i18n::t(lang, "humanize.failed"),
            )
        }
        Err(e) => {
            eprintln!("humanize panicked: {e}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                i18n::t(lang, "humanize.failed"),
            )
        }
    }
```

- [ ] **Step 8: Build and run the existing web test suite to check for regressions**

Run: `cd ~/alphageometry-studio && cargo build -p ag-studio && cargo test -p ag-studio`
Expected: builds cleanly (note: `db::list_history`/`db::delete_history` return `Result<_, ()>` per the existing `.map_err(|_| ())` calls at `web.rs:487-489`/`web.rs:507-510` — the `Ok(Err(()))` match arms above must compile against that `()` error type as written) and all existing tests PASS (these are pure logging additions — no response bodies or status codes changed).

- [ ] **Step 9: Commit**

```bash
cd ~/alphageometry-studio
git add ag-studio/src/web.rs
git commit -m "web: log every previously-silent error/panic path so incidents are diagnosable"
```

---

### Task 5: Isolate panics in the MCP server so one bad tool call can't kill the whole process

**Files:**
- Modify: `ag-studio/src/mcp.rs:31-48` (the `serve` request loop)
- Test: `ag-studio/src/mcp.rs` (new `#[cfg(test)] mod tests` block — check first whether one exists with `grep -n "mod tests" ag-studio/src/mcp.rs`)

**Interfaces:**
- Consumes: `handle(&Value) -> Option<Value>` (existing, `mcp.rs:53`).
- Produces: `fn catch_panics(id: Option<Value>, f: impl FnOnce() -> Option<Value>) -> Option<Value>` — a small reusable wrapper, directly unit-tested in this task.

**Context:** Unlike `web.rs`, which isolates every `engine::`/`render::` call inside `tokio::task::spawn_blocking` (so a panic there becomes a `JoinError`, not a process crash), `mcp.rs` calls `engine::solve`/`engine::solve_best` and — critically — `render::svg_to_png`/`render::svg_to_pdf` **directly on the single-threaded main loop** (`mcp.rs:198`, `mcp.rs:235`, `mcp.rs:259`, `mcp.rs:264`), with no `spawn_blocking` and no `catch_unwind` anywhere above `main()`. `render::render_figure`'s `catch_unwind` (used internally by `engine::solve`) only covers SVG *string generation*, not the PNG/PDF *rasterization* step that runs afterward in `tool_solve`/`tool_export` — so any panic reachable there (a resvg/tiny_skia edge case, or an unguarded parse/compile edge case) kills the entire MCP server process for that Claude session, not just the one tool call.

- [ ] **Step 1: Write the failing test**

Add at the end of `mcp.rs` (or inside its existing test module, if `grep -n "mod tests" ag-studio/src/mcp.rs` found one):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catch_panics_converts_a_panic_into_a_jsonrpc_error_response() {
        let id = Some(json!(7));
        let resp = catch_panics(id, || panic!("simulated engine panic"));
        let resp = resp.expect("a panicking request with an id must still get a response");
        assert_eq!(resp["error"]["code"], -32000);
        assert_eq!(resp["id"], 7);
    }

    #[test]
    fn catch_panics_passes_through_normal_results_unchanged() {
        let resp = catch_panics(Some(json!(1)), || Some(json!({"ok": true})));
        assert_eq!(resp, Some(json!({"ok": true})));
    }

    #[test]
    fn catch_panics_returns_none_for_a_panicking_notification() {
        // Notifications (no id) get no reply even when they panic — mirrors
        // `handle`'s existing behavior of returning `None` for notifications.
        let resp = catch_panics(None, || panic!("simulated panic on a notification"));
        assert_eq!(resp, None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd ~/alphageometry-studio && cargo test -p ag-studio --lib mcp::tests -- --nocapture`
Expected: FAIL with a compile error — `catch_panics` doesn't exist yet.

- [ ] **Step 3: Add `catch_panics` and wrap the request loop**

Add this function near `handle` in `mcp.rs` (e.g. right before `fn handle`):

```rust
/// Runs `f`, converting any panic into a JSON-RPC error response instead of
/// letting it propagate — which would otherwise kill this entire per-session
/// MCP process (unlike `ag-studio`'s web server, nothing here runs inside a
/// `tokio::task::spawn_blocking`, so there is no panic-isolation boundary
/// above this one). Mirrors the `catch_unwind(AssertUnwindSafe(...))`
/// convention used throughout `engine.rs`/`aux_search.rs`.
fn catch_panics(id: Option<Value>, f: impl FnOnce() -> Option<Value>) -> Option<Value> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(resp) => resp,
        Err(_) => {
            eprintln!("request handler panicked; returning an error instead of crashing");
            id.map(|id| {
                json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": { "code": -32000, "message": "internal error while handling this request" }
                })
            })
        }
    }
}
```

Then change the request loop in `serve` (currently `mcp.rs:31-48`):

```rust
    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("ignoring malformed JSON-RPC line: {e}");
                continue;
            }
        };
        if let Some(resp) = handle(&req) {
            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
            stdout.flush()?;
        }
    }
```

to:

```rust
    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("ignoring malformed JSON-RPC line: {e}");
                continue;
            }
        };
        let id = req.get("id").cloned();
        if let Some(resp) = catch_panics(id, || handle(&req)) {
            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
            stdout.flush()?;
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd ~/alphageometry-studio && cargo test -p ag-studio --lib mcp::tests -- --nocapture`
Expected: all three PASS.

- [ ] **Step 5: Run the full ag-studio test suite to check for regressions**

Run: `cd ~/alphageometry-studio && cargo test -p ag-studio`
Expected: all existing tests still PASS.

- [ ] **Step 6: Commit**

```bash
cd ~/alphageometry-studio
git add ag-studio/src/mcp.rs
git commit -m "mcp: isolate panics per-request so one bad tool call can't kill the session"
```

---

### Task 6: Adversarial regression tests through the HTTP API

**Files:**
- Modify: `ag-studio/src/web.rs` (add tests to the existing `#[cfg(test)] mod tests` block, `web.rs:770+`)

**Interfaces:**
- Consumes: `call`, `register_cookie`, `test_state` (existing test helpers, `web.rs:779-919`), and the Task 1/Task 3 engine-level fixes (via `engine::solve`'s error propagation into `api_solve`'s `Ok(Err(e))` arm, now logged per Task 4).
- Produces: nothing further depends on this — it's the end-to-end proof that Tasks 1–4 actually protect the real request path a user hits, not just the engine crate in isolation.

**Context:** Tasks 1–3 test the DDAR engine directly; this task proves the same adversarial inputs are rejected cleanly (400, not a hang, timeout, or connection drop) when they arrive the way a real user's browser would — through `/api/solve`. Both payloads below must stay under the 16,384-char `AGSTUDIO_MAX_INPUT_CHARS` default (checked by `check_input`, `web.rs:327-339`) so they actually reach the parser/engine instead of being rejected earlier as oversized — that's deliberate: this test is proving the *parser's* depth/point-count guards work, not the unrelated size limit.

- [ ] **Step 1: Write the tests**

Add to the `tests` module in `web.rs` (e.g. after `history_is_recorded_and_scoped_per_user`, `web.rs:991`):

```rust
    /// A `.geo` program with 500 levels of unary-minus nesting is well under
    /// the 16,384-char body limit (so it reaches the parser), but must be
    /// rejected as a clean 400 by the depth cap added in geo.rs — not hang,
    /// time out, or (in the pre-fix world) risk a stack overflow.
    #[tokio::test]
    async fn deeply_nested_solve_input_is_rejected_cleanly() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "sam").await;
        let mut input = String::from("prove dist(A, B) = ");
        input.push_str(&"-".repeat(500));
        input.push('5');
        assert!(input.len() < 16384, "test input must stay under the body cap");
        let (st, _, body) = call(
            &state,
            "POST",
            "/api/solve",
            Some(&cookie),
            serde_json::json!({"input": input}),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "got: {body:?}");
    }

    /// A `.geo` program declaring 500 points is well under the body limit,
    /// but must be rejected quickly by the point-count cap added in geo.rs —
    /// not accepted and left to blow up `Ddar::new`'s O(n^2) allocation.
    #[tokio::test]
    async fn too_many_points_solve_input_is_rejected_quickly() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "tara").await;
        let mut input = String::new();
        for i in 0..500 {
            input.push_str(&format!("p{i} = free\n"));
        }
        input.push_str("prove coll(p0, p0, p0)");
        assert!(input.len() < 16384, "test input must stay under the body cap");
        let start = std::time::Instant::now();
        let (st, _, body) = call(
            &state,
            "POST",
            "/api/solve",
            Some(&cookie),
            serde_json::json!({"input": input}),
        )
        .await;
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "rejection must be fast, not run the expensive instance-search loop first"
        );
        assert_eq!(st, StatusCode::BAD_REQUEST, "got: {body:?}");
    }

    /// Firing several adversarial and several valid solves concurrently must
    /// not let one request's failure affect another's success — each request
    /// gets its own `spawn_blocking` task and its own response.
    #[tokio::test]
    async fn concurrent_adversarial_and_valid_solves_do_not_interfere() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "uma").await;
        let mut bad_input = String::from("prove dist(A, B) = ");
        bad_input.push_str(&"-".repeat(500));
        bad_input.push('5');

        let mut tasks = Vec::new();
        for i in 0..8 {
            let state = state.clone();
            let cookie = cookie.clone();
            let input = if i % 2 == 0 {
                bad_input.clone()
            } else {
                ISOSCELES_GEO.to_string()
            };
            let expect_ok = i % 2 != 0;
            tasks.push(tokio::spawn(async move {
                let (st, _, body) = call(
                    &state,
                    "POST",
                    "/api/solve",
                    Some(&cookie),
                    serde_json::json!({"input": input}),
                )
                .await;
                let expected = if expect_ok { StatusCode::OK } else { StatusCode::BAD_REQUEST };
                assert_eq!(st, expected, "request {i} got: {body:?}");
            }));
        }
        for t in tasks {
            t.await.expect("request task panicked");
        }
    }
```

- [ ] **Step 2: Run the tests**

Run: `cd ~/alphageometry-studio && cargo test -p ag-studio`
Expected: all tests PASS, including the three new ones — this requires Tasks 1, 3, and 4 to already be done (the fixes these tests exercise live in `geo.rs`/`web.rs`).

- [ ] **Step 3: Commit**

```bash
cd ~/alphageometry-studio
git add ag-studio/src/web.rs
git commit -m "web: add adversarial and concurrent regression tests for the crash-surface fixes"
```

---

### Task 7: Build, deploy, and verify live against the running service

**Files:** none (build/ops task — no source changes)

**Interfaces:**
- Consumes: the binary produced by `cargo build --release -p ag-studio`, and the running `agstudio.service` (`/etc/systemd/system/agstudio.service`, binary at `/opt/agstudio/agstudio`).
- Produces: a verified-working live deployment; nothing later depends on this programmatically.

**Context:** All fixes so far are proven by `cargo test`, run against a throwaway in-memory/tempdir test server (`test_state()`). This task closes the loop against the actual production binary and the actual shared `agstudio.service` at `100.95.247.89:8787` that real users hit — per the earlier agreement, brief restart downtime and adversarial/concurrent live traffic are both fine.

- [ ] **Step 1: Run the full workspace test suite one more time**

Run: `cd ~/alphageometry-studio && cargo test`
Expected: every test in both crates passes (this re-confirms Tasks 1–6 together, not just individually).

- [ ] **Step 2: Build the release binary**

Run: `cd ~/alphageometry-studio && cargo build --release -p ag-studio`
Expected: builds successfully, producing `target/release/agstudio`.

- [ ] **Step 3: Deploy and restart the service**

```bash
cp ~/alphageometry-studio/target/release/agstudio /opt/agstudio/agstudio
systemctl restart agstudio.service
systemctl status agstudio.service --no-pager
```
Expected: `Active: active (running)`, and the startup banner in `journalctl -u agstudio.service -n 20 --no-pager` shows the usual `GeoSolver → http://<tailscale-ip>:8787` line with no errors.

- [ ] **Step 4: Smoke-test the golden path end to end**

```bash
curl -s http://100.95.247.89:8787/healthz
```
Expected: `ok`. Then, using a real browser session (or `curl` with a registered/logged-in session cookie), drive one full translate → solve → export cycle through the actual web UI at `http://100.95.247.89:8787` — enter a plain-English geometry problem, confirm it translates and solves, and download the PDF/PNG export — to confirm the logging changes in Task 4 didn't alter any success-path behavior.

- [ ] **Step 5: Fire the adversarial battery at the live instance**

From a registered/logged-in session, submit the same two adversarial payloads from Task 6 (500-level unary-minus chain; 500-point program) directly to `POST /api/solve` on `http://100.95.247.89:8787`, and confirm both return `400` promptly (not a hang, not a connection drop, not a 502/504).

- [ ] **Step 6: Fire concurrent load and confirm the service survives**

Run a small concurrent burst (e.g. 20 simultaneous `/api/solve` requests mixing the two adversarial payloads above with valid `.geo` programs, from a couple of different registered sessions to simulate multiple users) against the live instance, then check:

```bash
systemctl status agstudio.service --no-pager
journalctl -u agstudio.service --since "5 minutes ago" --no-pager | tail -60
```

Expected: `Active: active (running)` the whole time (no restart in the systemd status/journal), all adversarial requests logged per Task 4's new `eprintln!` lines (proving the diagnostic trail now exists), and all concurrent valid requests still succeeded normally — confirming multiple simultaneous users aren't affected by one another's bad input.

- [ ] **Step 7: Report findings**

Summarize what was fixed (stack-overflow-capable recursion in two parsers, unbounded point-count memory/CPU blowup, eight silent error/panic-logging gaps, one process-killing MCP panic path) and confirm via the journal output from Step 6 that the live service now logs enough detail to diagnose any future "it didn't work" report without needing a user-supplied repro.

