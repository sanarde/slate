# Contributing

- Build and run the linter before sending a change: `cargo build && cargo clippy`. libcfgd must be found by `pkg-config`, see the README.
- There are no automated tests. Check changes by hand: the Wayland backend with `slate --backend winit foot` and an X11 client such as `xterm`, the X11 backend on a display such as `Xvfb :99` with `DISPLAY=:99 slate --backend x11 xterm`. The `udev` backend needs a free virtual terminal.
- Keep each change limited to one area.
- Report bugs with the version, the steps to reproduce, and the observed output.

Source layout:

- `src/main.rs`: argument parsing and backend choice.
- `src/spawn.rs`: starting programs.
- `src/cfgd.rs` and `build.rs`: the binding to libcfgd. `src/config.rs`: the settings read through it, shared by both backends.
- `src/wayland`: the Wayland compositor. `state.rs` holds the compositor state, `settings.rs` live changes of settings, `handlers` the protocol handlers, `outputs.rs` the registry of outputs and applying a configuration, `output_management.rs` the wlr-output-management protocol, `shell.rs` window placement, focus and states, `grabs.rs` interactive move and resize, `input.rs` key bindings and pointer routing, `render.rs` and `cursor.rs` drawing, `winit.rs` and `udev.rs` the backends.
- `src/x11`: the X11 window manager on x11rb, independent of the Wayland code.
- `data`: session entries for display managers and the cfgd schema.
