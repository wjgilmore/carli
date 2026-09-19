# Changelog

All notable changes to `carli` are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-09-18

### Added

- Multi-stage pipelines with `|`, last-stage status semantics, per-stage
  redirection, isolated built-ins, shared process groups, and complete
  foreground job control.

## [0.1.0] - 2026-09-18

### Added

- Interactive command editing with persistent history and configurable prompts.
- Quoting, escaping, environment-variable expansion, and `$?` expansion.
- External command execution through `PATH` and core stateful built-ins.
- Input, truncating-output, and appending-output redirection.
- Foreground process groups, terminal signal routing, and terminal-state recovery.
- Basic `jobs`, `fg`, and `bg` job control.
- Command, batch, interactive, and login invocation modes.
- XDG-aware startup configuration and graceful hangup cleanup.
- Transactional installation and guarded uninstallation for Linux and macOS.
- Comprehensive unit, process, pseudo-terminal, and installer test coverage.

### Known limitations

- Pipelines, command separators, conditionals, command substitution, functions,
  loops, and globbing are not implemented.
- `carli` is not a Bash-, Zsh-, or POSIX-compatible scripting shell.

[Unreleased]: https://github.com/wjgilmore/carli/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/wjgilmore/carli/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/wjgilmore/carli/releases/tag/v0.1.0
