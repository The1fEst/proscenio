# The compat layer

`proscenio` itself knows nothing of the QML shell. Everything it does to stand
in for it lives in `src/compat/` and is compiled only with the `compat` feature:

```
cargo build --release          # proscenio on its own
cargo release-compat           # the same, with the compat layer
```

`cargo release-compat` is an alias from `.cargo/config.toml` for
`cargo build --release --features compat`. Both write
`target/release/proscenio`.

## What the layer does

- **Global shortcuts.** `proscenio` publishes its actions under the app id
  `proscenio` ([shortcuts.md](shortcuts.md)). The layer publishes them again
  under `quickshell`, the names the binds in the dots use, unless a `qs` or
  `quickshell` process is running: registering a taken name is a fatal
  protocol error.
- **Settings pages by component path.** `settings openPage` also takes the QML
  shell's `modules/settings/…Config.qml` paths and opens the matching page
  ([ipc.md](ipc.md)).
- **The QML shell's data.** On start, it copies the files `proscenio` lacks
  from the QML shell's places and turns `illogical-impulse/config.json` into
  `config.toml` ([files.md](files.md)).
- **The shell's name.** Labels that name the shell, such as "Reload Hyprland &
  Quickshell", say "Quickshell" instead of "proscenio".

## Leaving the QML shell behind

Delete `src/compat/`, the `compat` feature in `Cargo.toml`, the alias in
`.cargo/config.toml`, and the three lines in `src/main.rs` that name the
module, `compat::prepare()` and `compat::install(…)`, and `set_name` in
`src/core/shell.rs`. In the dots, the binds then change from
`quickshell:` to `proscenio:`.
