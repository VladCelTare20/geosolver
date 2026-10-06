# Contributing

Pull requests are welcome.

Before opening one:

```sh
cargo clippy --workspace --all-targets --release
RAYON_NUM_THREADS=6 cargo test --release --workspace -- --test-threads=6
cargo run --release -p alphageometry-rs --bin ddar -- --bench   # must stay 26/26
```

CI (`.github/workflows/ci.yml`) runs the same tests, the bench rows, an MSRV
(1.88) check, the Docker build with a smoke test, and checks that
`THIRD_PARTY_LICENSES.md` matches `Cargo.lock`. Run `cargo fmt` on the code
you change; the tree as a whole is not rustfmt-formatted yet, so do not
reformat files you are not otherwise editing.

Ground rules:

- **Soundness first.** The prover must never report a false statement as proved.
  Any change to deduction rules, goal compilation or the metric/ratio provers
  needs a regression test with a *false* statement that must stay unproved.
- **No coordinates in proofs.** Every proof is classical steps: deductions,
  cited theorems and trigonometry. Coordinates and numerics may only propose
  candidates, pick configuration branches and check non-degeneracy; a
  numerical check is never a proof (`status: holds-numerically`).
- Keep the engine pure Rust with no Python dependency.
- Translation uses the local `claude` CLI on a Claude subscription — never an API key.
- In `mcp` mode stdout is the protocol channel; nothing else may print there.
- **No explanatory comments in code.** Each code folder has a `CODEMAP.md`
  (one line per file, notes keyed to `file:symbol`), indexed from the root
  `CODEMAP.md`; update it in the same commit as the code. Older files still
  carry comments: move them into the CODEMAP when you next change that file,
  rather than stripping them in bulk.
- A change to the database schema bumps `ag-studio/src/db.rs:SCHEMA_VERSION`
  and adds its migration; a dependency change reruns
  `deploy/third-party-licenses.sh`.

## Releasing

1. Run the gate above and the corpus benchmark under the bench lock; put the
   numbers in the README tables if they moved.
2. Bump `version` in `ag-studio/Cargo.toml` and `alphageometry-rs/Cargo.toml`
   (the flake reads it) and the `org.opencontainers.image.version` label in
   the `Dockerfile`, and add the section to `CHANGELOG.md`.
3. Commit, tag `vX.Y.Z`, push the tag. `agstudio --version` and `/api/status`
   then report the version and the commit.

By contributing you agree your contribution is licensed under Apache-2.0
(see `LICENSE` and `NOTICE`).
