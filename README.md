# carli

`carli` is a small Unix shell written in Rust as a learning project. It is not
yet suitable for use as a login shell.

## Current features

- Runs external programs found through `PATH`
- Built-ins: `cd`, `pwd`, and `exit`
- Single quotes, double quotes, and backslash escapes
- Graceful handling of blank input, EOF, and command errors

## Try it

```sh
cargo run
```

Then try `pwd`, `echo "hello from carli"`, `cd /tmp`, and `exit`.

Run parser tests with `cargo test`.

## Roadmap

Next: environment expansion and built-ins, redirections, pipelines, signals and
process groups, then startup/login-shell support. Do not register `carli` with
`chsh` until those foundations are in place.
