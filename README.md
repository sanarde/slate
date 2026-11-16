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
| Mod+Tab, Alt+Tab, add Shift for backwards | Window switcher with previews on Wayland, hold the modifier and press Tab again to move, release to switch. On X11 it focuses the next or the previous window. |
| Mod+o | Overview of the windows on Wayland, click a window or press Return to switch to it, Esc closes the overview |
| Mod+t | Switch the workspace between floating and its tiling layout |
| Mod+space | Next layout of the workspace |
| Mod+Shift+space | Toggle floating for the focused window |
| Mod+l, Mod+h | Grow or shrink the master area |
| Mod+comma, Mod+period | More or fewer windows in the master area |
| Mod+Shift+Return | Make the focused window the master |
| Mod+Shift+j, Mod+Shift+k | Swap the focused window with the next or the previous one |
| Mod+1 to Mod+9 | Switch to a workspace |
| Mod+Shift+1 to Mod+Shift+9 | Move the focused window to a workspace |
| Mod+Shift+e | Quit |
| Mod+left button, drag | Move the window |
| Mod+right button, drag | Resize the window |
| Ctrl+Alt+F1 to F12 | Switch virtual terminal (`udev` only) |

Focus follows clicks. There are nine workspaces on every output, each output shows its own.

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

The default theme `material-you` follows Material Design 3: tonal surface containers for the titlebar, circular icon buttons with state layers (8 percent on hover, 12 percent when pressed), elevation 3 for the focused window and 1 for the others, and the emphasized motion curves with a short accelerating close. It uses the color scheme of Seurat, so titlebars and borders follow the wallpaper and the light or dark variant of the system, in the same way as applications that use SanarStyle. Settings made through cfgd, for example by a settings application, apply and the file only has to hold what the user wants to pin.

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
- `bind`, `rule` and `float` add a line to `bindings`, `decoration-rules` and `floating-rules`.
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

### Tiling

Every workspace has a layout. `floating` (the default) leaves windows where they are, the others tile the windows of the workspace on each monitor: `master-stack` (a master area on the left, the others stacked on the right), `spiral`, `grid` and `monocle` (every window fills the area, the focused one is on top). `layout` sets the initial layout of all workspaces, so `layout = master-stack` makes Slate a tiling window manager. `Mod+t` and `Mod+space` change the layout of the current workspace while Slate runs.

Dialogs, windows with a fixed size, windows with a parent and the applications in `float` rules stay floating. Moving a tiled window with `Mod` and the mouse makes it floating. Resizing a tiled window by hand is not supported, use `master-grow` and `master-shrink`.

| Key | Default | Meaning |
| --- | --- | --- |
| `layout` | `floating` | `floating`, `master-stack`, `spiral`, `grid` or `monocle`. |
| `tiling-gap` | 8 | Space between windows. |
| `tiling-outer-gap` | 8 | Space between windows and the screen edge. |
| `master-ratio` | 55 | Percent of the screen for the master area, or for the first split of the spiral. |
| `master-count` | 1 | Windows in the master area. |
| `floating-rules` | empty | Application ids that always float. In the file `float = APP_ID`. |

### Shadows and animations

The Wayland backend draws a soft shadow under windows that have a frame of Slate and animates windows. Both are on by default and can be turned off with `shadow-size = 0` and `animations = false`. The X11 backend has neither, because it has no compositor.

Animations: a window fades and grows when it opens or is restored, windows move and resize smoothly when they are tiled, untiled, maximized or restored, and windows slide in when the workspace changes. A window that is closed from Slate (titlebar button, key binding, panel, `slatectl close`) fades and shrinks. A window that its application closes by itself disappears at once, because its last picture is already gone.

| Key | Default | Meaning |
| --- | --- | --- |
| `shadow-size` | 18 | Blur radius in pixels, 0 turns shadows off. |
| `shadow-offset-x`, `shadow-offset-y` | 0, 6 | Shift of the shadow. |
| `shadow-color-focused`, `shadow-color-unfocused` | `#00000073`, `#00000040` | Shadow colors with alpha. |
| `elevation-focused`, `elevation-unfocused` | -1, -1 | Material Design 3 elevation level 0 to 5 for the shadow of the focused window and of the others. Levels 1 to 5 give a blur of 6, 10, 14, 18 and 22 pixels, an offset of 1, 3, 5, 7 and 9 pixels and a black of 18 to 34 percent. -1 uses the `shadow-*` keys above. |
| `shadow-tiled` | false | Shadows under tiled windows. |
| `animations` | true | Master switch. |
| `animation-duration` | 200 | Milliseconds, 0 turns animations off. |
| `animation-curve` | `ease-out` | `linear`, `ease-out`, `ease-in-out`, `overshoot` or a Material Design 3 curve: `standard`, `standard-decelerate`, `standard-accelerate`, `emphasized`, `emphasized-decelerate`, `emphasized-accelerate`. |
| `animation-close-curve`, `animation-close-duration` | `ease-out`, 200 | Easing and milliseconds of the close animation, 0 turns it off. |
| `animate-open`, `animate-move`, `animate-workspace` | true | Switches for opening and restoring, moving and resizing, and workspace changes. |
| `animate-close` | true | Close animation, see above. |
| `animation-open-scale` | 94 | Percent of its size that an opening window starts from and a closing window shrinks to. |
| `round-content` | true | Clip the content of windows with a frame of Slate to the rounded corners of the frame. |
| `dim-inactive` | 0 | Percent by which windows that are not focused are darkened, up to 80. |
| `blur-radius`, `blur-passes` | 3, 2 | Blur behind windows that have the `blur` rule. 0 for the radius turns it off. |

### Key bindings

The key `bindings` is a list of `COMBO=ACTION` entries that add to the built in bindings or replace the ones with the same combination. `COMBO=none` removes a binding. A combination is modifiers and a key joined by `+`. Modifiers are `Mod` (the modifier of `mod-key`), `Super`, `Alt`, `Ctrl` and `Shift`. The key is any xkb keysym name such as `Return`, `Tab`, `F5`, `Print`, `XF86AudioRaiseVolume` or a letter. Letters match the latin key of the layout, so bindings keep working on a non latin layout.

| Action | Meaning |
| --- | --- |
| `terminal`, `close`, `maximize`, `minimize`, `restore`, `fullscreen`, `quit` | As in the table above. `restore` brings back the window that was minimized last. |
| `toggle-tiling`, `layout-next`, `toggle-floating` | Layout of the workspace and floating of the window. |
| `master-grow`, `master-shrink`, `master-more`, `master-fewer`, `promote`, `swap-next`, `swap-prev` | Tiling adjustments. |
| `focus-next`, `focus-prev` | Cycle the windows of the workspace. |
| `switcher`, `switcher-prev`, `switcher-commit`, `switcher-cancel` | The window switcher. It ends when the modifier that started it is released, `switcher-commit` and `switcher-cancel` end it from a script. On X11 the first two do what `focus-next` and `focus-prev` do. |
| `overview` | Show or hide the overview, Wayland only. |
| `workspace N`, `move-to-workspace N` | N from 1. |
| `workspace-next`, `workspace-prev` | Step through the workspaces, without wrapping around. |
| `focus-monitor DIR`, `move-to-monitor DIR` | Move the pointer and the focus to another monitor or carry the focused window there. `DIR` is `next`, `prev` (in the order of the screen, from the left), `left`, `right`, `up` or `down`. A moved window keeps its offset inside the work area. |
| `scratchpad NAME` | Show, hide or start the scratchpad NAME, see [Window rules and session](#window-rules-and-session). |
| `reload` | Read the configuration again now. |
| `restart` | X11 only: restart Slate and keep the windows. Wayland clients cannot survive a compositor restart. |
| `exec COMMAND [ARGUMENT...]` | Start a program, arguments are split like in a shell. |
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

Both backends draw titlebars and borders themselves. On Wayland this is done for windows that ask for server side decorations through xdg-decoration and for X11 windows of XWayland, and applications that draw their own titlebar are left alone. The X11 backend puts every window except docks, popups and windows that ask for no decorations (Motif hints) into a frame window. All keys below take effect at once on both backends. Colors are `#rrggbb` or `#rrggbbaa`.

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
| `button-color`, `button-hover-color`, `button-pressed-color`, `button-close-hover-color` | `#00000000`, `#ffffff33`, `#ffffff4d`, `#e5484d` | Button backgrounds. The pressed color applies while the mouse button is held, the close button keeps its hover color. |
| `button-icon-color-focused`, `button-icon-color-unfocused` | `#ffffff`, `#cac4d0` | Icon colors. |
| `titlebar-double-click`, `titlebar-middle-click`, `titlebar-right-click` | `maximize`, `none`, `none` | `none`, `maximize`, `minimize`, `fullscreen` or `close`. |

Dragging the titlebar moves a window and dragging the frame or the margin resizes it. In the X11 backend the grab area is the border and the top rows of the titlebar, because a frame window cannot extend beyond its borders, and the rounded corners use the Shape extension, so they have no antialiasing. A minimized window is hidden until it is restored with the `restore` binding, by a request of the application or, on X11, by a pager that activates it. Slate has no taskbar of its own.

```
rule = foot=none
rule = firefox=client
button-layout = close:maximize
```

## Window rules and session

### Window rules

`window = CONDITIONS => ACTIONS` changes how a window opens. Conditions are `app_id=GLOB` (`class=` is the same), `title=GLOB` and `title~=TEXT` (contains). `*` and `?` work in globs, a value with spaces goes in quotes, all conditions of a rule must match. Actions are separated by commas. Later rules win over earlier ones.

| Action | Meaning |
| --- | --- |
| `float`, `tile` | Force floating or tiling. |
| `workspace N` | Open on workspace N, from 1. |
| `size WxH` | Size of the window content. |
| `position X,Y`, `position center` | Place the frame relative to the work area. |
| `above` | Keep the window above the others. |
| `sticky` | Show the window on every workspace, it always floats. |
| `fullscreen`, `maximize` | Open in that state. |
| `opacity V` | From 0.05 to 1. Wayland draws it, X11 sets `_NET_WM_WINDOW_OPACITY` for a compositor. |
| `no-shadow`, `no-decoration` | Wayland only for the shadow. `no-decoration` removes the frame on both backends. |
| `blur` | Blur what is behind the window, Wayland only. It is meant for windows with `opacity` below 1 or with a transparent background. The area under such a window is redrawn on every frame, the blur does not follow the rounded corners yet. |
| `scratchpad NAME` | Make the window the scratchpad NAME. |

```
window = app_id=pavucontrol => float, size 900x600, position center
window = title~="Picture in Picture" => float, sticky, above, opacity 0.9
window = app_id=Steam title=Friends* => workspace 4
```

On Wayland the app id and the title are those at the moment the window opens. Rules are applied once, a restart on X11 does not move windows.

### Scratchpads

`scratchpad = NAME=COMMAND` declares a drop down program and a window rule `scratchpad NAME` tells which window belongs to it. The action `scratchpad NAME` starts the program if there is no such window, hides the window when it is focused and brings it to the current workspace otherwise.

```
scratchpad = term=foot --app-id scratch
window = app_id=scratch => scratchpad term, size 1000x500, position center
bind = Mod+grave=scratchpad term
```

### Session

`exec-once = COMMAND` starts a program when Slate starts, repeat the line for more. `xdg-autostart = true` also starts the applications from the XDG autostart directories, `Hidden`, `OnlyShowIn`, `NotShowIn` and `TryExec` are honored and the desktop name is `Slate`. `session-environment` (on by default) imports `WAYLAND_DISPLAY`, `DISPLAY`, `SLATE_SOCKET`, `XCURSOR_*`, `XDG_CURRENT_DESKTOP` and `XDG_SESSION_TYPE` into the D-Bus activation and systemd user environments with `dbus-update-activation-environment --systemd`, so that services find the session. It is skipped in the nested winit backend. The `restart` action does not run `exec-once` and autostart again.

## Monitors, gestures and touch

Every monitor has its own current workspace and its own layout for each workspace, like tags in awesome. `workspace N` switches the workspace of the monitor with the pointer (on X11 the monitor of the last key press or the focused window), the windows of the other monitors stay where they are. A window belongs to the monitor it overlaps most. A window that is moved to another monitor, with `move-to-monitor` or by dragging it, takes the workspace that this monitor shows. Tiling lays out the windows of every monitor separately. `slatectl outputs` and the `workspace` event tell which monitor shows which workspace. Bind `focus-monitor` and `move-to-monitor` to reach the other monitors from the keyboard:

```
bind = Mod+Alt+Right=focus-monitor right
bind = Mod+Alt+Left=focus-monitor left
bind = Mod+Alt+Shift+Right=move-to-monitor right
bind = Mod+Alt+Shift+Left=move-to-monitor left
```

On Wayland with the udev backend a touchpad swipe with `gesture-fingers` fingers (3 by default) runs an action once it has travelled `gesture-threshold` units. `gesture-swipe-left`, `gesture-swipe-right`, `gesture-swipe-up` and `gesture-swipe-down` take any action of the key bindings, by default a swipe to the left goes to the next workspace and a swipe to the right to the previous one. Swipes with another finger count, pinch and hold go to the application through the pointer gestures protocol. Touchscreens work as a touch seat: a touch focuses the window under it and the events go to the application. The titlebar buttons do not react to touch yet. X11 has neither gestures nor touch handling of its own.

## Panels, scripts and IPC

Slate answers over a Unix socket, `$XDG_RUNTIME_DIR/slate.sock`, or `slate-PID.sock` when that name is taken by a running instance. Programs that Slate starts get the path in `SLATE_SOCKET`. A request is one line and an answer is one line of JSON, `{"ok":true,"data":...}` or `{"ok":false,"error":"..."}`. `slatectl` is the client.

```
slatectl state                      # workspaces, windows and outputs
slatectl windows                    # id, title, app_id, workspace, focused, floating, tiled, maximized, fullscreen, minimized
slatectl focus ID                   # also restores a minimized window and changes the workspace
slatectl action workspace 3         # any action of the key bindings, for example action exec foot
slatectl action focus-monitor right  # monitors, the switcher, scratchpads and the rest work the same way
slatectl layout 2 spiral
slatectl move-to-workspace ID 4
slatectl subscribe                  # JSON lines until interrupted
```

`state`, `windows`, `workspaces`, `outputs`, `version`, `focus ID`, `close ID`, `minimize ID`, `restore ID`, `move-to-workspace ID N`, `layout N NAME` and `action ACTION` are the commands. Workspaces are numbered from 1. `subscribe` prints the events `window-open`, `window-close`, `window-title`, `focus`, `workspace` and `layout`. Window ids are the X11 window ids in the X11 backend.

```
slatectl subscribe | while read -r line; do case "$line" in *'"event":"workspace"'*) pkill -SIGRTMIN+8 waybar ;; esac; done
```

The Wayland backend also implements `wlr-foreign-toplevel-management`, so taskbars (the `wlr/taskbar` module of Waybar, `sfwbar` and similar), window switchers and `wlrctl` list windows, activate, minimize, maximize and close them, and bring minimized windows back.

## Roadmap

- Close animations and shadows for windows that draw their own decorations.
- Text input and ext-image-copy-capture.

## Acknowledgments

- [Smithay](https://github.com/Smithay/smithay), the Wayland compositor library.
- [x11rb](https://github.com/psychon/x11rb), the X11 protocol bindings.

## Links

- [CONTRIBUTING](CONTRIBUTING.md)

## License

GPL-3.0-only. See [LICENSE](LICENSE).
