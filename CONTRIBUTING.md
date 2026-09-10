# Contributing

Bug reports and reproducible test cases are welcome. Before sending code, run:

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The project uses a dual-licensing model: this public edition is GPL-3.0-only,
while the copyright holder also develops a commercial edition. Code
contributions cannot be merged into shared Lite/Pro components until an
appropriate Contributor License Agreement is published and accepted. Opening
an issue does not transfer copyright or grant additional licensing rights.
