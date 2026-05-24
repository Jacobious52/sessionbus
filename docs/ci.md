# CI

The canonical local gate is:

```bash
bun run selftest
```

To enable GitHub Actions, add this workflow at `.github/workflows/ci.yml`.
Creating or updating that path through `gh` requires the `workflow` OAuth scope.

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  selftest:
    name: Selftest
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: stable
          components: rustfmt

      - name: Set up Bun
        uses: oven-sh/setup-bun@v2
        with:
          bun-version: latest

      - name: Install dependencies
        run: bun install --frozen-lockfile

      - name: Run selftest
        run: bun run selftest
```
