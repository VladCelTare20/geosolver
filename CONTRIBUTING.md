# Contributing

Pull requests are welcome.

Before opening one:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --release
cargo test --workspace --release
cargo run --release -p alphageometry-rs --bin ddar -- --bench   # must stay 26/26
```

Ground rules:

- **Soundness first.** The prover must never report a false statement as proved.
  Any change to deduction rules, goal compilation or the metric/ratio provers
  needs a regression test with a *false* statement that must stay unproved.
- Keep the engine pure Rust with no Python dependency.
- Translation uses the local `claude` CLI on a Claude subscription — never an API key.
- In `mcp` mode stdout is the protocol channel; nothing else may print there.

By contributing you agree your contribution is licensed under Apache-2.0
(see `LICENSE` and `NOTICE`).
