import Cocoa

@_cdecl("glim_preview_find")
func glimPreviewFind(_ handle: UnsafeMutableRawPointer, _ show: Bool) -> Bool {
    precondition(Thread.isMainThread)
    let preview = Unmanaged<NativePreviewController>.fromOpaque(handle).takeUnretainedValue()
    if show && preview.canFind { preview.showFind() }
    return preview.canFind
}

// Opaque handles are retained by one Rust reader, and used only on the UI thread.
@_cdecl("glim_preview_create")
func glimPreviewCreate(_ bytes: UnsafePointer<UInt8>, _ length: Int) -> UnsafeMutableRawPointer {
    precondition(Thread.isMainThread)
    let preview = NativePreviewController()
    preview.host.isHidden = true
    let data = Data(bytes: bytes, count: length)
    // AppKit/PDFKit/Quick Look loading occurs outside GPUI render/layout callbacks.
    DispatchQueue.main.async { [weak preview] in
        guard let preview = preview, !preview.closed else { return }
        do {
            try preview.load(NativePreviewRequest.decode(data))
        } catch {
            preview.host.subviews.forEach { $0.removeFromSuperview() }
            let label = NSTextField(wrappingLabelWithString: error.localizedDescription)
            label.frame = preview.host.bounds.insetBy(dx: 24, dy: 24)
            label.autoresizingMask = [.width, .height]
            preview.host.addSubview(label)
        }
    }
    return Unmanaged.passRetained(preview).toOpaque()
}

// Owns only a Rust channel sender. All C ABI encoding and pointer lifetime
// management stay in this bridge; the controller receives a Swift closure.
private final class PDFReloadCallback {
    private var context: UnsafeMutableRawPointer?
    private let complete: @convention(c) (UnsafeMutableRawPointer, Int32) -> Void

    init(context: UnsafeMutableRawPointer,
         complete: @escaping @convention(c) (UnsafeMutableRawPointer, Int32) -> Void) {
        self.context = context; self.complete = complete
    }

    func finish(_ outcome: PDFReloadOutcome) {
        guard let context = context else { return }
        self.context = nil
        let status: Int32
        switch outcome {
        case .applied: status = 0
        case .cancelled: status = 1
        case .notFound: status = 2
        case .permissionDenied: status = 3
        case .invalidDocument: status = 4
        case .locked: status = 5
        }
        complete(context, status)
    }

    deinit { finish(.cancelled) }
}

// On true Swift consumes context exactly once. On false ownership stays in Rust.
@_cdecl("glim_preview_reload_pdf")
func glimPreviewReloadPDF(_ handle: UnsafeMutableRawPointer,
                         _ context: UnsafeMutableRawPointer,
                         _ complete: @escaping @convention(c) (UnsafeMutableRawPointer, Int32) -> Void) -> Bool {
    precondition(Thread.isMainThread)
    let preview = Unmanaged<NativePreviewController>.fromOpaque(handle).takeUnretainedValue()
    guard preview.canReloadPDF else { return false }
    let callback = PDFReloadCallback(context: context, complete: complete)
    preview.startPDFReload { outcome in callback.finish(outcome) }
    return true
}

@_cdecl("glim_preview_layout")
func glimPreviewLayout(_ handle: UnsafeMutableRawPointer, _ parentPointer: UnsafeMutableRawPointer,
                       _ x: Double, _ y: Double, _ width: Double, _ height: Double) {
    precondition(Thread.isMainThread)
    let preview = Unmanaged<NativePreviewController>.fromOpaque(handle).takeUnretainedValue()
    let gpuiView = Unmanaged<NSView>.fromOpaque(parentPointer).takeUnretainedValue()
    // Share the content view with GPUI. Register native accessibility children
    // at the window level because AccessKit owns the content view's AX tree.
    let parent = gpuiView.superview ?? gpuiView
    if preview.host.presentingSlides {
        if width > 0 && height > 0 && preview.host.superview === parent {
            preview.host.frame = parent.bounds
            return
        }
        preview.host.stopPresentation()
    }
    if preview.host.superview !== parent {
        preview.host.removeFromSuperview()
        parent.addSubview(preview.host)
        preview.host.gpuiView = gpuiView
        preview.host.nextResponder = gpuiView
    }
    // GPUI uses top-left logical pixels; AppKit views may use bottom-left points.
    let rect = NSRect(x: gpuiView.bounds.minX + x,
        y: gpuiView.isFlipped ? gpuiView.bounds.minY + y : gpuiView.bounds.maxY - y - height,
        width: max(0, width), height: max(0, height))
    preview.host.frame = parent.convert(rect, from: gpuiView)
    // Native readers are siblings of GPUI's Metal view, so they need matching
    // clipping rather than relying on the GPUI container's rounded overflow.
    preview.host.wantsLayer = true
    preview.host.layer?.cornerRadius = preview.host.frame == parent.bounds ? 0 : 12
    // With tabs in the title row, the reader also meets the top frame corners.
    preview.host.layer?.maskedCorners = preview.host.roundTopCorners
        ? [.layerMinXMinYCorner, .layerMaxXMinYCorner, .layerMinXMaxYCorner, .layerMaxXMaxYCorner]
        : [.layerMinXMinYCorner, .layerMaxXMinYCorner]
    preview.host.layer?.masksToBounds = true
    let hidden = width <= 0 || height <= 0
    if preview.host.isHidden != hidden {
        preview.host.isHidden = hidden
        preview.host.updateAccessibility()
    }
    preview.imagePreview?.resume()
    preview.resumePendingPlayback()
    if let window = gpuiView.window { WorkspaceReaderToolbar.refresh(in: window) }
}

@_cdecl("glim_preview_hide")
func glimPreviewHide(_ handle: UnsafeMutableRawPointer) {
    precondition(Thread.isMainThread)
    let preview = Unmanaged<NativePreviewController>.fromOpaque(handle).takeUnretainedValue()
    guard !preview.host.isHidden else { return }
    preview.host.stopPresentation()
    preview.pdfSlideshow?.exit()
    preview.cancelContentCopy()
    if let responder = preview.host.window?.firstResponder as? NSView,
       responder.isDescendant(of: preview.host) {
        preview.host.window?.makeFirstResponder(preview.host.gpuiView)
    }
    preview.host.isHidden = true
    if let window = preview.host.window { WorkspaceReaderToolbar.refresh(in: window) }
    preview.host.updateAccessibility()
    preview.pendingPlayback = false
    if preview.mediaLoaded { preview.player.pause() }
    preview.imagePreview?.pause()
}

@_cdecl("glim_preview_release")
func glimPreviewRelease(_ handle: UnsafeMutableRawPointer) {
    precondition(Thread.isMainThread)
    let preview = Unmanaged<NativePreviewController>.fromOpaque(handle).takeRetainedValue()
    preview.close()
}

@_cdecl("glim_preview_take_navigation")
func glimPreviewTakeNavigation(_ handle: UnsafeMutableRawPointer) -> UnsafeMutablePointer<CChar>? {
    precondition(Thread.isMainThread)
    let preview = Unmanaged<NativePreviewController>.fromOpaque(handle).takeUnretainedValue()
    guard let html = preview.htmlPreview, let url = html.pendingNavigation else { return nil }
    html.pendingNavigation = nil
    return url.withCString { strdup($0) }
}
@_cdecl("glim_preview_free_string")
func glimPreviewFreeString(_ string: UnsafeMutablePointer<CChar>) { free(string) }
@_cdecl("glim_preview_reveal_anchor")
func glimPreviewRevealAnchor(_ handle: UnsafeMutableRawPointer, _ bytes: UnsafePointer<UInt8>, _ length: Int) {
    precondition(Thread.isMainThread)
    let preview = Unmanaged<NativePreviewController>.fromOpaque(handle).takeUnretainedValue()
    if let anchor = String(data: Data(bytes: bytes, count: length), encoding: .utf8) {
        preview.htmlAnchor = anchor; preview.htmlPreview?.reveal(anchor)
    }
}

// AppKit context menus live above both GPUI and native document surfaces.
// They never hide, pause or reconstruct a document reader.
private struct FileMenuItem: Decodable { let title: String?; let enabled: Bool; let symbol: String? }
private final class FileContextMenu: NSObject {
    let menu = NSMenu()
    var selection: Int64 = -2
    init(items: [FileMenuItem]) {
        super.init()
        menu.autoenablesItems = false
        for (index, specification) in items.enumerated() {
            if let title = specification.title {
                let item = NSMenuItem(title: title, action: #selector(choose(_:)), keyEquivalent: "")
                item.target = self; item.tag = index; item.isEnabled = specification.enabled
                if let symbol = specification.symbol {
                    item.image = NSImage(systemSymbolName: symbol, accessibilityDescription: title)
                    item.representedObject = symbol
                }
                menu.addItem(item)
            } else {
                menu.addItem(.separator())
            }
        }
        ReaderMenu.decorate(menu)
    }
    @objc private func choose(_ item: NSMenuItem) { selection = Int64(item.tag) }
}
@_cdecl("glim_context_menu_create")
func glimContextMenuCreate(_ parent: UnsafeMutableRawPointer, _ x: Double, _ y: Double,
                          _ bytes: UnsafePointer<UInt8>, _ length: Int) -> UnsafeMutableRawPointer? {
    precondition(Thread.isMainThread)
    guard let items = try? JSONDecoder().decode([FileMenuItem].self, from: Data(bytes: bytes, count: length)) else { return nil }
    let controller = FileContextMenu(items: items)
    let view = Unmanaged<NSView>.fromOpaque(parent).takeUnretainedValue()
    DispatchQueue.main.async { [weak controller, weak view] in
        guard let controller = controller else { return }
        guard let view = view, view.window != nil else { controller.selection = -1; return }
        let point = NSPoint(x: view.bounds.minX + x, y: view.isFlipped ? view.bounds.minY + y : view.bounds.maxY - y)
        controller.menu.popUp(positioning: nil, at: point, in: view)
        if controller.selection == -2 { controller.selection = -1 }
    }
    return Unmanaged.passRetained(controller).toOpaque()
}
@_cdecl("glim_context_menu_selection")
func glimContextMenuSelection(_ handle: UnsafeMutableRawPointer) -> Int64 {
    precondition(Thread.isMainThread)
    return Unmanaged<FileContextMenu>.fromOpaque(handle).takeUnretainedValue().selection
}
@_cdecl("glim_context_menu_release")
func glimContextMenuRelease(_ handle: UnsafeMutableRawPointer) {
    precondition(Thread.isMainThread)
    let controller = Unmanaged<FileContextMenu>.fromOpaque(handle).takeRetainedValue()
    if controller.selection == -2 { controller.menu.cancelTrackingWithoutAnimation() }
}

private final class PathHintPanel: NSPanel {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}
private final class FilePathHint {
    private weak var parent: NSWindow?
    private let anchor: NSRect
    private let panel: PathHintPanel
    private var task: DispatchWorkItem?
    private var monitor: Any?
    private var observers: [NSObjectProtocol] = []
    private var closed = false
    init(parent: NSWindow, anchor: NSRect, title: String, path: String, directory: Bool) {
        self.parent = parent; self.anchor = anchor
        panel = PathHintPanel(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.isReleasedWhenClosed = false; panel.isOpaque = false; panel.backgroundColor = .clear
        panel.hasShadow = false; panel.ignoresMouseEvents = true; panel.hidesOnDeactivate = true
        panel.appearance = parent.effectiveAppearance
        panel.level = .floating; panel.collectionBehavior = [.transient, .ignoresCycle, .fullScreenAuxiliary]
        let content = NSView()
        let surface = NSView(); surface.wantsLayer = true
        surface.layer?.cornerRadius = 6; surface.layer?.borderWidth = 0.5
        parent.effectiveAppearance.performAsCurrentDrawingAppearance {
            surface.layer?.backgroundColor = NSColor.controlBackgroundColor.cgColor
            surface.layer?.borderColor = NSColor.separatorColor.withAlphaComponent(0.45).cgColor
        }
        surface.layer?.shadowColor = NSColor.black.cgColor
        surface.layer?.shadowOpacity = 0.10; surface.layer?.shadowRadius = 4
        surface.layer?.shadowOffset = NSSize(width: 0, height: -1)
        content.addSubview(surface)
        let stack = NSStackView(); stack.orientation = .vertical; stack.alignment = .leading; stack.spacing = 4
        stack.translatesAutoresizingMaskIntoConstraints = false; surface.addSubview(stack)
        let heading = NSStackView(); heading.orientation = .horizontal; heading.spacing = 6
        let icon = NSImageView(image: NSImage(systemSymbolName: directory ? "folder" : "doc.text", accessibilityDescription: nil) ?? NSImage())
        icon.contentTintColor = .secondaryLabelColor
        icon.widthAnchor.constraint(equalToConstant: 14).isActive = true; icon.heightAnchor.constraint(equalToConstant: 14).isActive = true
        let titleFont = NSFont.systemFont(ofSize: 12, weight: .medium)
        let pathFont = NSFont.systemFont(ofSize: 11)
        let name = NSTextField(wrappingLabelWithString: title); name.font = titleFont
        name.textColor = .labelColor; name.maximumNumberOfLines = 0; name.lineBreakMode = .byCharWrapping
        if !path.isEmpty { heading.addArrangedSubview(icon) }
        heading.addArrangedSubview(name); stack.addArrangedSubview(heading)
        let location = NSTextField(wrappingLabelWithString: path); location.font = pathFont
        location.textColor = .secondaryLabelColor; location.lineBreakMode = .byCharWrapping; location.maximumNumberOfLines = 0
        if !path.isEmpty { stack.addArrangedSubview(location) }
        let screen = parent.screen?.visibleFrame ?? NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 800, height: 600)
        let titleWidth = (title.components(separatedBy: "\n").map {
            ($0 as NSString).size(withAttributes: [.font: titleFont]).width
        }.max() ?? 0) + (path.isEmpty ? 0 : 20)
        let pathWidth = path.components(separatedBy: "\n").map {
            ($0 as NSString).size(withAttributes: [.font: pathFont]).width
        }.max() ?? 0
        let width = min(max(path.isEmpty ? 120 : 220, ceil(max(titleWidth, pathWidth)) + 24), min(420, screen.width - 44))
        let nameWidth = width - (path.isEmpty ? 24 : 44)
        name.preferredMaxLayoutWidth = nameWidth; location.preferredMaxLayoutWidth = width - 24
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: surface.leadingAnchor, constant: 12),
            stack.trailingAnchor.constraint(equalTo: surface.trailingAnchor, constant: -12),
            stack.topAnchor.constraint(equalTo: surface.topAnchor, constant: 9),
            stack.bottomAnchor.constraint(equalTo: surface.bottomAnchor, constant: -9)
        ])
        let titleHeight = name.cell?.cellSize(forBounds: NSRect(x: 0, y: 0, width: nameWidth, height: CGFloat.greatestFiniteMagnitude)).height ?? 15
        let pathHeight = location.cell?.cellSize(forBounds: NSRect(x: 0, y: 0, width: width - 24, height: CGFloat.greatestFiniteMagnitude)).height ?? 14
        let height = min(ceil(max(15, titleHeight) + (path.isEmpty ? 18 : pathHeight + 22)), screen.height - 44)
        let panelWidth = width + 12, panelHeight = height + 12
        let x = min(max(screen.minX + 6, anchor.minX - 6), screen.maxX - panelWidth - 6)
        var y = anchor.minY - panelHeight
        if y < screen.minY + 6 { y = min(anchor.maxY, screen.maxY - panelHeight - 6) }
        panel.setFrame(NSRect(x: x, y: y, width: panelWidth, height: panelHeight), display: false)
        surface.frame = NSRect(x: 6, y: 6, width: width, height: height)
        surface.layer?.shadowPath = CGPath(roundedRect: surface.bounds, cornerWidth: 6, cornerHeight: 6, transform: nil)
        panel.contentView = content; surface.layoutSubtreeIfNeeded()
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.mouseMoved, .leftMouseDragged, .rightMouseDragged, .scrollWheel, .leftMouseDown, .rightMouseDown, .keyDown]) { [weak self] event in
            guard let self = self else { return event }
            if event.type == .mouseMoved && self.anchor.contains(NSEvent.mouseLocation) && NSEvent.pressedMouseButtons == 0 {
                self.panel.orderOut(nil); self.schedule()
            } else { self.close() }
            return event
        }
        for notification in [NSWindow.didResignKeyNotification, NSWindow.didMoveNotification, NSWindow.didResizeNotification, NSWindow.willCloseNotification] {
            observers.append(NotificationCenter.default.addObserver(forName: notification, object: parent, queue: .main) { [weak self] _ in self?.close() })
        }
        schedule()
    }
    private func schedule() {
        task?.cancel()
        guard !closed else { return }
        let task = DispatchWorkItem { [weak self] in
            guard let self = self, !self.closed, let parent = self.parent, parent.isKeyWindow,
                  self.anchor.contains(NSEvent.mouseLocation), NSEvent.pressedMouseButtons == 0 else { return }
            if parent.childWindows?.contains(self.panel) != true { parent.addChildWindow(self.panel, ordered: .above) }
            self.panel.orderFront(nil)
        }
        self.task = task; DispatchQueue.main.asyncAfter(deadline: .now() + 0.45, execute: task)
    }
    func close() {
        guard !closed else { return }; closed = true; task?.cancel(); task = nil
        if let monitor = monitor { NSEvent.removeMonitor(monitor); self.monitor = nil }
        observers.forEach { NotificationCenter.default.removeObserver($0) }; observers.removeAll()
        parent?.removeChildWindow(panel); panel.orderOut(nil)
    }
    deinit { close() }
}
@_cdecl("glim_path_hint_create")
func glimPathHintCreate(_ parentPointer: UnsafeMutableRawPointer, _ x: Double, _ y: Double, _ width: Double, _ height: Double,
                       _ title: UnsafePointer<UInt8>, _ titleLength: Int, _ path: UnsafePointer<UInt8>, _ pathLength: Int,
                       _ directory: Bool) -> UnsafeMutableRawPointer? {
    precondition(Thread.isMainThread)
    let view = Unmanaged<NSView>.fromOpaque(parentPointer).takeUnretainedValue()
    guard let window = view.window else { return nil }
    let rect = NSRect(x: view.bounds.minX + x, y: view.isFlipped ? view.bounds.minY + y : view.bounds.maxY - y - height, width: width, height: height)
    let anchor = window.convertToScreen(view.convert(rect, to: nil))
    let hint = FilePathHint(parent: window, anchor: anchor,
        title: String(decoding: UnsafeBufferPointer(start: title, count: titleLength), as: UTF8.self),
        path: String(decoding: UnsafeBufferPointer(start: path, count: pathLength), as: UTF8.self), directory: directory)
    return Unmanaged.passRetained(hint).toOpaque()
}
@_cdecl("glim_path_hint_release")
func glimPathHintRelease(_ pointer: UnsafeMutableRawPointer) {
    precondition(Thread.isMainThread)
    let hint = Unmanaged<FilePathHint>.fromOpaque(pointer).takeRetainedValue(); hint.close()
}
