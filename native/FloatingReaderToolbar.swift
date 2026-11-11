import Cocoa

// Reader controls stay alive when the workspace lends this capsule its actions.
final class FloatingReaderToolbar: NSView, NSPopoverDelegate {
    private let controls: ReaderControls
    private var profile: String
    private var separators: [NSView] = []
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
    private static let spacing: CGFloat = 4
    private static let horizontalPadding: CGFloat = 10
    private static let maximumWidth: CGFloat = 640
    private let more = ReaderActionButton(title: "", target: nil, action: nil)
    private var overflowItems: [NSView] = []
    private var overflowPopover: NSPopover?
    private var lentControls: [NSView] = []
    private var wrappers: [ObjectIdentifier: NSView] = [:]
    var workspaceHidden = false {
        didSet {
            guard oldValue != workspaceHidden else { return }
            material.isHidden = workspaceHidden
            if workspaceHidden { overflowPopover?.close() }
        }
    }
    convenience init(profile: String, items: NSStackView) { self.init(profile: profile, controls: ReaderControls(stack: items)) }
    init(profile: String = "native", controls: ReaderControls) {
        self.profile = profile
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
            button.symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 16, weight: .regular)
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
        let ordered = ReaderActionPresets.profiles[profile]!
        self.items = ordered.compactMap { id in items.first { $0.identifier?.rawValue == id } }
        setAccessibilityRole(.toolbar)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }
    func setMoreLabel(_ label: String) {
        more.toolTip = label; more.setAccessibilityLabel(label)
    }
    func setProfile(_ value: String) {
        guard profile != value else { return }
        precondition(ReaderActionPresets.profiles[value] != nil)
        profile = value
        let current = workspaceControls
        setWorkspaceControls(nil)
        setWorkspaceControls(current)
    }
    func setWorkspaceControls(_ value: ReaderControls?) {
        guard workspaceControls !== value else { return }
        overflowPopover?.close()
        workspaceControls?.entries.forEach {
            wrappers.removeValue(forKey: ObjectIdentifier($0.view))?.removeFromSuperview()
            $0.view.removeFromSuperview()
        }
        workspaceControls = value
        let common = value?.entries.map { $0.view } ?? []
        let candidates = common + controls.entries.map { $0.view }
        let ids = ReaderActionPresets.profiles[profile]!
        items = ids.compactMap { id in candidates.last { $0.identifier?.rawValue == id } }
        for item in items where wrappers[ObjectIdentifier(item)] == nil {
            let wrapper = NSView()
            wrappers[ObjectIdentifier(item)] = wrapper
            item.removeFromSuperview(); item.translatesAutoresizingMaskIntoConstraints = true
            document.addSubview(wrapper); wrapper.addSubview(item)
        }
        needsLayout = true
    }
    private func width(for item: NSView) -> CGFloat {
        let width = ReaderActionPresets.layout[item.identifier!.rawValue]!.width
        guard item.identifier?.rawValue == "zoom-status", let label = item as? NSTextField else { return width }
        let textWidth = (label.stringValue as NSString).size(withAttributes: [
            .font: label.font ?? NSFont.systemFont(ofSize: 13)
        ]).width
        return max(width, ceil(textWidth) + 4)
    }
    private func measurements() -> [(NSView, NSSize)] {
        items.filter { !$0.isHidden }.map { item in
            let width = width(for: item)
            for constraint in item.constraints where constraint.isActive && constraint.firstAttribute == .width && constraint.secondItem == nil {
                constraint.isActive = false
            }
            if let label = item as? NSTextField, !label.isEditable {
                label.textColor = .secondaryLabelColor
                label.lineBreakMode = item.identifier?.rawValue == "zoom-status" ? .byClipping : .byTruncatingTail
                label.maximumNumberOfLines = 1
                label.toolTip = label.stringValue
                return (item, NSSize(width: width, height: 18))
            }
            return (item, NSSize(width: width, height: 28))
        }
    }
    private func group(_ view: NSView) -> String {
        ReaderActionPresets.layout[view.identifier!.rawValue]!.group
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
        func groupSpace(_ values: [(NSView, NSSize)]) -> CGFloat {
            zip(values, values.dropFirst()).reduce(CGFloat(0)) { $0 + (group($1.0.0) == group($1.1.0) ? 0 : 9) }
        }
        let contentWidth = groupSpace(entries) + entries.reduce(CGFloat(0)) { $0 + $1.1.width } + CGFloat(max(0, entries.count - 1)) * gap
        let rowHeight = entries.reduce(CGFloat(28)) { max($0, $1.1.height) }
        let available = max(0, min(Self.maximumWidth, bounds.width - 32) - padding * 2)
        let overflow = contentWidth > available
        let isPinned: (NSView) -> Bool = { ["file-fullscreen", "hide-reader-toolbar"].contains($0.identifier?.rawValue ?? "") }
        let pinned = entries.filter { isPinned($0.0) }
        var remaining = available - groupSpace(entries) - (overflow ? Self.iconWidth + gap : 0)
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
        let visibleWidth = groupSpace(visibleEntries) + visibleEntries.reduce(CGFloat(0)) { $0 + $1.1.width }
            + CGFloat(max(0, visibleEntries.count - 1)) * gap
        let width = min(bounds.width, visibleWidth + padding * 2 + (hasMore ? Self.iconWidth + gap : 0))
        material.frame = NSRect(x: (bounds.width - width) / 2, y: bottomInset, width: width, height: rowHeight + 12)
        interior.frame = material.bounds
        more.isHidden = !hasMore
        viewport.frame = NSRect(x: padding, y: 6, width: visibleWidth, height: rowHeight)
        more.frame = NSRect(x: width - padding - Self.iconWidth, y: 6, width: Self.iconWidth, height: rowHeight)
        document.frame = viewport.bounds
        let visibleIDs = Set(visibleEntries.map { ObjectIdentifier($0.0) })
        for (id, wrapper) in wrappers {
            let hidden = !visibleIDs.contains(id)
            if wrapper.isHidden != hidden { wrapper.isHidden = hidden }
        }
        separators.forEach { $0.removeFromSuperview() }; separators.removeAll()
        var previousGroup: String?
        var x: CGFloat = 0
        for (item, size) in visibleEntries {
            let nextGroup = group(item)
            if let previousGroup = previousGroup, previousGroup != nextGroup {
                let separator = NSBox(); separator.boxType = .separator
                separator.frame = NSRect(x: x + 4, y: 6, width: 1, height: 16)
                document.addSubview(separator); separators.append(separator)
                x += 9
            }
            previousGroup = nextGroup
            let wrapper = wrappers[ObjectIdentifier(item)]
            wrapper?.frame = NSRect(x: x, y: (document.bounds.height - size.height) / 2, width: size.width, height: size.height)
            guard !lentControls.contains(item) else { continue }
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
        if let popover = overflowPopover { popover.close(); return }
        guard !overflowItems.isEmpty else { return }
        let controller = NSViewController()
        let rows = NSStackView(); rows.orientation = .vertical; rows.alignment = .leading; rows.spacing = 4
        rows.edgeInsets = NSEdgeInsets(top: 10, left: 10, bottom: 10, right: 10)
        lentControls = overflowItems
        for item in lentControls {
            item.removeFromSuperview()
            let width = width(for: item)
            let row = NSView(frame: NSRect(x: 0, y: 0, width: 260, height: 28))
            row.widthAnchor.constraint(equalToConstant: 260).isActive = true
            row.heightAnchor.constraint(equalToConstant: 28).isActive = true
            item.frame = NSRect(x: 0, y: 0, width: width, height: 28); row.addSubview(item)
            if let button = item as? NSButton, button.imagePosition == .imageOnly {
                let label = NSTextField(labelWithString: button.toolTip ?? "")
                label.frame = NSRect(x: width + 8, y: 6, width: 250 - width, height: 18)
                label.lineBreakMode = .byTruncatingTail
                row.addSubview(label)
            }
            rows.addArrangedSubview(row)
        }
        controller.view = rows
        let popover = NSPopover(); popover.behavior = .transient; popover.delegate = self
        popover.contentViewController = controller
        popover.contentSize = NSSize(width: 280, height: CGFloat(lentControls.count) * 32 + 16)
        overflowPopover = popover
        popover.show(relativeTo: more.bounds, of: more, preferredEdge: .maxY)
    }
    func popoverDidClose(_ notification: Notification) {
        for item in lentControls { item.removeFromSuperview(); wrappers[ObjectIdentifier(item)]?.addSubview(item) }
        lentControls.removeAll(); overflowPopover = nil; needsLayout = true
    }
    // Transparent margins never intercept document clicks or image drags.
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHiddenOrHasHiddenAncestor, !workspaceHidden,
              material.frame.contains(convert(point, from: superview)) else { return nil }
        // Route through the live controls, independently of the glass view's
        // decorative subviews and their compositing order.
        for item in [more] + items.reversed() where !item.isHiddenOrHasHiddenAncestor && item.window === window {
            let local = item.convert(point, from: superview)
            guard item.visibleRect.contains(local), let parent = item.superview else { continue }
            if let hit = item.hitTest(parent.convert(point, from: superview)) { return hit }
        }
        return self
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
        return fallback.hitTest(convert(point, from: superview))
    }
    func update(parent: NSView, rect: NSRect, data: Data, visible: Bool, collapsed: Bool) {
        if superview !== parent { removeFromSuperview(); parent.addSubview(self) }
        if frame != rect { frame = rect }
        self.visible = visible; self.collapsed = collapsed
        if payload != data {
            payload = data
            let entries = (try? JSONSerialization.jsonObject(with: data)) as? [[String: Any]] ?? []
            if let profile = entries.first?["profile"] as? String { fallback.setProfile(profile) }
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
                button.symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 16, weight: .regular)
                let enabled = item["enabled"] as? Bool ?? true
                if button.isEnabled != enabled { button.isEnabled = enabled }
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
        let hideFallback = target !== fallback || !visible || collapsed
        if fallback.isHidden != hideFallback { fallback.isHidden = hideFallback }
        target.layoutSubtreeIfNeeded()
        let hideRestore = !collapsed || !fullscreen
        if restore.isHidden != hideRestore { restore.isHidden = hideRestore }
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
