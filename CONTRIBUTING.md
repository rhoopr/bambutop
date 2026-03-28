# Contributing

PRs welcome, along with feedback and feature requests. For anything larger, open an issue or discussion first so the approach can be talked through before you put in the work.

## Before You Submit

Make sure all three pass:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs these on every PR. If CI is red, the PR won't get merged.

## Code Style

- No `#[allow(...)]` - fix the warning instead
- No `.unwrap()` in production code (tests are fine)
- Use `anyhow` with `.context()` for error handling
- Ratatui styles: `Style::new()` not `Style::default()`

## Tests

New functionality should have tests. You don't need 100% coverage, but the happy path and obvious edge cases should be covered.

## Commits

Write a short commit message that says what changed and why. No particular format required.
