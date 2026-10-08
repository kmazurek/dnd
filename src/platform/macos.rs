//! macOS backend, built directly on AppKit through the `objc2` bindings.
//!
//! The window is an `NSScrollView` holding a vertical `NSStackView` of
//! [`ItemView`] rows. Each row is its own `NSDraggingSource` and starts a
//! drag session with file URLs on the pasteboard. In target mode the window's
//! content view is a [`DropView`] registered for file URLs.

use std::cell::{Cell, OnceCell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{
    AnyThread, ClassType, DefinedClass, MainThreadOnly, Message, define_class, msg_send, sel,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSBezierPath, NSBitmapImageFileType, NSColor, NSDragOperation, NSDraggingContext,
    NSDraggingDestination, NSDraggingInfo, NSDraggingItem, NSDraggingSession, NSDraggingSource,
    NSEvent, NSImage, NSImageScaling, NSImageView, NSLayoutAttribute, NSLayoutConstraint,
    NSLayoutConstraintOrientation, NSLineBreakMode, NSMenu, NSMenuItem, NSPasteboardTypeFileURL,
    NSPasteboardURLReadingFileURLsOnlyKey, NSScrollView, NSStackView, NSTextField, NSView,
    NSWindow, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSDictionary, NSEdgeInsets, NSNotification, NSNumber, NSObject,
    NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL, ns_string,
};

use crate::config::{DragAll, Mode, Options};
use crate::items::{self, Item};
use crate::output;

/// Padding between the window edge and the rows.
const MARGIN: f64 = 8.0;

/// Height reserved above the list for the "Drop files here" hint.
const HINT_HEIGHT: f64 = MARGIN * 2.0 + 16.0;

/// When set, the window is rendered to this PNG file instead of being shown,
/// and the program exits. Used by the GUI smoke tests; not a public interface.
const SNAPSHOT_ENV: &str = "DND_SNAPSHOT";

pub fn run(options: Options) -> Result<()> {
    let mtm = MainThreadMarker::new().context("the GUI must run on the main thread")?;
    let options = Rc::new(options);

    let app = NSApplication::sharedApplication(mtm);
    // An unbundled binary defaults to a background-only app that can neither
    // show a key window nor take part in drag and drop.
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    app.setMainMenu(Some(&main_menu(mtm)));

    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

    let window = build_window(mtm, &options);
    if let Some(target) = std::env::var_os(SNAPSHOT_ENV) {
        return snapshot(&window, Path::new(&target));
    }
    window.center();
    window.makeKeyAndOrderFront(None);

    app.run();
    Ok(())
}

/// Renders the window, including its background, into a PNG file at `target`.
fn snapshot(window: &NSWindow, target: &Path) -> Result<()> {
    let content = window.contentView().context("window has no content view")?;
    content.layoutSubtreeIfNeeded();
    // The frame view also draws the window background, so text stays legible
    // in dark mode.
    // SAFETY: the frame view is owned by the window, which outlives `view`.
    let view = unsafe { content.superview() }.unwrap_or(content);
    window.display();
    let bounds = view.bounds();
    let rep = view
        .bitmapImageRepForCachingDisplayInRect(bounds)
        .context("cannot allocate a bitmap for the snapshot")?;
    view.cacheDisplayInRect_toBitmapImageRep(bounds, &rep);
    // SAFETY: an empty property dictionary is valid for PNG.
    let png = unsafe {
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }
    .context("cannot encode the snapshot as PNG")?;
    if !png.writeToFile_atomically(&ns_path(target), true) {
        anyhow::bail!("cannot write snapshot to {}", target.display());
    }
    Ok(())
}

fn build_window(mtm: MainThreadMarker, options: &Rc<Options>) -> Retained<NSWindow> {
    let mut style =
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
    if options.resizable {
        style |= NSWindowStyleMask::Resizable;
    }
    let frame = NSRect::new(NSPoint::ZERO, NSSize::new(options.width, options.height));

    // SAFETY: the window is not released when closed (see below), so the
    // `Retained` we hold stays valid.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: required when creating windows outside of a window controller.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(ns_string!("dnd"));

    let list = item_list(mtm);
    let scroll = scroll_view(mtm, &list);

    let content: Retained<NSView> = match options.mode {
        Mode::Source => {
            let root = NSView::initWithFrame(NSView::alloc(mtm), frame);
            root.addSubview(&scroll);
            pin(&scroll, &root, 0.0, 0.0);
            root
        }
        Mode::Target { keep } => {
            let root = DropView::new(mtm, frame, options.clone());
            let hint = NSTextField::labelWithString(ns_string!("Drop files here"), mtm);
            hint.setTextColor(Some(&NSColor::secondaryLabelColor()));
            hint.setTranslatesAutoresizingMaskIntoConstraints(false);
            root.addSubview(&hint);
            let center_x = hint
                .centerXAnchor()
                .constraintEqualToAnchor(&root.centerXAnchor());
            if keep {
                root.addSubview(&scroll);
                pin(&scroll, &root, HINT_HEIGHT, 0.0);
                activate(&[
                    center_x,
                    hint.topAnchor()
                        .constraintEqualToAnchor_constant(&root.topAnchor(), MARGIN),
                ]);
                root.ivars().list.set(list.clone()).ok();
            } else {
                activate(&[
                    center_x,
                    hint.centerYAnchor()
                        .constraintEqualToAnchor(&root.centerYAnchor()),
                ]);
            }
            // SAFETY: AppKit initialises this static before main runs.
            let file_url_type = unsafe { NSPasteboardTypeFileURL };
            root.registerForDraggedTypes(&NSArray::from_slice(&[file_url_type]));
            Retained::into_super(root)
        }
    };
    window.setContentView(Some(&content));

    add_items(mtm, &list, &options.items, options, options.drag_all);
    window
}

/// Adds rows for `items` to `list`.
fn add_items(
    mtm: MainThreadMarker,
    list: &NSStackView,
    items: &[Item],
    options: &Rc<Options>,
    drag_all: DragAll,
) {
    if items.is_empty() {
        return;
    }
    if drag_all != DragAll::Off {
        let paths = items.iter().map(|i| i.path.clone()).collect();
        let label = match items.len() {
            1 => "1 file".to_owned(),
            n => format!("All {n} files"),
        };
        add_row(list, ItemView::new(mtm, paths, &label, options.clone()));
    }
    if drag_all != DragAll::Compact {
        for item in items {
            let row = ItemView::new(mtm, vec![item.path.clone()], &item.label, options.clone());
            add_row(list, row);
        }
    }
}

fn add_row(list: &NSStackView, row: Retained<ItemView>) {
    list.addArrangedSubview(&row);
    activate(&[row
        .widthAnchor()
        .constraintEqualToAnchor_constant(&list.widthAnchor(), -2.0 * MARGIN)]);
}

fn item_list(mtm: MainThreadMarker) -> Retained<NSStackView> {
    let list = Retained::into_super(FlippedStackView::new(mtm));
    list.setOrientation(objc2_app_kit::NSUserInterfaceLayoutOrientation::Vertical);
    list.setAlignment(NSLayoutAttribute::CenterX);
    list.setSpacing(4.0);
    list.setEdgeInsets(NSEdgeInsets {
        top: MARGIN,
        left: MARGIN,
        bottom: MARGIN,
        right: MARGIN,
    });
    list.setTranslatesAutoresizingMaskIntoConstraints(false);
    list
}

fn scroll_view(mtm: MainThreadMarker, list: &NSStackView) -> Retained<NSScrollView> {
    let scroll = NSScrollView::new(mtm);
    scroll.setHasVerticalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setDrawsBackground(false);
    scroll.setTranslatesAutoresizingMaskIntoConstraints(false);
    scroll.setDocumentView(Some(list));

    let clip = scroll.contentView();
    activate(&[
        list.leadingAnchor()
            .constraintEqualToAnchor(&clip.leadingAnchor()),
        list.trailingAnchor()
            .constraintEqualToAnchor(&clip.trailingAnchor()),
        list.topAnchor().constraintEqualToAnchor(&clip.topAnchor()),
    ]);
    scroll
}

/// Pins `view` to the edges of `parent`, leaving `top` and `bottom` points.
fn pin(view: &NSView, parent: &NSView, top: f64, bottom: f64) {
    activate(&[
        view.leadingAnchor()
            .constraintEqualToAnchor(&parent.leadingAnchor()),
        view.trailingAnchor()
            .constraintEqualToAnchor(&parent.trailingAnchor()),
        view.topAnchor()
            .constraintEqualToAnchor_constant(&parent.topAnchor(), top),
        view.bottomAnchor()
            .constraintEqualToAnchor_constant(&parent.bottomAnchor(), -bottom),
    ]);
}

fn activate(constraints: &[Retained<NSLayoutConstraint>]) {
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(constraints));
}

fn main_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    let app_item = NSMenuItem::new(mtm);
    let app_menu = NSMenu::new(mtm);
    // SAFETY: the selectors are valid actions for the first responder chain.
    unsafe {
        app_menu.addItemWithTitle_action_keyEquivalent(
            ns_string!("Close Window"),
            Some(sel!(performClose:)),
            ns_string!("w"),
        );
        app_menu.addItemWithTitle_action_keyEquivalent(
            ns_string!("Close Window"),
            Some(sel!(performClose:)),
            ns_string!("\u{1b}"),
        );
        app_menu.addItemWithTitle_action_keyEquivalent(
            ns_string!("Quit dnd"),
            Some(sel!(terminate:)),
            ns_string!("q"),
        );
    }
    // The Escape variant only exists for its key equivalent.
    if let Some(item) = app_menu.itemAtIndex(1) {
        item.setHidden(true);
    }
    app_item.setSubmenu(Some(&app_menu));
    menu.addItem(&app_item);
    menu
}

fn quit(mtm: MainThreadMarker) {
    NSApplication::sharedApplication(mtm).terminate(None);
}

fn ns_path(path: &Path) -> Retained<NSString> {
    NSString::from_str(&path.to_string_lossy())
}

fn file_url(path: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&ns_path(path))
}

/// Loads a thumbnail for images (when enabled) and the Finder icon otherwise.
fn icon_for(path: &Path, thumbnails: bool) -> Retained<NSImage> {
    let path_str = ns_path(path);
    if thumbnails
        && items::is_image(path)
        && let Some(image) = NSImage::initWithContentsOfFile(NSImage::alloc(), &path_str)
    {
        return image;
    }
    NSWorkspace::sharedWorkspace().iconForFile(&path_str)
}

fn icon_for_all(paths: &[PathBuf], thumbnails: bool) -> Retained<NSImage> {
    match paths {
        [single] => icon_for(single, thumbnails),
        _ => {
            let strings: Vec<_> = paths.iter().map(|p| ns_path(p)).collect();
            NSWorkspace::sharedWorkspace()
                .iconForFiles(&NSArray::from_retained_slice(&strings))
                .unwrap_or_else(|| icon_for(&paths[0], false))
        }
    }
}

// ---------------------------------------------------------------------------
// Scroll view document
// ---------------------------------------------------------------------------

define_class!(
    /// A stack view with a top-left origin. As the document view of a scroll
    /// view this makes a short list hug the top instead of the bottom.
    // SAFETY: NSStackView has no subclassing requirements and we do not
    // implement Drop.
    #[unsafe(super(NSStackView, NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "DndFlippedStackView"]
    struct FlippedStackView;

    impl FlippedStackView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

impl FlippedStackView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSView's init has this signature.
        unsafe { msg_send![super(this), init] }
    }
}

// ---------------------------------------------------------------------------
// Application delegate
// ---------------------------------------------------------------------------

define_class!(
    // SAFETY: NSObject has no subclassing requirements and we do not
    // implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DndAppDelegate"]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            // Required to bring an unbundled app to the front.
            #[allow(deprecated)]
            NSApplication::sharedApplication(self.mtm()).activateIgnoringOtherApps(true);
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn should_terminate_after_last_window_closed(&self, _app: &NSApplication) -> bool {
            true
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's init has this signature.
        unsafe { msg_send![super(this), init] }
    }
}

// ---------------------------------------------------------------------------
// Draggable row
// ---------------------------------------------------------------------------

struct ItemIvars {
    paths: Vec<PathBuf>,
    options: Rc<Options>,
    /// Set between mouse down and mouse up; drawn as a highlight.
    pressed: Cell<bool>,
    /// Whether the current mouse down already turned into a drag.
    dragging: Cell<bool>,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements and we do not implement
    // Drop.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "DndItemView"]
    #[ivars = ItemIvars]
    struct ItemView;

    impl ItemView {
        /// Let the first click in an inactive window start a drag right away,
        /// so files can be dragged without focusing the window first.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        /// Swallow clicks on the icon and label so the row handles them.
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> Option<Retained<NSView>> {
            let frame = self.frame();
            let inside = point.x >= frame.origin.x
                && point.y >= frame.origin.y
                && point.x < frame.origin.x + frame.size.width
                && point.y < frame.origin.y + frame.size.height;
            inside.then(|| Retained::into_super(self.retain()))
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            self.ivars().dragging.set(false);
            self.set_pressed(true);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            if !self.ivars().dragging.replace(true) {
                self.begin_drag(event);
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            self.set_pressed(false);
            let ivars = self.ivars();
            // Only single files open on click; opening every file behind a
            // "drag all" row at once would rarely be what the user wanted.
            if !ivars.dragging.get()
                && ivars.options.click_to_open
                && let [path] = ivars.paths.as_slice()
            {
                NSWorkspace::sharedWorkspace().openURL(&file_url(path));
            }
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let bounds = self.bounds();
            let color = if self.ivars().pressed.get() {
                NSColor::selectedContentBackgroundColor()
            } else {
                NSColor::quaternaryLabelColor()
            };
            color.setFill();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds, 6.0, 6.0).fill();
        }
    }

    unsafe impl NSObjectProtocol for ItemView {}

    unsafe impl NSDraggingSource for ItemView {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn source_operation_mask(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            // Deliberately no `Move`: Finder would otherwise move the original
            // file away when dropping onto the same volume.
            NSDragOperation::Copy | NSDragOperation::Link | NSDragOperation::Generic
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn ended(&self, _session: &NSDraggingSession, _point: NSPoint, op: NSDragOperation) {
            self.set_pressed(false);
            let options = &self.ivars().options;
            let accepted = op != NSDragOperation::None;
            if options.verbose {
                eprintln!("dnd: drag ended (operation {:#x}, accepted: {accepted})", op.0);
            }
            if accepted && options.and_exit {
                quit(self.mtm());
            }
        }
    }
);

impl ItemView {
    fn new(
        mtm: MainThreadMarker,
        paths: Vec<PathBuf>,
        label: &str,
        options: Rc<Options>,
    ) -> Retained<Self> {
        let icon = icon_for_all(&paths, options.thumbnails);
        let tooltip = paths
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n");
        let icon_size = options.icon_size;
        let icons_only = options.icons_only;

        let this = Self::alloc(mtm).set_ivars(ItemIvars {
            paths,
            options,
            pressed: Cell::new(false),
            dragging: Cell::new(false),
        });
        // SAFETY: NSView's init has this signature.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        this.setTranslatesAutoresizingMaskIntoConstraints(false);
        this.setToolTip(Some(&NSString::from_str(&tooltip)));

        let image_view = NSImageView::imageViewWithImage(&icon, mtm);
        image_view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        let mut views: Vec<Retained<NSView>> = vec![Retained::into_super(Retained::into_super(
            image_view.clone(),
        ))];
        activate(&[
            image_view
                .widthAnchor()
                .constraintEqualToConstant(icon_size),
            image_view
                .heightAnchor()
                .constraintEqualToConstant(icon_size),
        ]);

        if !icons_only {
            let text = NSTextField::labelWithString(&NSString::from_str(label), mtm);
            text.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
            // Let the label shrink (and truncate) rather than widen the window.
            text.setContentCompressionResistancePriority_forOrientation(
                1.0,
                NSLayoutConstraintOrientation::Horizontal,
            );
            views.push(Retained::into_super(Retained::into_super(text)));
        }

        let row = NSStackView::stackViewWithViews(&NSArray::from_retained_slice(&views), mtm);
        row.setSpacing(8.0);
        row.setEdgeInsets(NSEdgeInsets {
            top: 4.0,
            left: 6.0,
            bottom: 4.0,
            right: 6.0,
        });
        row.setTranslatesAutoresizingMaskIntoConstraints(false);
        this.addSubview(&row);
        pin(&row, &this, 0.0, 0.0);
        this
    }

    fn set_pressed(&self, pressed: bool) {
        self.ivars().pressed.set(pressed);
        self.setNeedsDisplay(true);
    }

    fn begin_drag(&self, event: &NSEvent) {
        let ivars = self.ivars();
        let size = ivars.options.icon_size;
        let at = self.convertPoint_fromView(event.locationInWindow(), None);
        let frame = NSRect::new(
            NSPoint::new(at.x - size / 2.0, at.y - size / 2.0),
            NSSize::new(size, size),
        );

        let items: Vec<Retained<NSDraggingItem>> = ivars
            .paths
            .iter()
            .map(|path| {
                let url = file_url(path);
                let item = NSDraggingItem::initWithPasteboardWriter(
                    NSDraggingItem::alloc(),
                    ProtocolObject::from_ref(&*url),
                );
                let icon = icon_for(path, ivars.options.thumbnails);
                // SAFETY: an NSImage is a valid drag image.
                unsafe { item.setDraggingFrame_contents(frame, Some(icon.as_ref())) };
                item
            })
            .collect();

        if ivars.options.verbose {
            eprintln!("dnd: dragging {:?}", ivars.paths);
        }
        self.beginDraggingSessionWithItems_event_source(
            &NSArray::from_retained_slice(&items),
            event,
            ProtocolObject::from_ref(self),
        );
    }
}

// ---------------------------------------------------------------------------
// Drop target
// ---------------------------------------------------------------------------

struct DropIvars {
    options: Rc<Options>,
    /// The list kept dropped files are appended to (only with `--keep`).
    list: OnceCell<Retained<NSStackView>>,
    highlighted: Cell<bool>,
    /// Paths already shown in the list, so repeated drops do not duplicate.
    kept: RefCell<Vec<PathBuf>>,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements and we do not implement
    // Drop.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "DndDropView"]
    #[ivars = DropIvars]
    struct DropView;

    impl DropView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            if !self.ivars().highlighted.get() {
                return;
            }
            let b = self.bounds();
            let inset = NSRect::new(
                NSPoint::new(b.origin.x + 3.0, b.origin.y + 3.0),
                NSSize::new(b.size.width - 6.0, b.size.height - 6.0),
            );
            let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(inset, 8.0, 8.0);
            path.setLineWidth(3.0);
            NSColor::controlAccentColor().setStroke();
            path.stroke();
        }
    }

    unsafe impl NSObjectProtocol for DropView {}

    unsafe impl NSDraggingDestination for DropView {
        #[unsafe(method(draggingEntered:))]
        fn dragging_entered(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            // Ignore our own rows being dragged around inside the window.
            if sender.draggingSource().is_some() {
                return NSDragOperation::None;
            }
            self.set_highlighted(true);
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingExited:))]
        fn dragging_exited(&self, _sender: Option<&ProtocolObject<dyn NSDraggingInfo>>) {
            self.set_highlighted(false);
        }

        #[unsafe(method(performDragOperation:))]
        fn perform_drag_operation(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            self.set_highlighted(false);
            let paths = dropped_paths(sender);
            let accepted = !paths.is_empty();
            if accepted {
                self.accept(paths);
            }
            accepted
        }
    }
);

impl DropView {
    fn new(mtm: MainThreadMarker, frame: NSRect, options: Rc<Options>) -> Retained<Self> {
        let kept = options.items.iter().map(|i| i.path.clone()).collect();
        let this = Self::alloc(mtm).set_ivars(DropIvars {
            options,
            list: OnceCell::new(),
            highlighted: Cell::new(false),
            kept: RefCell::new(kept),
        });
        // SAFETY: NSView's designated initializer has this signature.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    fn set_highlighted(&self, highlighted: bool) {
        self.ivars().highlighted.set(highlighted);
        self.setNeedsDisplay(true);
    }

    fn accept(&self, paths: Vec<PathBuf>) {
        let ivars = self.ivars();
        output::print_paths(paths.iter().map(PathBuf::as_path));

        if let Some(list) = ivars.list.get() {
            let mut kept = ivars.kept.borrow_mut();
            let new: Vec<Item> = paths
                .into_iter()
                .filter(|p| !kept.contains(p))
                .map(|p| Item::new(p, ivars.options.basename))
                .collect();
            kept.extend(new.iter().map(|i| i.path.clone()));
            // "Drag all" rows are only built for the initial items; dropped
            // files are always added individually.
            add_items(self.mtm(), list, &new, &ivars.options, DragAll::Off);
        }

        if ivars.options.and_exit {
            quit(self.mtm());
        }
    }
}

/// Reads the file URLs from a drop and converts them to paths.
fn dropped_paths(sender: &ProtocolObject<dyn NSDraggingInfo>) -> Vec<PathBuf> {
    let pasteboard = sender.draggingPasteboard();
    let classes = NSArray::from_slice(&[NSURL::class()]);
    let yes = NSNumber::new_bool(true);
    // SAFETY: the key is a valid static and the value has the documented type.
    let options: Retained<NSDictionary<_, AnyObject>> = NSDictionary::from_slices(
        &[unsafe { NSPasteboardURLReadingFileURLsOnlyKey }],
        &[yes.as_ref()],
    );
    // SAFETY: NSURL conforms to NSPasteboardReading.
    let Some(objects) =
        (unsafe { pasteboard.readObjectsForClasses_options(&classes, Some(&options)) })
    else {
        return Vec::new();
    };
    objects
        .iter()
        .filter_map(|obj| obj.downcast::<NSURL>().ok())
        .filter_map(|url| url.path())
        .map(|path| PathBuf::from(path.to_string()))
        .collect()
}
