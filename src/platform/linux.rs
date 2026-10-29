//! Linux backend, built on GTK 4 through the `gtk4` bindings.
//!
//! The window is a `GtkScrolledWindow` holding a vertical `GtkBox` of rows.
//! Each row carries its own `GtkDragSource` offering its files as a
//! `GdkFileList`, which GTK serializes as `text/uri-list` (plus the file
//! transfer portal formats understood by sandboxed applications).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result, anyhow};
use gtk::prelude::*;
use gtk::{gdk, gio, glib, graphene, pango};

use crate::config::Options;
use crate::items;

/// Application ID reported to the desktop, e.g. as the Wayland `app_id`.
const APP_ID: &str = "io.github.kmazurek.dnd";

/// Padding between the window edge and the rows.
const MARGIN: i32 = 8;

/// When set, the window content is rendered to this PNG file once it has been
/// drawn, and the program exits. Used by the GUI smoke tests; not a public
/// interface.
const SNAPSHOT_ENV: &str = "DND_SNAPSHOT";

/// Row styling. The colors are derived from the theme, so they follow dark
/// mode and high contrast settings.
const CSS: &str = "
.dnd-row {
    padding: 4px 6px;
    border-radius: 6px;
    background-color: alpha(currentColor, 0.08);
}
.dnd-row:hover {
    background-color: alpha(currentColor, 0.12);
}
.dnd-row.pressed {
    background-color: alpha(@theme_selected_bg_color, 0.5);
}
";

pub fn run(options: Options) -> Result<()> {
    gtk::init().context("cannot open a display (is DISPLAY or WAYLAND_DISPLAY set?)")?;
    let options = Rc::new(options);
    let snapshot_target = std::env::var_os(SNAPSHOT_ENV).map(PathBuf::from);
    let outcome = Rc::new(RefCell::new(Ok(())));

    // Every invocation gets its own window, instead of handing its files over
    // to an instance that is already running.
    let app = gtk::Application::new(Some(APP_ID), gio::ApplicationFlags::NON_UNIQUE);
    app.connect_startup(|_| load_css());
    let result = outcome.clone();
    app.connect_activate(move |app| {
        let (window, content) = build_window(app, &options);
        match snapshot_target.clone() {
            Some(target) => {
                // Without the window behind it the content has no background.
                content.add_css_class("background");
                let result = result.clone();
                let app = app.clone();
                after_first_frame(&window, move || {
                    *result.borrow_mut() = snapshot(&content, &target);
                    app.quit();
                });
            }
            // The size request sets the initial size; dropping it afterwards
            // lets the user shrink the window as well as grow it.
            None if options.resizable => {
                after_first_frame(&window, move || content.set_size_request(-1, -1));
            }
            None => {}
        }
        window.present();
    });

    // Our arguments are not meant for GTK.
    app.run_with_args::<&str>(&[]);
    outcome.replace(Ok(()))
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(CSS);
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// Calls `f` once, after the window has been laid out and drawn for the
/// first time.
fn after_first_frame(window: &gtk::ApplicationWindow, f: impl FnOnce() + 'static) {
    let f = Cell::new(Some(f));
    let ticks = Cell::new(0);
    window.add_tick_callback(move |_, _| {
        // The first tick comes before the first frame is laid out and painted.
        ticks.set(ticks.get() + 1);
        if ticks.get() < 2 {
            return glib::ControlFlow::Continue;
        }
        if let Some(f) = f.take() {
            f();
        }
        glib::ControlFlow::Break
    });
}

/// Renders `content` into a PNG file at `target`.
fn snapshot(content: &gtk::ScrolledWindow, target: &Path) -> Result<()> {
    let (width, height) = (content.width(), content.height());
    let snapshot = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(content)).snapshot(&snapshot, width.into(), height.into());
    let node = snapshot.to_node().context("the window rendered nothing")?;
    let renderer = content
        .native()
        .and_then(|native| native.renderer())
        .context("the window has no renderer")?;
    let viewport = graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
    renderer
        .render_texture(node, Some(&viewport))
        .save_to_png(target)
        .map_err(|_| anyhow!("cannot write snapshot to {}", target.display()))
}

/// Builds the window and returns it together with its content, whose size
/// request holds the requested content size.
fn build_window(
    app: &gtk::Application,
    options: &Rc<Options>,
) -> (gtk::ApplicationWindow, gtk::ScrolledWindow) {
    let list = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_top(MARGIN)
        .margin_bottom(MARGIN)
        .margin_start(MARGIN)
        .margin_end(MARGIN)
        .build();
    add_rows(&list, options);

    // Sizing the content rather than the window keeps the title bar out of
    // the requested size, as on macOS.
    let content = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .width_request(options.width as i32)
        .height_request(options.height as i32)
        .child(&list)
        .build();

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("dnd")
        .resizable(options.resizable)
        .child(&content)
        .build();
    window.add_controller(shortcuts());
    (window, content)
}

/// Escape, Ctrl+W and Ctrl+Q close the window, which ends the program.
fn shortcuts() -> gtk::ShortcutController {
    let controller = gtk::ShortcutController::new();
    for (key, modifiers) in [
        (gdk::Key::Escape, gdk::ModifierType::empty()),
        (gdk::Key::w, gdk::ModifierType::CONTROL_MASK),
        (gdk::Key::q, gdk::ModifierType::CONTROL_MASK),
    ] {
        controller.add_shortcut(gtk::Shortcut::new(
            Some(gtk::KeyvalTrigger::new(key, modifiers)),
            Some(gtk::NamedAction::new("window.close")),
        ));
    }
    controller
}

/// Fills `list` with a "drag all" row and/or one row per file, as decided by
/// [`crate::config::DragAll::rows`].
fn add_rows(list: &gtk::Box, options: &Rc<Options>) {
    let items = &options.items;
    let rows = options.drag_all.rows(items.len());
    if rows.all {
        let paths = items.iter().map(|i| i.path.clone()).collect();
        list.append(&row(paths, &items::all_label(items.len()), options));
    }
    if rows.individual {
        for item in items {
            list.append(&row(vec![item.path.clone()], &item.label, options));
        }
    }
}

/// A row showing an icon and, unless `--icons-only`, a label. Dragging it
/// drags all of `paths`.
fn row(paths: Vec<PathBuf>, label: &str, options: &Rc<Options>) -> gtk::Box {
    let icon = Icon::for_paths(&paths, options.thumbnails);
    let tooltip = paths
        .iter()
        .map(|p| p.to_string_lossy())
        .collect::<Vec<_>>()
        .join("\n");

    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .css_classes(["dnd-row"])
        .tooltip_text(tooltip)
        .build();
    row.append(&icon.image(options.icon_size as i32));
    if !options.icons_only {
        // Let the label shrink (and ellipsize) rather than widen the window.
        let text = gtk::Label::builder()
            .label(label)
            .ellipsize(pango::EllipsizeMode::Middle)
            .xalign(0.0)
            .hexpand(true)
            .build();
        row.append(&text);
    }

    row.add_controller(press_highlight());
    row.add_controller(drag_source(paths, icon, options.clone()));
    row
}

/// Highlights the row from button press to release. A drag that starts in
/// between takes over the highlight, see [`drag_source`].
fn press_highlight() -> gtk::GestureClick {
    let gesture = gtk::GestureClick::new();
    let set = |gesture: &gtk::GestureClick, pressed: bool| {
        if let Some(widget) = gesture.widget() {
            set_pressed(&widget, pressed);
        }
    };
    gesture.connect_pressed(move |gesture, _, _, _| set(gesture, true));
    gesture.connect_released(move |gesture, _, _, _| set(gesture, false));
    gesture.connect_stopped(move |gesture| set(gesture, false));
    gesture
}

fn set_pressed(widget: &gtk::Widget, pressed: bool) {
    if pressed {
        widget.add_css_class("pressed");
    } else {
        widget.remove_css_class("pressed");
    }
}

fn drag_source(paths: Vec<PathBuf>, icon: Icon, options: Rc<Options>) -> gtk::DragSource {
    let files: Vec<gio::File> = paths.iter().map(gio::File::for_path).collect();
    let content = gdk::ContentProvider::for_value(&gdk::FileList::from_array(&files).to_value());

    let source = gtk::DragSource::new();
    source.set_content(Some(&content));
    // Deliberately no `MOVE`: file managers would otherwise move the original
    // file away when dropping onto the same file system.
    source.set_actions(gdk::DragAction::COPY | gdk::DragAction::LINK);

    // GTK emits `drag-cancel` before `drag-end` when the drop failed or was
    // refused; `drag-end` alone means it was accepted.
    let cancelled = Rc::new(Cell::new(false));

    let verbose = options.verbose;
    let size = options.icon_size as i32;
    let begun = cancelled.clone();
    source.connect_drag_begin(move |source, drag| {
        begun.set(false);
        if let Some(widget) = source.widget() {
            set_pressed(&widget, true);
        }
        gtk::DragIcon::for_drag(drag).set_child(Some(&icon.image(size)));
        drag.set_hotspot(size / 2, size / 2);
        if verbose {
            eprintln!("dnd: dragging {paths:?}");
        }
    });

    let failed = cancelled.clone();
    source.connect_drag_cancel(move |_, _, reason| {
        failed.set(true);
        if verbose {
            eprintln!("dnd: drag cancelled ({reason:?})");
        }
        false
    });

    source.connect_drag_end(move |source, drag, _delete| {
        let Some(widget) = source.widget() else {
            return;
        };
        set_pressed(&widget, false);
        let accepted = !cancelled.get();
        if options.verbose {
            let action = drag.selected_action();
            eprintln!("dnd: drag ended (action {action:?}, accepted: {accepted})");
        }
        if accepted
            && options.and_exit
            && let Some(app) = widget
                .root()
                .and_downcast::<gtk::Window>()
                .and_then(|window| window.application())
        {
            app.quit();
        }
    });
    source
}

/// What a row shows next to its label, and what follows the pointer during a
/// drag.
#[derive(Clone)]
enum Icon {
    /// An image thumbnail.
    Texture(gdk::Texture),
    /// A file type icon from the icon theme.
    Themed(gio::Icon),
}

impl Icon {
    fn for_paths(paths: &[PathBuf], thumbnails: bool) -> Self {
        match paths {
            [single] => Self::for_path(single, thumbnails),
            _ => Self::Themed(common_icon(paths)),
        }
    }

    /// Loads a thumbnail for images (when enabled) and the file type icon
    /// otherwise.
    fn for_path(path: &Path, thumbnails: bool) -> Self {
        if thumbnails
            && items::is_image(path)
            && let Ok(texture) = gdk::Texture::from_filename(path)
        {
            return Self::Texture(texture);
        }
        Self::Themed(file_icon(path))
    }

    /// A new image widget showing the icon at `size` logical pixels.
    fn image(&self, size: i32) -> gtk::Image {
        let image = match self {
            Self::Texture(texture) => gtk::Image::from_paintable(Some(texture)),
            Self::Themed(icon) => gtk::Image::from_gicon(icon),
        };
        image.set_pixel_size(size);
        image
    }
}

/// The file type icon shown for `path` by file managers.
fn file_icon(path: &Path) -> gio::Icon {
    gio::File::for_path(path)
        .query_info(
            gio::FILE_ATTRIBUTE_STANDARD_ICON,
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .ok()
        .and_then(|info| info.icon())
        .unwrap_or_else(generic_icon)
}

/// The icon shared by all of `paths`, or a generic one when they differ.
fn common_icon(paths: &[PathBuf]) -> gio::Icon {
    let mut icons = paths.iter().map(|p| file_icon(p));
    let Some(first) = icons.next() else {
        return generic_icon();
    };
    if icons.all(|icon| icon.equal(Some(&first))) {
        first
    } else {
        generic_icon()
    }
}

fn generic_icon() -> gio::Icon {
    gio::ThemedIcon::new("text-x-generic").upcast()
}
