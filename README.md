# dnd - drag n drop

A vibe-coded CLI utility for drag and drop.

Drag files from the terminal into other applications. `dnd` opens a small
native window listing files that can be dragged into any application.

It is modelled after [ripdrag](https://github.com/nik012003/ripdrag) and
[dragon](https://github.com/mwh/dragon) and accepts the same flags for
dragging files out, but uses the platform's native toolkit instead of GTK.
Acting as a drop target (`--target`) is not supported.

| Platform | Status                                  |
| -------- | --------------------------------------- |
| macOS    | Supported (AppKit via [objc2])          |
| Linux    | Planned (GTK 4); builds, exits with an error |

[objc2]: https://github.com/madsmtm/objc2

## Usage

```sh
dnd FILE...                 # drag files out, one by one or all at once
dnd -x *.png                # ...and quit after the first successful drag
dnd -A -x ~/Downloads/*.pdf # a single row that drags every file
fd -e pdf | dnd -I          # read paths from stdin
```

With more than one file, the top row ("All N files") drags every file in a
single drag; the rows below drag them individually.

Close the window with Escape, ⌘W or ⌘Q.

Drags offer *copy* and *link* but never *move*, so dropping onto Finder on the
same volume copies the file instead of moving it away.

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

## Building

Requires [rustup](https://rustup.rs) and, on macOS, the Xcode command line
tools. `rust-toolchain.toml` pins the stable toolchain and components.

```sh
make setup     # install toolchain, rustfmt, clippy, rust-analyzer, Linux target
make build     # debug build
make install   # release build into ~/.local/bin (override with PREFIX=...)
make help      # list all targets
```

## Development

```sh
make ci             # fmt-check, clippy, rust-analyzer, tests, Linux type-check
make test           # all tests
make test-headless  # skip GUI smoke tests (e.g. over SSH)
make run ARGS="-a Cargo.toml src"
```

Layout:

- `src/cli.rs` – clap argument definitions (ripdrag compatible)
- `src/config.rs` – validation into the `Options` handed to backends
- `src/items.rs` – path resolution, labels, stdin parsing
- `src/platform/macos.rs` – AppKit window and drag source
- `src/platform/unsupported.rs` – stub for platforms without a backend

Tests:

- Unit tests cover argument parsing, validation and path handling.
- `tests/cli.rs` runs the binary for help, version and error cases.
- `tests/gui_macos.rs` builds the real window for each mode and renders it
  to a PNG via the internal `DND_SNAPSHOT=<file.png>` variable, catching
  crashes and layout regressions without user input. These need a logged-in
  GUI session; set `DND_SKIP_GUI_TESTS=1` to skip them.

The mouse-driven drag itself is not automated: synthesizing input requires
Accessibility permission. Verify it manually after UI changes, e.g. by
dragging from `dnd -x Cargo.toml` onto Finder or a text editor.
