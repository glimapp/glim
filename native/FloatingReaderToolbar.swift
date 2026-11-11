import Cocoa

final class ReaderActionButton: NSButton {
    private var hovered = false
    private var activated = false
    private var completed = false
    private var feedbackTask: DispatchWorkItem?
    private var hoverArea: NSTrackingArea?

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let area = hoverArea { removeTrackingArea(area) }
        let area = NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self)
        addTrackingArea(area); hoverArea = area
    }
    override func mouseEntered(with event: NSEvent) { hovered = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { hovered = false; needsDisplay = true }
    override func draw(_ dirtyRect: NSRect) {
        let selected = state == .on
        let pressed = cell?.isHighlighted == true || activated
        let shape = NSBezierPath(roundedRect: bounds.insetBy(dx: 1, dy: 1), xRadius: 7, yRadius: 7)
        let fill: NSColor = selected ? .controlAccentColor.withAlphaComponent(pressed ? 0.30 : 0.20)
            : .labelColor.withAlphaComponent(pressed ? 0.18 : (hovered && isEnabled ? 0.09 : 0))
        fill.setFill(); shape.fill()
        let tint: NSColor = !isEnabled ? .disabledControlTextColor : (selected ? .controlAccentColor : .labelColor)
        if contentTintColor != tint { contentTintColor = tint }
        if completed {
            NSColor.controlAccentColor.setStroke()
            let check = NSBezierPath()
            let direction: CGFloat = isFlipped ? -1 : 1
            check.move(to: NSPoint(x: bounds.midX - 5, y: bounds.midY))
            check.line(to: NSPoint(x: bounds.midX - 1, y: bounds.midY - 4 * direction))
            check.line(to: NSPoint(x: bounds.midX + 6, y: bounds.midY + 5 * direction))
            check.lineWidth = 2; check.lineCapStyle = .round; check.lineJoinStyle = .round; check.stroke()
        } else { super.draw(dirtyRect) }
    }
    override func sendAction(_ action: Selector?, to target: Any?) -> Bool {
        let sent = super.sendAction(action, to: target)
        if sent && !completed { feedback(success: false) }
        return sent
    }
    func showCompletion() { feedback(success: true) }
    private func feedback(success: Bool) {
        feedbackTask?.cancel()
        activated = true; completed = success; needsDisplay = true
        let task = DispatchWorkItem { [weak self] in
            self?.activated = false; self?.completed = false; self?.needsDisplay = true
            self?.feedbackTask = nil
        }
        feedbackTask = task
        DispatchQueue.main.asyncAfter(deadline: .now() + (success ? 1.0 : 0.28), execute: task)
    }
    deinit { feedbackTask?.cancel() }
}

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
    private var measuredSize = NSSize.zero
    private var measuredViews: [ObjectIdentifier] = []
    private let viewport = NSScrollView()
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
        viewport.drawsBackground = false
        viewport.hasHorizontalScroller = false; viewport.hasVerticalScroller = false
        viewport.scrollerStyle = .overlay; viewport.autohidesScrollers = true
        viewport.documentView = document
        interior.addSubview(viewport)
        more.target = self; more.action = #selector(showMore)
        more.bezelStyle = .rounded; more.isBordered = false
        more.toolTip = L("More file actions"); more.setAccessibilityLabel(L("More file actions"))
        more.image = NSImage(systemSymbolName: "ellipsis", accessibilityDescription: L("More file actions"))
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
        invalidateIntrinsicContentSize(); needsLayout = true
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
                measured.width = max(ceil(text.width) + 4, label.cell?.cellSize.width ?? 0)
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
                    case .imageOnly: measured.width = max(28, imageWidth + 12)
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
    private var contentSize: NSSize {
        let entries = measurements()
        return NSSize(width: entries.reduce(0) { $0 + $1.1.width } + CGFloat(max(0, entries.count - 1)) * 6,
                      height: entries.reduce(CGFloat(28)) { max($0, $1.1.height) })
    }
    override var intrinsicContentSize: NSSize {
        let size = contentSize
        return NSSize(width: size.width + 24, height: max(44, size.height + 16))
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
        let size = contentSize
        let visible = items.filter { !$0.isHidden }.map { ObjectIdentifier($0) }
        if size != measuredSize || visible != measuredViews {
            measuredSize = size; measuredViews = visible
            invalidateIntrinsicContentSize(); needsLayout = true
        }
        super.viewWillDraw()
    }
    override func layout() {
        super.layout()
        let content = contentSize
        let width = min(bounds.width, content.width + 24)
        material.frame = NSRect(x: (bounds.width - width) / 2, y: 0, width: width, height: max(28, content.height) + 16)
        interior.frame = material.bounds
        let overflow = content.width > max(0, width - 24)
        more.isHidden = !overflow
        viewport.frame = NSRect(x: 12, y: 8, width: max(0, width - 24 - (overflow ? 34 : 0)), height: max(28, content.height))
        more.frame = NSRect(x: max(0, width - 40), y: 8, width: 28, height: max(28, content.height))
        let entries = measurements()
        document.frame = NSRect(origin: .zero, size: viewport.contentSize)
        let pinned = entries.filter { ["file-fullscreen", "hide-reader-toolbar"].contains($0.0.identifier?.rawValue ?? "") }
        var remaining = viewport.contentSize.width - pinned.reduce(CGFloat(0)) { $0 + $1.1.width + 6 }
        overflowItems = []
        var x: CGFloat = 0
        for (item, size) in entries {
            let keep = !overflow || pinned.contains { $0.0 === item } || size.width <= remaining
            let wrapper = wrappers[ObjectIdentifier(item)]
            wrapper?.isHidden = item.isHidden || !keep
            guard keep else { overflowItems.append(item); continue }
            if !pinned.contains(where: { $0.0 === item }) { remaining -= size.width + 6 }
            wrapper?.frame = NSRect(x: x, y: (document.bounds.height - size.height) / 2, width: size.width, height: size.height)
            item.frame = NSRect(origin: .zero, size: size)
            x += size.width + 6
        }
        for item in items where item.isHidden { wrappers[ObjectIdentifier(item)]?.isHidden = true }
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
        translatesAutoresizingMaskIntoConstraints = false
        host.addSubview(self)
        keyboardShortcuts = ReaderKeyboardShortcuts(host: host, controls: controls)
        if managesContextMenu {
            contextMenu = ReaderContextMenu(host: host, kind: ReaderTelemetry.kind(controls)) { [weak self] _ in self?.controls.menu() }
        }
        let maximumWidth = widthAnchor.constraint(equalTo: host.widthAnchor, constant: -32)
        NSLayoutConstraint.activate([
            centerXAnchor.constraint(equalTo: host.centerXAnchor),
            bottomAnchor.constraint(equalTo: host.bottomAnchor, constant: -bottomInset),
            maximumWidth
        ])
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
            active?.setWorkspaceControls(nil)
            let entries = (try? JSONSerialization.jsonObject(with: data)) as? [[String: Any]] ?? []
            fullscreen = entries.contains { $0["id"] as? String == "file-fullscreen" && $0["selected"] as? Bool == true }
            if let labels = entries.first(where: { $0["id"] as? String == "hide-reader-toolbar" }) {
                roundTop = labels["roundTop"] as? Bool ?? false
                moreLabel = labels["moreLabel"] as? String ?? L("More file actions")
                let label = labels["restoreLabel"] as? String ?? L("Show file toolbar")
                restore.toolTip = label; restore.setAccessibilityLabel(label)
            }
            common = ReaderControls(entries.compactMap { item in
                guard let id = item["id"] as? String, let label = item["label"] as? String else { return nil }
                let button = ReaderActionButton(title: "", target: self, action: #selector(invoke(_:)))
                button.identifier = NSUserInterfaceItemIdentifier(id)
                button.toolTip = label; button.setAccessibilityLabel(label)
                button.bezelStyle = .rounded; button.isBordered = false
                button.setButtonType(["reader-preview", "reader-source", "word-wrap", "file-fullscreen"].contains(id) ? .toggle : .momentaryPushIn)
                button.image = NSImage(systemSymbolName: item["symbol"] as? String ?? "ellipsis", accessibilityDescription: label)
                button.imagePosition = .imageOnly
                button.isEnabled = item["enabled"] as? Bool ?? true
                button.state = item["selected"] as? Bool == true ? .on : .off
                return ReaderControl(id, button)
            })
        }
        refresh()
    }
    @objc private func invoke(_ sender: NSButton) {
        guard let id = sender.identifier?.rawValue else { return }
        if let entries = (try? JSONSerialization.jsonObject(with: payload)) as? [[String: Any]],
           let item = entries.first(where: { $0["id"] as? String == id }) {
            sender.state = item["selected"] as? Bool == true ? .on : .off
        }
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
