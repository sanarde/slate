# Contributing

- Build and run the linter before sending a change: `cargo build && cargo clippy`.
- Run the unit tests with `cargo test`. The rest has no automated tests, check changes by hand: the Wayland backend with `slate --backend winit foot` and an X11 client such as `xterm`, the X11 backend on a display such as `Xvfb :99` with `DISPLAY=:99 slate --backend x11 xterm`. The `udev` backend needs a free virtual terminal.
- Keep each change limited to one area.
- Report bugs with the version, the steps to reproduce, and the observed output.

Source layout:

- `src/main.rs`: argument parsing and backend choice.
- `src/spawn.rs`: starting programs.
- `src/layout.rs`: tiling layouts as pure geometry, shared by both backends. `src/wayland/tiling.rs` and `src/x11/wm/tiling.rs` apply them.
- `src/overview.rs`: layout of the overview grid and the switcher row, shared geometry with tests. `src/wayland/switcher.rs` draws them with live scaled windows and handles their keys. `src/wayland/clip.rs` (rounded content and dimming through a texture shader), `src/wayland/closing.rs` (close animation from an offscreen snapshot taken when the close is requested) and `src/wayland/blur.rs` (backdrop render and separable gaussian passes) are the render effects.
- `src/monitors.rs`: picking the neighbour monitor and keeping a moved frame inside the target, shared by both backends. `src/wayland/monitors.rs` and `src/x11/wm/monitors.rs` apply it. Gestures and touch are in `src/wayland/input.rs`.
- `src/rules.rs`: window rules and scratchpad declarations as pure functions, applied by `place_window` on Wayland and `manage` on X11. `src/session.rs`: exec-once, XDG autostart and the environment import.
- `src/ipc.rs`, `src/json.rs` and `src/bin/slatectl.rs`: the IPC server (threads and an eventfd, the backends answer in their own loop), the JSON writer and the client. `src/wayland/ipc.rs` and `src/x11/wm/ipc.rs` implement the state and the commands, `src/wayland/foreign_toplevel.rs` the wlr-foreign-toplevel-management protocol.
- `src/bindings.rs`: parsing and matching of key bindings, shared by both backends.
- `src/config.rs`: the setting types, their defaults and parsing from values, shared by both backends. `src/source.rs`: the layers (defaults, cfgd, theme, file), variables and color roles, `--dump-config`. `src/conf.rs`: the file format. `src/themes.rs` and `data/themes`: the themes. `src/scheme.rs`: the Seurat client and the baseline palette. `src/cfgd.rs`: libcfgd loaded at run time.
- `src/wayland`: the Wayland compositor. `state.rs` holds the compositor state, `settings.rs` live changes of settings, `handlers` the protocol handlers, `decoration.rs` frame geometry, hit testing and interaction, `outputs.rs` the registry of outputs and applying a configuration, `output_management.rs` the wlr-output-management protocol, `screencopy.rs` the wlr-screencopy protocol, `animation.rs` animations of windows, `shadow.rs` window shadows, `lock.rs` the session lock, `scale.rs` the fractional scale, `shell.rs` window placement, focus and states, `grabs.rs` interactive move and resize, `input.rs` key bindings and pointer routing, `render.rs` and `cursor.rs` drawing, `winit.rs` and `udev.rs` the backends.
- `src/titlebar.rs` and `src/font.rs`: drawing of the titlebar, shared by both backends.
- `src/x11`: the X11 window manager on x11rb, independent of the Wayland code. `wm/frame.rs` the frame windows, their drawing and mouse handling, `wm/tiling.rs` tiling.
- `data`: session entries for display managers and the cfgd schema.
