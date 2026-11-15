# Sanar Slate

> [!NOTE]
> **Repository Mirrors**
>
> - **git.nuros.org** ([sanarde/slate](https://git.nuros.org/sanarde/slate)): primary, self-hosted Forgejo instance, accounts restricted to the core team.
> - **GitHub** ([sanarde/slate](https://github.com/sanarde/slate)): mirror for external contributors. Issues and Pull Requests opened here are welcome and are reviewed and processed by the core team.

## About

Floating window manager of SanarDE that runs as a Wayland compositor built on Smithay and as a separate X11 window manager.

## Dependencies

- Rust 1.88 or newer (edition 2024) and Cargo
- libseat, libinput, libudev, libgbm, libxkbcommon
- libcfgd, the client library of [cfgd](https://git.nuros.org/sanarde/cfgd), found through `pkg-config`
- EGL and OpenGL ES (Mesa)

Optional at run time:

- Xwayland, to run X11 applications in the Wayland session
- The cfgd daemon `cfgdd`, for settings. Without it the defaults apply.

## Build

```
cargo build --release
```

The binary is `target/release/slate`. When libcfgd is not installed, point `PKG_CONFIG_PATH` at the directory with its `.pc` file. The files in `data` are installed by hand: `slate-wayland.desktop` to `share/wayland-sessions`, `slate-x11.desktop` to `share/xsessions` and `org.nuros.sanar.slate.schema` to `share/cfgd/schemas`.

## Usage

```
slate [--backend auto|winit|udev|x11] [COMMAND [ARGUMENT...]]
```

`COMMAND` is started once the session is up. Backends:

- `udev`: Wayland compositor on a TTY or from a display manager. Uses DRM, libinput and libseat.
- `winit`: Wayland compositor in a window of another Wayland or X11 session, for development.
- `x11`: window manager for a running X server. Does not use Smithay.
- `auto`: `winit` when `WAYLAND_DISPLAY` or `DISPLAY` is set, otherwise `udev`. The `x11` backend is never chosen automatically.

Key bindings. Mod is Super, except in `winit` where it is Alt.

| Keys | Action |
| --- | --- |
| Mod+Return | Start `$TERMINAL`, `foot` on Wayland and `xterm` on X11 by default |
| Mod+q | Close the focused window |
| Mod+m | Toggle maximized |
| Mod+f | Toggle fullscreen |
| Mod+1 to Mod+9 | Switch to a workspace |
| Mod+Shift+1 to Mod+Shift+9 | Move the focused window to a workspace |
| Mod+Shift+e | Quit |
| Mod+left button, drag | Move the window |
| Mod+right button, drag | Resize the window |
| Ctrl+Alt+F1 to F12 | Switch virtual terminal (`udev` only) |

Focus follows clicks. There are nine workspaces shared by all outputs.

The Wayland backend implements xdg-shell, wlr-layer-shell, wlr-output-management, wlr-screencopy, linux-dmabuf, xdg-output, xdg-activation, the data device and primary selection, viewporter, cursor-shape, relative-pointer, pointer-gestures, single-pixel-buffer, idle-notify and XWayland. Output management supports position, scale, transform, mode and enabling or disabling outputs. Screen capture copies into shared memory buffers, so `grim` works, and portal based tools need `xdg-desktop-portal-wlr`. Fractional scales are rounded up for clients. In a nested `winit` window only position and scale can change.

The X11 backend supports the EWMH hints needed by panels and wallpaper programs: window types `DOCK` and `DESKTOP`, struts, `_NET_ACTIVE_WINDOW`, `_NET_WM_STATE` including above, fullscreen and maximized at map time, `_NET_CURRENT_DESKTOP`, the `WM_HINTS` input flag and RandR monitors.

## Settings

Settings are keys of the cfgd schema `org.nuros.sanar.slate`, path `/org/nuros/sanar/slate/`. Changes apply without a restart.

| Key | Default | Applies to |
| --- | --- | --- |
| `mod-key` | `auto` | Both. `auto` is Super, or Alt in `winit`. Values: `auto`, `super`, `alt`. |
| `terminal` | empty | Both. Command of Mod+Return, empty uses `$TERMINAL` and then `foot` or `xterm`. |
| `workspace-count` | 9 | Both. From 1 to 9. |
| `border-width` | 2 | X11. |
| `border-color-focused`, `border-color-unfocused` | `#6750a4`, `#49454f` | X11. Form `#rrggbb`. |
| `xkb-rules`, `xkb-model`, `xkb-layout`, `xkb-variant`, `xkb-options` | empty | Wayland. Empty uses the `XKB_DEFAULT_*` variables. |
| `repeat-delay`, `repeat-rate` | 200, 25 | Wayland. |
| `cursor-theme`, `cursor-size` | empty, 0 | Wayland. Empty and 0 use `XCURSOR_THEME`, `XCURSOR_SIZE` and then `default`, 24. |
| `tap-to-click`, `natural-scroll` | true, false | Wayland on a TTY. |

```
cfgd set /org/nuros/sanar/slate/xkb-layout 'us,ru'
cfgd set /org/nuros/sanar/slate/xkb-options 'grp:alt_shift_toggle'
```

## Roadmap

- Tiling layout as a setting, floating stays the default.
- Configurable key bindings.
- Server-side decorations (xdg-decoration).
- Fractional scale protocol, text input, session lock and ext-image-copy-capture.

## Acknowledgments

- [Smithay](https://github.com/Smithay/smithay), the Wayland compositor library.
- [x11rb](https://github.com/psychon/x11rb), the X11 protocol bindings.

## Links

- [CONTRIBUTING](CONTRIBUTING.md)

## License

GPL-3.0-only. See [LICENSE](LICENSE).
