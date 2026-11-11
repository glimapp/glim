import Cocoa

// Reader controls stay alive when the workspace lends this capsule its actions.
final class FloatingReaderToolbar: NSView {
    private let controls: ReaderControls
    private var contextMenu: ReaderContextMenu?
    private var keyboardShortcuts: ReaderKeyboardShortcuts?
    private var items: [NSView]
    private var workspaceControls: ReaderControls?
    private let document = NSView()
    private let material: NSView
    private let interior: NSView
    private var measuredSizes: [NSSize] = []
    private var measuredViews: [ObjectIdentifier] = []
    private let viewport = NSView()
    private var bottomInset: CGFloat = 16
    private static let iconWidth: CGFloat = 28
    private static let spacing: CGFloat = 6
    private static let horizontalPadding: CGFloat = 12
    private static let maximumWidth: CGFloat = 640
    private let more = ReaderActionButton(title: "", target: nil, action: nil)
    private var overflowItems: [NSView] = []
    private var wrappers: [ObjectIdentifier: NSView] = [:]
    var workspaceHidden = false { didSet { material.isHidden = workspaceHidden } }
    convenience init(items: NSStackView) { self.init(controls: ReaderControls(stack: items)) }
    init(controls: ReaderControls) {
        self.controls = controls
        let items = controls.entries.map { $0.view }
        self.items = items
        let interior = NSView(); self.interior = interior
        #if GLIM_NATIVE_GLASS
        if #available(macOS 26.0, *) {
            let glass = NSGlassEffectView()
            glass.style = .regular; glass.contentView = interior
            material = glass
        } else {
            let effect = NSVisualEffectView()
            effect.material = .hudWindow; effect.blendingMode = .withinWindow; effect.state = .active
            effect.addSubview(interior); material = effect
        }
        #else
        let effect = NSVisualEffectView()
        effect.material = .hudWindow; effect.blendingMode = .withinWindow; effect.state = .active
        effect.addSubview(interior); material = effect
        #endif
        super.init(frame: .zero)
        for item in items {
            // The scroll document has one frame-based layout owner. Reparenting
            // removes stack gravity/distribution from the sizing feedback loop.
            item.removeFromSuperview()
            item.translatesAutoresizingMaskIntoConstraints = true
            let wrapper = NSView()
            wrappers[ObjectIdentifier(item)] = wrapper
            document.addSubview(wrapper); wrapper.addSubview(item)
            if let picker = item as? NSPopUpButton {
                picker.cell?.lineBreakMode = .byTruncatingMiddle
            }
        }
        for case let button as NSButton in items where !(button is NSPopUpButton) {
            button.isBordered = false; button.bezelStyle = .rounded
            button.controlSize = .regular
            button.setContentHuggingPriority(.defaultHigh, for: .horizontal)
            button.font = .systemFont(ofSize: 13, weight: .medium)
            let label = button.title == "+" ? L("Zoom in") : (["−", "-"].contains(button.title) ? L("Zoom out") : button.title)
            button.toolTip = label
            button.setAccessibilityLabel(label)
            if let symbol = ReaderMenu.toolbarSymbol(for: label),
               let image = NSImage(systemSymbolName: symbol, accessibilityDescription: label) {
                button.image = image
                button.imagePosition = .imageOnly; button.title = ""
            }
        }
        viewport.wantsLayer = true; viewport.layer?.masksToBounds = true
        viewport.addSubview(document)
        interior.addSubview(viewport)
        more.target = self; more.action = #selector(showMore)
        more.bezelStyle = .rounded; more.isBordered = false
        more.toolTip = L("More file actions"); more.setAccessibilityLabel(L("More file actions"))
        more.image = NSImage(systemSymbolName: "chevron.right.2", accessibilityDescription: L("More file actions"))
        more.imagePosition = .imageOnly
        interior.addSubview(more)
        addSubview(material)
        material.isHidden = false
        wantsLayer = true
        layer?.shadowColor = NSColor.shadowColor.cgColor
        layer?.shadowOpacity = 0.12; layer?.shadowRadius = 10
        layer?.shadowOffset = NSSize(width: 0, height: -3)
        setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        setAccessibilityRole(.toolbar)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }
    func setMoreLabel(_ label: String) {
        more.toolTip = label; more.setAccessibilityLabel(label)
    }
    func setWorkspaceControls(_ value: ReaderControls?) {
        guard workspaceControls !== value else { return }
        workspaceControls?.entries.forEach {
            wrappers.removeValue(forKey: ObjectIdentifier($0.view))?.removeFromSuperview()
            $0.view.removeFromSuperview()
        }
        workspaceControls = value
        let common = value?.entries.map { $0.view } ?? []
        // Fullscreen and collapse remain at the trailing edge for every reader.
        let trailing = common.filter { ["file-fullscreen", "hide-reader-toolbar"].contains($0.identifier?.rawValue ?? "") }
        let leading = common.filter { view in
            if trailing.contains(view) { return false }
            if view.identifier?.rawValue == "find-in-file" {
                return !controls.entries.contains { entry in
                    guard let button = entry.view as? NSButton else { return false }
                    return button.title == L("Find in file · ⌘F") || button.toolTip == L("Find in file · ⌘F")
                }
            }
            return true
        }
        items = leading + controls.entries.map { $0.view } + trailing
        for item in items where wrappers[ObjectIdentifier(item)] == nil {
            let wrapper = NSView()
            wrappers[ObjectIdentifier(item)] = wrapper
            item.removeFromSuperview(); item.translatesAutoresizingMaskIntoConstraints = true
            document.addSubview(wrapper); wrapper.addSubview(item)
        }
        needsLayout = true
    }
    private func measurements() -> [(NSView, NSSize)] {
        // Measure the visible controls directly: stack fittingSize can inherit a
        // previous narrow document frame and compress button cells to ellipses.
        return items.filter { !$0.isHidden }.map { item in
            var measured = item.intrinsicContentSize
            if let label = item as? NSTextField {
                label.textColor = .secondaryLabelColor
                label.setContentHuggingPriority(.defaultHigh, for: .horizontal)
                let font = label.font ?? NSFont.systemFont(ofSize: NSFont.systemFontSize(for: label.controlSize))
                let text = (label.stringValue as NSString).size(withAttributes: [.font: font])
                measured.width = ceil(text.width) + 8
                measured.height = max(ceil(text.height), label.cell?.cellSize.height ?? 0)
            }
            if let button = item as? NSButton {
                let title = (button as? NSPopUpButton)?.selectedItem?.title ?? button.title
                let font = button.font ?? button.cell?.font ?? NSFont.systemFont(ofSize: NSFont.systemFontSize(for: button.controlSize))
                let textWidth = ceil((title as NSString).size(withAttributes: [.font: font]).width)
                if let picker = button as? NSPopUpButton {
                    // A popup's intrinsic size can retain a previous large menu
                    // width. Size the control from its current title instead.
                    let imageWidth = picker.selectedItem?.image.map { $0.size.width + 6 } ?? 0
                    measured.width = textWidth + imageWidth + 40
                    picker.toolTip = title
                } else {
                    let imageWidth = button.image?.size.width ?? 0
                    switch button.imagePosition {
                    case .imageOnly: measured.width = Self.iconWidth
                    case .imageLeft, .imageRight, .imageLeading, .imageTrailing:
                        measured.width = max(28, textWidth + (imageWidth > 0 ? imageWidth + 6 : 0) + 32)
                    default: measured.width = max(28, max(textWidth, imageWidth) + 32)
                    }
                }
            } else if let segments = item as? NSSegmentedControl {
                let font = segments.font ?? NSFont.systemFont(ofSize: 13)
                let labelWidth = (0..<segments.segmentCount).reduce(CGFloat(0)) {
                    $0 + ceil(((segments.label(forSegment: $1) ?? "") as NSString).size(withAttributes: [.font: font]).width) + 24
                }
                measured.width = labelWidth
            }
            if !(item is NSControl) {
                // An unconstrained spacer has no content width. Never measure
                // its last stretched frame via fittingSize.
                measured.width = max(0, measured.width)
                measured.height = max(0, measured.height)
            }
            measured.width = max(0, measured.width)
            if let label = item as? NSTextField, !label.isEditable {
                measured.height = ceil(measured.height)
            } else {
                measured.height = max(28, measured.height)
            }
            for constraint in item.constraints where constraint.isActive && constraint.firstAttribute == .width && constraint.secondItem == nil {
                if constraint.relation == .greaterThanOrEqual || constraint.relation == .equal {
                    measured.width = max(measured.width, constraint.constant)
                }
            }
            return (item, measured)
        }
    }
    override func viewWillDraw() {
        for case let button as NSButton in items where !(button is NSPopUpButton) && !button.title.isEmpty {
            let label = button.title
            button.toolTip = label; button.setAccessibilityLabel(label)
            if let symbol = ReaderMenu.toolbarSymbol(for: label),
               let image = NSImage(systemSymbolName: symbol, accessibilityDescription: label) {
                button.image = image; button.imagePosition = .imageOnly; button.title = ""
            }
        }
        let entries = measurements()
        let sizes = entries.map { $0.1 }
        let visible = entries.map { ObjectIdentifier($0.0) }
        if sizes != measuredSizes || visible != measuredViews {
            measuredSizes = sizes; measuredViews = visible
            needsLayout = true
        }
        super.viewWillDraw()
    }
    override func layout() {
        super.layout()
        let entries = measurements()
        let padding = Self.horizontalPadding
        let gap = Self.spacing
        let contentWidth = entries.reduce(CGFloat(0)) { $0 + $1.1.width } + CGFloat(max(0, entries.count - 1)) * gap
        let rowHeight = entries.reduce(CGFloat(28)) { max($0, $1.1.height) }
        let available = max(0, min(Self.maximumWidth, bounds.width - 32) - padding * 2)
        let overflow = contentWidth > available
        let isPinned: (NSView) -> Bool = { ["file-fullscreen", "hide-reader-toolbar"].contains($0.identifier?.rawValue ?? "") }
        let pinned = entries.filter { isPinned($0.0) }
        var remaining = available - (overflow ? Self.iconWidth + gap : 0)
            - pinned.reduce(CGFloat(0)) { $0 + $1.1.width + gap }
        var visibleEntries: [(NSView, NSSize)] = []
        overflowItems = []
        for entry in entries {
            if !overflow || isPinned(entry.0) || entry.1.width <= remaining {
                visibleEntries.append(entry)
                if !isPinned(entry.0) { remaining -= entry.1.width + gap }
            } else {
                overflowItems.append(entry.0)
            }
        }
        let hasMore = !overflowItems.isEmpty
        let visibleWidth = visibleEntries.reduce(CGFloat(0)) { $0 + $1.1.width }
            + CGFloat(max(0, visibleEntries.count - 1)) * gap
        let width = min(bounds.width, visibleWidth + padding * 2 + (hasMore ? Self.iconWidth + gap : 0))
        material.frame = NSRect(x: (bounds.width - width) / 2, y: bottomInset, width: width, height: rowHeight + 16)
        interior.frame = material.bounds
        more.isHidden = !hasMore
        viewport.frame = NSRect(x: padding, y: 8, width: visibleWidth, height: rowHeight)
        more.frame = NSRect(x: width - padding - Self.iconWidth, y: 8, width: Self.iconWidth, height: rowHeight)
        document.frame = viewport.bounds
        for item in items { wrappers[ObjectIdentifier(item)]?.isHidden = true }
        var x: CGFloat = 0
        for (item, size) in visibleEntries {
            let wrapper = wrappers[ObjectIdentifier(item)]
            wrapper?.isHidden = false
            wrapper?.frame = NSRect(x: x, y: (document.bounds.height - size.height) / 2, width: size.width, height: size.height)
            item.autoresizingMask = []
            item.frame = NSRect(origin: .zero, size: size)
            x += size.width + gap
        }
        material.wantsLayer = true
        material.layer?.cornerRadius = material.bounds.height / 2
        material.layer?.masksToBounds = true
        #if GLIM_NATIVE_GLASS
        if #available(macOS 26.0, *), let glass = material as? NSGlassEffectView { glass.cornerRadius = material.bounds.height / 2 }
        #endif
    }
    @objc private func showMore() {
        let controls = ReaderControls(overflowItems.enumerated().map { ReaderControl("overflow-\($0.offset)", $0.element) })
        controls.menu().popUp(positioning: nil, at: NSPoint(x: 0, y: more.bounds.height + 6), in: more)
    }
    // Transparent margins never intercept document clicks or image drags.
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !workspaceHidden, material.frame.contains(convert(point, from: superview)) else { return nil }
        return super.hitTest(point)
    }
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let window = window { WorkspaceReaderToolbar.refresh(in: window) }
    }
    func install(in host: NSView, bottomInset: CGFloat = 16, managesContextMenu: Bool = true) {
        self.bottomInset = bottomInset
        frame = host.bounds
        autoresizingMask = [.width, .height]
        host.addSubview(self)
        keyboardShortcuts = ReaderKeyboardShortcuts(host: host, controls: controls)
        if managesContextMenu {
            contextMenu = ReaderContextMenu(host: host, kind: ReaderTelemetry.kind(controls)) { [weak self] _ in self?.controls.menu() }
        }
        needsLayout = true
    }
}

// One workspace supplies common actions to the visible native reader, or uses
// the same capsule over GPUI content. Native controls are never cloned.
final class WorkspaceReaderToolbar: NSView {
    private let fallback = FloatingReaderToolbar(controls: ReaderControls([]))
    private let restore = ReaderActionButton(title: "", target: nil, action: nil)
    private weak var active: FloatingReaderToolbar?
    private var common = ReaderControls([])
    private var payload = Data()
    private var visible = false
    private var collapsed = false
    private var fullscreen = false
    private var roundTop = false
    private var moreLabel = L("More file actions")
    private var refreshPending = false
    private let context: UnsafeMutableRawPointer
    private let callback: @convention(c) (UnsafeMutableRawPointer, UnsafePointer<CChar>) -> Void

    init(context: UnsafeMutableRawPointer,
         callback: @escaping @convention(c) (UnsafeMutableRawPointer, UnsafePointer<CChar>) -> Void) {
        self.context = context; self.callback = callback
        super.init(frame: .zero)
        setAccessibilityRole(.toolbar)
        fallback.install(in: self, managesContextMenu: false)
        restore.target = self; restore.action = #selector(invoke(_:))
        restore.identifier = NSUserInterfaceItemIdentifier("show-reader-toolbar")
        restore.bezelStyle = .rounded; restore.isBordered = false
        restore.toolTip = L("Show file toolbar"); restore.setAccessibilityLabel(L("Show file toolbar"))
        restore.image = NSImage(systemSymbolName: "rectangle.bottomthird.inset.filled", accessibilityDescription: L("Show file toolbar"))
        restore.isHidden = true
        addSubview(restore)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard visible else { return nil }
        if !restore.isHidden, let hit = restore.hitTest(convert(point, from: superview)) { return hit }
        guard !collapsed, active === fallback else { return nil }
        let hit = super.hitTest(point)
        return hit === self ? nil : hit
    }
    func update(parent: NSView, rect: NSRect, data: Data, visible: Bool, collapsed: Bool) {
        if superview !== parent { removeFromSuperview(); parent.addSubview(self) }
        frame = rect
        self.visible = visible; self.collapsed = collapsed
        if payload != data {
            payload = data
            let entries = (try? JSONSerialization.jsonObject(with: data)) as? [[String: Any]] ?? []
            fullscreen = entries.contains { $0["id"] as? String == "file-fullscreen" && $0["selection"] as? Bool == true }
            if let labels = entries.first(where: { $0["id"] as? String == "hide-reader-toolbar" }) {
                roundTop = labels["roundTop"] as? Bool ?? false
                moreLabel = labels["moreLabel"] as? String ?? L("More file actions")
                let label = labels["restoreLabel"] as? String ?? L("Show file toolbar")
                restore.toolTip = label; restore.setAccessibilityLabel(label)
            }
            let existing = Dictionary(uniqueKeysWithValues: common.entries.map { ($0.id, $0.view) })
            let updated = entries.compactMap { item -> ReaderControl? in
                guard let id = item["id"] as? String, let label = item["label"] as? String else { return nil }
                let button = existing[id] as? ReaderActionButton
                    ?? ReaderActionButton(title: "", target: self, action: #selector(invoke(_:)))
                button.identifier = NSUserInterfaceItemIdentifier(id)
                button.toolTip = label; button.setAccessibilityLabel(label)
                button.bezelStyle = .rounded; button.isBordered = false
                button.image = NSImage(systemSymbolName: item["symbol"] as? String ?? "ellipsis", accessibilityDescription: label)
                button.imagePosition = .imageOnly
                button.isEnabled = item["enabled"] as? Bool ?? true
                button.selection = item["selection"] as? Bool
                return ReaderControl(id, button)
            }
            if common.entries.map({ $0.id }) != updated.map({ $0.id }) {
                active?.setWorkspaceControls(nil)
                common = ReaderControls(updated)
            }
        }
        refresh()
    }
    @objc private func invoke(_ sender: NSButton) {
        guard let id = sender.identifier?.rawValue else { return }
        id.withCString { callback(context, $0) }
    }
    static func refresh(in window: NSWindow) {
        guard let content = window.contentView else { return }
        func visit(_ view: NSView) {
            if let toolbar = view as? WorkspaceReaderToolbar { toolbar.scheduleRefresh(); return }
            view.subviews.forEach(visit)
        }
        visit(content.superview ?? content)
    }
    private func scheduleRefresh() {
        guard !refreshPending else { return }
        refreshPending = true
        DispatchQueue.main.async { [weak self] in
            guard let self = self else { return }
            self.refreshPending = false; self.refresh()
        }
    }
    private func refresh() {
        guard let parent = superview else { return }
        guard visible else {
            active?.setWorkspaceControls(nil)
            active?.workspaceHidden = true
            active = nil
            fallback.isHidden = true
            restore.isHidden = true
            updateAccessibility()
            return
        }
        for case let preview as NativePreviewHost in parent.subviews where !preview.isHidden {
            if preview.roundTopCorners != roundTop { preview.roundTopCorners = roundTop }
        }
        func find(_ view: NSView) -> FloatingReaderToolbar? {
            guard !(view is WorkspaceReaderToolbar), !view.isHidden else { return nil }
            if let toolbar = view as? FloatingReaderToolbar { return toolbar }
            for child in view.subviews { if let toolbar = find(child) { return toolbar } }
            return nil
        }
        let target = parent.subviews.compactMap { find($0) }.first ?? fallback
        if active !== target {
            active?.setWorkspaceControls(nil)
            active?.workspaceHidden = true
            active = target
        }
        target.setWorkspaceControls(common)
        target.setMoreLabel(moreLabel)
        target.workspaceHidden = !visible || collapsed
        fallback.isHidden = target !== fallback || !visible || collapsed
        target.needsLayout = true
        target.layoutSubtreeIfNeeded()
        restore.isHidden = !collapsed || !fullscreen
        restore.frame = NSRect(x: 12, y: max(0, bounds.height - 40), width: 28, height: 28)
        if parent.subviews.last !== self { parent.addSubview(self, positioned: .above, relativeTo: nil) }
        updateAccessibility()
    }
    private func updateAccessibility() {
        guard let window = window else { return }
        let exposed = visible && (!restore.isHidden || !fallback.isHidden)
        setAccessibilityChildren(!restore.isHidden ? [restore] : (!fallback.isHidden ? [fallback] : []))
        let children = window.accessibilityChildren() ?? []
        let present = children.contains { ($0 as? WorkspaceReaderToolbar) === self }
        guard present != exposed else { return }
        let others = children.filter { ($0 as? WorkspaceReaderToolbar) !== self }
        window.setAccessibilityChildren(exposed ? others + [self] : others)
        NSAccessibility.post(element: window, notification: .layoutChanged)
    }
    func detach() {
        visible = false
        updateAccessibility()
        active?.setWorkspaceControls(nil)
        active?.workspaceHidden = true
        removeFromSuperview()
    }
}

@_cdecl("glim_reader_toolbar_create")
func glimReaderToolbarCreate(_ context: UnsafeMutableRawPointer,
    _ callback: @escaping @convention(c) (UnsafeMutableRawPointer, UnsafePointer<CChar>) -> Void) -> UnsafeMutableRawPointer {
    precondition(Thread.isMainThread)
    return Unmanaged.passRetained(WorkspaceReaderToolbar(context: context, callback: callback)).toOpaque()
}

@_cdecl("glim_reader_toolbar_update")
func glimReaderToolbarUpdate(_ handle: UnsafeMutableRawPointer, _ parentPointer: UnsafeMutableRawPointer,
    _ x: Double, _ y: Double, _ width: Double, _ height: Double,
    _ bytes: UnsafePointer<UInt8>, _ length: Int, _ visible: Bool, _ collapsed: Bool) {
    precondition(Thread.isMainThread)
    let toolbar = Unmanaged<WorkspaceReaderToolbar>.fromOpaque(handle).takeUnretainedValue()
    let gpui = Unmanaged<NSView>.fromOpaque(parentPointer).takeUnretainedValue()
    let parent = gpui.superview ?? gpui
    let rect = NSRect(x: gpui.bounds.minX + x,
        y: gpui.isFlipped ? gpui.bounds.minY + y : gpui.bounds.maxY - y - height,
        width: max(0, width), height: max(0, height))
    toolbar.update(parent: parent, rect: parent.convert(rect, from: gpui),
        data: Data(bytes: bytes, count: length), visible: visible, collapsed: collapsed)
}

@_cdecl("glim_reader_toolbar_release")
func glimReaderToolbarRelease(_ handle: UnsafeMutableRawPointer) {
    precondition(Thread.isMainThread)
    Unmanaged<WorkspaceReaderToolbar>.fromOpaque(handle).takeRetainedValue().detach()
}
