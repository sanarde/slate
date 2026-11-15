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

Key bindings. Mod is Super, except in `winit` where it is Alt. The bindings below are the built in ones and can be changed, see Settings.

| Keys | Action |
| --- | --- |
| Mod+Return | Start `$TERMINAL`, `foot` on Wayland and `xterm` on X11 by default |
| Mod+q | Close the focused window |
| Mod+m | Toggle maximized |
| Mod+f | Toggle fullscreen |
| Mod+Tab, Mod+Shift+Tab | Focus the next or the previous window |
| Mod+1 to Mod+9 | Switch to a workspace |
| Mod+Shift+1 to Mod+Shift+9 | Move the focused window to a workspace |
| Mod+Shift+e | Quit |
| Mod+left button, drag | Move the window |
| Mod+right button, drag | Resize the window |
| Ctrl+Alt+F1 to F12 | Switch virtual terminal (`udev` only) |

Focus follows clicks. There are nine workspaces shared by all outputs.

The Wayland backend implements xdg-shell, wlr-layer-shell, wlr-output-management, wlr-screencopy, ext-session-lock, fractional-scale, linux-dmabuf, xdg-output, xdg-activation, the data device and primary selection, viewporter, cursor-shape, relative-pointer, pointer-gestures, single-pixel-buffer, idle-notify and XWayland. Output management supports position, scale, transform, mode and enabling or disabling outputs. Screen capture copies into shared memory buffers, so `grim` works, and portal based tools need `xdg-desktop-portal-wlr`. In a nested `winit` window only position and scale can change.

The X11 backend supports the EWMH hints needed by panels and wallpaper programs: window types `DOCK` and `DESKTOP`, struts, `_NET_ACTIVE_WINDOW`, `_NET_WM_STATE` including above, fullscreen and maximized at map time, `_NET_CURRENT_DESKTOP`, the `WM_HINTS` input flag and RandR monitors.

## Settings

Settings are keys of the cfgd schema `org.nuros.sanar.slate`, path `/org/nuros/sanar/slate/`. Changes apply without a restart.

| Key | Default | Applies to |
| --- | --- | --- |
| `mod-key` | `auto` | Both. `auto` is Super, or Alt in `winit`. Values: `auto`, `super`, `alt`. |
| `terminal` | empty | Both. Command of Mod+Return, empty uses `$TERMINAL` and then `foot` or `xterm`. |
| `workspace-count` | 9 | Both. From 1 to 9. |
| `border-width` | 2 | Both. Window border, around the frame on Wayland. |
| `border-color-focused`, `border-color-unfocused` | `#6750a4`, `#49454f` | Both. Form `#rrggbb`. |
| `xkb-rules`, `xkb-model`, `xkb-layout`, `xkb-variant`, `xkb-options` | empty | Wayland. Empty uses the `XKB_DEFAULT_*` variables. |
| `repeat-delay`, `repeat-rate` | 200, 25 | Wayland. |
| `cursor-theme`, `cursor-size` | empty, 0 | Wayland. Empty and 0 use `XCURSOR_THEME`, `XCURSOR_SIZE` and then `default`, 24. |
| `tap-to-click`, `natural-scroll` | true, false | Wayland on a TTY. |

```
cfgd set /org/nuros/sanar/slate/xkb-layout 'us,ru'
cfgd set /org/nuros/sanar/slate/xkb-options 'grp:alt_shift_toggle'
```

### Key bindings

The key `bindings` is a list of `COMBO=ACTION` entries that add to the built in bindings or replace the ones with the same combination. `COMBO=none` removes a binding. A combination is modifiers and a key joined by `+`. Modifiers are `Mod` (the modifier of `mod-key`), `Super`, `Alt`, `Ctrl` and `Shift`. The key is any xkb keysym name such as `Return`, `Tab`, `F5`, `Print`, `XF86AudioRaiseVolume` or a letter. Letters match the latin key of the layout, so bindings keep working on a non latin layout.

| Action | Meaning |
| --- | --- |
| `terminal`, `close`, `maximize`, `fullscreen`, `quit` | As in the table above. |
| `focus-next`, `focus-prev` | Cycle the windows of the workspace. |
| `workspace N`, `move-to-workspace N` | N from 1. |
| `exec COMMAND [ARGUMENT...]` | Start a program, arguments are split at spaces. |
| `none` | Only in `COMBO=none`, removes the binding. |

```
cfgd set /org/nuros/sanar/slate/bindings 'Mod+b=exec firefox;Mod+Return=exec foot -e fish;Mod+q=none;Ctrl+Alt+t=terminal'
```

Entries that cannot be parsed are logged and ignored.

### Window decorations

The Wayland backend draws titlebars and borders itself for windows that ask for server side decorations through xdg-decoration and for X11 windows of XWayland. Applications that draw their own titlebar are left alone. All keys below apply to the Wayland backend and take effect at once. Colors are `#rrggbb` or `#rrggbbaa`.

| Key | Default | Meaning |
| --- | --- | --- |
| `decoration-mode` | `server` | `server` asks every application for server side decorations, `client` for client side ones, `auto` follows the application. |
| `decoration-rules` | empty | List of `APP_ID=server`, `APP_ID=client` or `APP_ID=none`. `none` removes the frame completely. |
| `titlebar-height` | 30 | Height in pixels. |
| `titlebar-color-focused`, `titlebar-color-unfocused` | `#6750a4`, `#49454f` | Titlebar colors. |
| `title-color-focused`, `title-color-unfocused` | `#ffffff`, `#cac4d0` | Title text colors. |
| `title-font` | `sans-serif` | Comma separated fontconfig patterns or font file paths. Later fonts supply glyphs that earlier ones lack. |
| `title-font-size` | 13 | Pixels. |
| `title-align` | `center` | `left`, `center` or `right`. |
| `title-padding` | 12 | Space between the title and the buttons or the edge. |
| `corner-radius` | 8 | Radius of the upper corners. |
| `resize-margin` | 4 | Invisible resize area outside the frame. |
| `button-layout` | `:maximize,close` | `LEFT:RIGHT` lists of `close` and `maximize`. |
| `button-size`, `button-spacing`, `button-margin` | 18, 6, 8 | Pixels. |
| `button-shape` | `circle` | `circle`, `rounded` or `square`. |
| `button-color`, `button-hover-color`, `button-close-hover-color` | `#00000000`, `#ffffff33`, `#e5484d` | Button backgrounds. |
| `button-icon-color-focused`, `button-icon-color-unfocused` | `#ffffff`, `#cac4d0` | Icon colors. |
| `titlebar-double-click`, `titlebar-middle-click`, `titlebar-right-click` | `maximize`, `none`, `none` | `none`, `maximize`, `fullscreen` or `close`. |

Dragging the titlebar moves a window and dragging the frame or the margin resizes it.

```
cfgd set /org/nuros/sanar/slate/decoration-rules 'foot=none;firefox=client'
cfgd set /org/nuros/sanar/slate/button-layout 'close:maximize'
```

## Roadmap

- Tiling layout as a setting, floating stays the default.
- Decorations in the X11 backend, window shadows and a minimize button.
- Text input and ext-image-copy-capture.

## Acknowledgments

- [Smithay](https://github.com/Smithay/smithay), the Wayland compositor library.
- [x11rb](https://github.com/psychon/x11rb), the X11 protocol bindings.

## Links

- [CONTRIBUTING](CONTRIBUTING.md)

## License

GPL-3.0-only. See [LICENSE](LICENSE).
