# Sanar Slate

> [!NOTE]
> **Repository Mirrors**
>
> - **git.nuros.org** ([sanarde/slate](https://git.nuros.org/sanarde/slate)): primary, self-hosted Forgejo instance, accounts restricted to the core team.
> - **GitHub** ([sanarde/slate](https://github.com/sanarde/slate)): mirror for external contributors. Issues and Pull Requests opened here are welcome and are reviewed and processed by the core team.

## About

Floating window manager that runs as a Wayland compositor built on Smithay and as a separate X11 window manager. It is the window manager of SanarDE and works without it.

## Dependencies

- Rust 1.88 or newer (edition 2024) and Cargo
- libseat, libinput, libudev, libgbm, libxkbcommon
- EGL and OpenGL ES (Mesa)
- fontconfig (`fc-match`), to find the titlebar font

Optional at run time:

- Xwayland, to run X11 applications in the Wayland session
- libcfgd and the cfgd daemon `cfgdd` of [cfgd](https://git.nuros.org/sanarde/cfgd), the settings service of SanarDE. Slate loads libcfgd when it is installed and works from its configuration file when it is not.
- `seuratd` of [seurat](https://git.nuros.org/sanarde/seurat), the source of the Material You color scheme. Without it a built in palette is used.

## Build

```
cargo build --release
```

The binary is `target/release/slate`. The files in `data` are installed by hand: `slate-wayland.desktop` to `share/wayland-sessions`, `slate-x11.desktop` to `share/xsessions`, `org.nuros.sanar.slate.schema` to `share/cfgd/schemas` (only for SanarDE), `themes/*.conf` to `share/slate/themes` (optional, the themes are built in) and `slate.conf.example` as a starting point for `~/.config/slate/slate.conf`.

## Usage

```
slate [OPTION...] [COMMAND [ARGUMENT...]]
```

`COMMAND` is started once the session is up. Options:

| Option | Meaning |
| --- | --- |
| `--backend auto\|winit\|udev\|x11` | Display backend. |
| `--config PATH` | Configuration file, default `$XDG_CONFIG_HOME/slate/slate.conf`, or `$SLATE_CONFIG`. |
| `--theme NAME` | Use this theme instead of the configured one. |
| `--dump-config` | Print the effective settings and where each value comes from. |
| `--check` | Read the configuration, print the problems and exit with status 1 when there are any. |
| `--list-themes` | Print the available themes. |

Backends:

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
| Mod+n | Minimize the focused window |
| Mod+Shift+n | Restore the window that was minimized last |
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

## Configuration

Slate has the same settings with or without SanarDE. They come from four layers, from the lowest:

1. The built in defaults.
2. The settings of the cfgd schema `org.nuros.sanar.slate`, path `/org/nuros/sanar/slate/`, when libcfgd and `cfgdd` are present. A value equal to the default counts as not set.
3. The theme.
4. The file `~/.config/slate/slate.conf`.

A change in any layer applies without a restart, including the files that are included and the color scheme of Seurat. `slate --dump-config` prints every setting with its layer, `slate --check` finds mistakes.

### Without SanarDE

Copy `data/slate.conf.example` to `~/.config/slate/slate.conf` and edit it. A minimal window manager:

```
theme = minimal
mod-key = alt
terminal = alacritty
decoration-rules = alacritty=none
bind = Mod+d=exec rofi -show drun
bind = Mod+b=exec firefox
```

### With SanarDE

The default theme `material-you` uses the color scheme of Seurat, so titlebars and borders follow the wallpaper and the light or dark variant of the system, in the same way as applications that use SanarStyle. Settings made through cfgd, for example by a settings application, apply and the file only has to hold what the user wants to pin.

### File format

```
# comment
$accent = #6750a4                    # a variable
theme = nord
border-color-focused = $accent       # use a variable
titlebar-color-unfocused = @surface_container_high
button-hover-color = @on_surface/12  # a color role with 12 percent alpha
include = ~/.config/slate/mine.conf  # another file, theme:NAME includes a theme
bind = Mod+b=exec firefox            # repeat for more bindings
rule = foot=none                     # repeat for more decoration rules
```

- A key is a setting name from the tables below. Unknown keys are reported. Section headers such as `[slate]` are ignored.
- `$name` is a variable. A theme defines its palette as variables, so defining the same variable in `slate.conf` recolors the theme.
- `@role` is a role of the Material You scheme: `primary`, `on_primary`, `primary_container`, `surface`, `on_surface`, `surface_container_high`, `outline`, `outline_variant`, `error` and the other roles of Seurat. `@role/NN` sets the alpha to NN percent. Without Seurat the roles come from the built in baseline palette, `color-scheme` selects its light or dark variant.
- `bind` and `rule` add a line to `bindings` and `decoration-rules`. Those keys also accept a list separated by `;`.
- A comment starts with `#` at the start of a line or after a space. A color such as `#6750a4` is not a comment.

### Themes

`slate --list-themes` lists them. Built in: `material-you`, `minimal` (borders only), `nord`, `gruvbox-dark`, `catppuccin-mocha`, `dracula`, `tokyo-night` and `light`. `none` uses no theme. A theme is a file in the format above. Files in `~/.config/slate/themes` and `~/.local/share/slate/themes` replace the built in ones with the same name and add new ones, so a rice is one file that can be shared. A theme can build on another with `include = theme:nord`.

### Settings

Every setting has the same name in the file and in cfgd. The defaults in the tables are the values without a theme, the theme `material-you` changes colors, sizes and the font.

| Key | Default | Applies to |
| --- | --- | --- |
| `theme` | `material-you` | Both. |
| `color-scheme` | `auto` | Both. Palette variant without Seurat: `auto`, `light` or `dark`. |
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
xkb-layout = us,ru
xkb-options = grp:alt_shift_toggle
```

With SanarDE the same through cfgd: `cfgd set /org/nuros/sanar/slate/xkb-layout 'us,ru'`.

### Key bindings

The key `bindings` is a list of `COMBO=ACTION` entries that add to the built in bindings or replace the ones with the same combination. `COMBO=none` removes a binding. A combination is modifiers and a key joined by `+`. Modifiers are `Mod` (the modifier of `mod-key`), `Super`, `Alt`, `Ctrl` and `Shift`. The key is any xkb keysym name such as `Return`, `Tab`, `F5`, `Print`, `XF86AudioRaiseVolume` or a letter. Letters match the latin key of the layout, so bindings keep working on a non latin layout.

| Action | Meaning |
| --- | --- |
| `terminal`, `close`, `maximize`, `minimize`, `restore`, `fullscreen`, `quit` | As in the table above. `restore` brings back the window that was minimized last. |
| `focus-next`, `focus-prev` | Cycle the windows of the workspace. |
| `workspace N`, `move-to-workspace N` | N from 1. |
| `exec COMMAND [ARGUMENT...]` | Start a program, arguments are split at spaces. |
| `none` | Only in `COMBO=none`, removes the binding. |

```
bind = Mod+b=exec firefox
bind = Mod+Return=exec foot -e fish
bind = Mod+q=none
bind = Ctrl+Alt+t=terminal
```

Through cfgd the key `bindings` takes the list: `cfgd set /org/nuros/sanar/slate/bindings 'Mod+b=exec firefox;Mod+q=none'`.

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
| `button-layout` | `:minimize,maximize,close` | `LEFT:RIGHT` lists of `close`, `maximize` and `minimize`. |
| `button-size`, `button-spacing`, `button-margin` | 18, 6, 8 | Pixels. |
| `button-shape` | `circle` | `circle`, `rounded` or `square`. |
| `button-color`, `button-hover-color`, `button-close-hover-color` | `#00000000`, `#ffffff33`, `#e5484d` | Button backgrounds. |
| `button-icon-color-focused`, `button-icon-color-unfocused` | `#ffffff`, `#cac4d0` | Icon colors. |
| `titlebar-double-click`, `titlebar-middle-click`, `titlebar-right-click` | `maximize`, `none`, `none` | `none`, `maximize`, `minimize`, `fullscreen` or `close`. |

Dragging the titlebar moves a window and dragging the frame or the margin resizes it. A minimized window is hidden until it is restored with the `restore` binding, by a request of the application or, on X11, by a pager that activates it. Slate has no taskbar of its own.

```
rule = foot=none
rule = firefox=client
button-layout = close:maximize
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
