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
- EGL and OpenGL ES (Mesa)

Optional at run time:

- Xwayland, to run X11 applications in the Wayland session

## Build

```
cargo build --release
```

The binary is `target/release/slate`. The files in `data` are session entries for display managers: `slate-wayland.desktop` goes to `share/wayland-sessions`, `slate-x11.desktop` to `share/xsessions`.

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
| Mod+Shift+e | Quit |
| Mod+left button, drag | Move the window |
| Mod+right button, drag | Resize the window |
| Ctrl+Alt+F1 to F12 | Switch virtual terminal (`udev` only) |

Focus follows clicks. The Wayland backend implements xdg-shell, wlr-layer-shell, xdg-output, the data device and XWayland. The X11 backend supports the EWMH hints needed by panels and wallpaper programs: window types `DOCK` and `DESKTOP`, struts, `_NET_ACTIVE_WINDOW`, `_NET_WM_STATE` and RandR monitors.

## Roadmap

- Tiling layout as a setting, floating stays the default.
- Settings through cfgd.
- Server-side decorations (xdg-decoration).
- linux-dmabuf, so that GPU clients do not fall back to shared memory.
- Screen capture, idle and session lock protocols.
- Workspaces.

## Acknowledgments

- [Smithay](https://github.com/Smithay/smithay), the Wayland compositor library.
- [x11rb](https://github.com/psychon/x11rb), the X11 protocol bindings.

## Links

- [CONTRIBUTING](CONTRIBUTING.md)

## License

GPL-3.0-only. See [LICENSE](LICENSE).
