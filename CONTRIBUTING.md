# Contributing to Pearl

Thank you for helping. People trust Pearl with their writing, so a few rules are strict. The full
set, for people and AI agents alike, is in [`AGENTS.md`](AGENTS.md); this page is the short version.

## Before you start

Pick from [`docs/roadmap.md`](docs/roadmap.md), and read [`docs/architecture.md`](docs/architecture.md).

## The rules that are never bent

- **No one else's assets.** Every asset file needs a row in [`ATTRIBUTION.md`](ATTRIBUTION.md).
- **No GPL, AGPL or LGPL code**, and permissively licensed dependencies; exceptions are listed in `AGENTS.md`.
- **Never crash.** No `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` or `unsafe` outside tests. A
  crash fix comes with a regression test.
- **Text in the catalogs.** User-visible text goes in `locales/en.ftl` and `locales/pt-BR.ftl`.

## Making the change

```sh
cargo xtask ci        # every quality gate; must pass
cargo test            # the tests alone
```

CI runs the same on every pull request.

## Commits and pull requests

- One change per commit, its message saying what and why; `Fixes #n` when it closes an issue.
- Add a line to `CHANGELOG.md` under `## [Unreleased]` for anything a user would notice.
- Update the docs your change affects in the same pull request.

## Reporting

Bugs and feature requests: [GitHub issues](https://github.com/morrisonkuhlsen/mkpearl/issues/new/choose).
Security problems: privately, as [`SECURITY.md`](SECURITY.md) says, never in a public issue.

By contributing, you agree that your contribution is licensed under MIT OR Apache-2.0, as the rest
of the project.
