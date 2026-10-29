# dnd - drag n drop

A vibe-coded CLI utility for drag and drop.

Drag files from the terminal into other applications. `dnd` opens a small
native window listing files that can be dragged into any application.

It is modelled after [ripdrag](https://github.com/nik012003/ripdrag) and
[dragon](https://github.com/mwh/dragon) and accepts the same flags for
dragging files out, but uses each platform's native toolkit: AppKit on macOS
and GTK 4 on Linux.
Acting as a drop target (`--target`) is not supported.

| Platform | Status                                              |
| -------- | --------------------------------------------------- |
| macOS    | Supported (AppKit via [objc2])                      |
| Linux    | Supported (GTK 4.8+ via [gtk4-rs]), Wayland and X11 |

[objc2]: https://github.com/madsmtm/objc2
[gtk4-rs]: https://gtk-rs.org/gtk4-rs

## Usage

```sh
dnd FILE...                 # drag files out, one by one or all at once
dnd -x *.png                # ...and quit after the first successful drag
dnd -A -x ~/Downloads/*.pdf # a single row that drags every file
fd -e pdf | dnd -I          # read paths from stdin
```

With more than one file, the top row ("All N files") drags every file in a
single drag; the rows below drag them individually.

Close the window with Escape, ⌘W or ⌘Q (Ctrl+W or Ctrl+Q on Linux).

Drags offer *copy* and *link* but never *move*, so dropping onto Finder (or a
Linux file manager) on the same volume copies the file instead of moving it
away. On Linux files are offered as `text/uri-list`, plus the document portal
formats that let sandboxed (Flatpak) applications receive them.

| Flag | Meaning |
| ---- | ------- |
| `-x`, `--and-exit` | Exit after the first successful drag |
| `-a`, `--all` | Show the "drag all" row even for a single file |
| `-A`, `--all-compact` | Show only the "drag all" row |
| `--no-all` | Never show the "drag all" row |
| `-I`, `--from-stdin` | Read additional paths from stdin, one per line |
| `-b`, `--basename` | Show file names instead of full paths |
| `-i`, `--icons-only` | Hide the labels |
| `-d`, `--disable-thumbnails` | Use file type icons instead of image previews |
| `-s`, `--icon-size N` | Icon size in points (16–512, default 32) |
| `-W`, `--content-width N` | Window content width (default 360) |
| `-H`, `--content-height N` | Window content height (default 360) |
| `-r`, `--resizable` | Make the window resizable |
| `-v`, `--verbose` | Print diagnostics to stderr |

## Configuration

Defaults for every option can be set in `$XDG_CONFIG_HOME/dnd/config.toml`,
falling back to `~/.config/dnd/config.toml` when `XDG_CONFIG_HOME` is unset.
Without the file the built-in defaults apply. Flags given on the command line
take precedence over the file.

Keys are named after the long flags; the `-a`/`-A`/`--no-all` flags map to a
single `drag-all` key. All keys are optional; unknown keys and out-of-range
values are reported as errors.

```toml
verbose = false
and-exit = true
icons-only = false
disable-thumbnails = false
icon-size = 48
basename = true
resizable = false
content-width = 360
content-height = 360
drag-all = "auto"   # "auto", "always" (-a), "compact" (-A) or "never" (--no-all)
from-stdin = false
```

A boolean enabled in the file cannot be switched off from the command line.

## Building

Requires [rustup](https://rustup.rs) and, on macOS, the Xcode command line
tools. `rust-toolchain.toml` pins the stable toolchain and components.

On Linux, building needs the GTK 4 development files (4.8 or newer) and
`pkg-config`:

```sh
sudo apt install libgtk-4-dev pkg-config     # Debian, Ubuntu
sudo dnf install gtk4-devel pkgconf          # Fedora
sudo zypper install gtk4-devel pkgconf       # openSUSE
sudo pacman -S gtk4 pkgconf                  # Arch
```

```sh
make setup     # install toolchain, rustfmt, clippy, rust-analyzer, Linux target
make build     # debug build
make install   # release build into ~/.local/bin (override with PREFIX=...)
make help      # list all targets
```

## Development

```sh
make ci             # fmt-check, clippy, rust-analyzer, tests, Linux type-check
                    # (the type-check works without GTK installed)
make test           # all tests
make test-headless  # skip GUI smoke tests (e.g. over SSH)
make run ARGS="-a Cargo.toml src"
```

Layout:

- `src/cli.rs` – clap argument definitions (ripdrag compatible)
- `src/config.rs` – validation into the `Options` handed to backends
- `src/config_file.rs` – locating and parsing `config.toml`
- `src/items.rs` – path resolution, labels, stdin parsing
- `src/platform/macos.rs` – AppKit window and drag source
- `src/platform/linux.rs` – GTK 4 window and drag source
- `src/platform/unsupported.rs` – stub for platforms without a backend

Tests:

- Unit tests cover argument parsing, config file parsing and precedence,
  validation and path handling.
- `tests/cli.rs` runs the binary for help, version, error and config file
  cases. Integration tests point `XDG_CONFIG_HOME` elsewhere so a personal
  config file cannot affect them.
- `tests/gui_macos.rs` builds the real window for each mode and renders it
  to a PNG via the internal `DND_SNAPSHOT=<file.png>` variable, catching
  crashes and layout regressions without user input. These need a logged-in
  GUI session; set `DND_SKIP_GUI_TESTS=1` to skip them.
- `tests/gui_linux.rs` does the same for the GTK backend, rendering only the
  window content. The window flashes up briefly for each test. They are
  skipped when neither `WAYLAND_DISPLAY` nor `DISPLAY` is set, or with
  `DND_SKIP_GUI_TESTS=1`. A tiling window manager that resizes the window
  makes the size assertions fail.

The mouse-driven drag itself is not automated: synthesizing input requires
Accessibility permission on macOS and is not possible for regular clients on
Wayland. Verify it manually after UI changes, e.g. by dragging from
`dnd -x Cargo.toml` onto Finder, a Linux file manager or a text editor.
