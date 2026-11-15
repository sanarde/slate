# Contributing

- Build and run the linter before sending a change: `cargo build && cargo clippy`.
- Run the unit tests with `cargo test`. The rest has no automated tests, check changes by hand: the Wayland backend with `slate --backend winit foot` and an X11 client such as `xterm`, the X11 backend on a display such as `Xvfb :99` with `DISPLAY=:99 slate --backend x11 xterm`. The `udev` backend needs a free virtual terminal.
- Keep each change limited to one area.
- Report bugs with the version, the steps to reproduce, and the observed output.

Source layout:

- `src/main.rs`: argument parsing and backend choice.
- `src/spawn.rs`: starting programs.
- `src/bindings.rs`: parsing and matching of key bindings, shared by both backends.
- `src/config.rs`: the setting types, their defaults and parsing from values, shared by both backends. `src/source.rs`: the layers (defaults, cfgd, theme, file), variables and color roles, `--dump-config`. `src/conf.rs`: the file format. `src/themes.rs` and `data/themes`: the themes. `src/scheme.rs`: the Seurat client and the baseline palette. `src/cfgd.rs`: libcfgd loaded at run time.
- `src/wayland`: the Wayland compositor. `state.rs` holds the compositor state, `settings.rs` live changes of settings, `handlers` the protocol handlers, `decoration.rs` frame geometry, hit testing and interaction, `titlebar.rs` and `font.rs` drawing of the titlebar, `outputs.rs` the registry of outputs and applying a configuration, `output_management.rs` the wlr-output-management protocol, `screencopy.rs` the wlr-screencopy protocol, `lock.rs` the session lock, `scale.rs` the fractional scale, `shell.rs` window placement, focus and states, `grabs.rs` interactive move and resize, `input.rs` key bindings and pointer routing, `render.rs` and `cursor.rs` drawing, `winit.rs` and `udev.rs` the backends.
- `src/x11`: the X11 window manager on x11rb, independent of the Wayland code.
- `data`: session entries for display managers and the cfgd schema.
