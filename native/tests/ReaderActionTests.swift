import Cocoa

private final class ActionRecorder: NSObject {
    var ids: [String] = []
    @objc func invoke(_ sender: ReaderActionButton) {
        precondition(sender.state == (sender.selection == true ? .on : .off))
        ids.append(sender.identifier?.rawValue ?? "command")
    }
}

func readerActionState() throws {
    _ = NSApplication.shared
    let recorder = ActionRecorder()
    let command = ReaderActionButton(title: "Zoom", target: recorder, action: #selector(ActionRecorder.invoke(_:)))
    for type: NSButton.ButtonType in [.momentaryLight, .momentaryPushIn, .toggle] {
        command.setButtonType(type)
        command.isBordered = false
        command.performClick(nil)
        precondition(command.state == .off, "Commands must not latch AppKit's click state")
    }
    command.showCompletion()
    RunLoop.current.run(until: Date(timeIntervalSinceNow: 1.1))
    precondition(command.state == .off)
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1100, height: 800), styleMask: [.titled, .resizable, .fullSizeContentView], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    let parent = NSView(frame: NSRect(x: 0, y: 0, width: 900, height: 600))
    window.contentView!.addSubview(parent)
    let toolbar = WorkspaceReaderToolbar(context: Unmanaged.passUnretained(recorder).toOpaque()) { pointer, id in
        Unmanaged<ActionRecorder>.fromOpaque(pointer).takeUnretainedValue().ids.append(String(cString: id))
    }
    defer { toolbar.detach() }
    func update(preview: Bool, enabled: Bool = true, extra: Bool = false) throws {
        var items: [[String: Any]] = [
            ["id": "reader-preview", "label": "Preview", "symbol": "eye", "selection": preview],
            ["id": "reader-source", "label": "Source", "symbol": "chevron.left.forwardslash.chevron.right", "selection": !preview, "enabled": enabled],
            ["id": "word-wrap", "label": "Wrap", "selection": true],
            ["id": "find-in-file", "label": "Find"],
        ]
        if extra { items.append(["id": "hide-reader-toolbar", "label": "Hide"]) }
        toolbar.update(parent: parent, rect: NSRect(x: 130, y: 6, width: 700, height: 540), data: try JSONSerialization.data(withJSONObject: items), visible: true, collapsed: false)
    }
    func button(_ id: String) -> ReaderActionButton {
        func find(_ view: NSView) -> ReaderActionButton? {
            if let button = view as? ReaderActionButton, button.identifier?.rawValue == id { return button }
            return view.subviews.lazy.compactMap { find($0) }.first
        }
        return find(toolbar)!
    }
    try update(preview: true)
    let preview = button("reader-preview"), source = button("reader-source"), wrap = button("word-wrap")
    let point = source.convert(NSPoint(x: source.bounds.midX, y: source.bounds.midY), to: parent)
    let hit = parent.hitTest(point)
    precondition(hit === source, "A physical click must hit the visible source button")
    let rootPoint = source.convert(NSPoint(x: source.bounds.midX, y: source.bounds.midY), to: window.contentView!.superview)
    precondition(window.contentView!.hitTest(rootPoint) === source)
    precondition(toolbar.hitTest(NSPoint(x: 150, y: 400)) == nil, "Transparent space belongs to the document")
    // Glass can install a decorative child above its content. It must never
    // become the target of a click on a live toolbar control.
    func capsule(_ view: NSView) -> FloatingReaderToolbar? {
        if let value = view as? FloatingReaderToolbar { return value }
        return view.subviews.lazy.compactMap { capsule($0) }.first
    }
    let material = capsule(toolbar)!.subviews.first!
    let decoration = NSView(frame: material.bounds)
    material.addSubview(decoration, positioned: .above, relativeTo: nil)
    precondition(window.contentView!.hitTest(rootPoint) === source)
    decoration.removeFromSuperview()
    let inactive = WorkspaceReaderToolbar(context: Unmanaged.passUnretained(recorder).toOpaque()) { _, _ in
        preconditionFailure("An inactive workspace must not receive mouse input")
    }
    inactive.update(parent: parent, rect: parent.bounds, data: Data("[]".utf8), visible: false, collapsed: false)
    precondition(window.contentView!.hitTest(rootPoint) === source)
    inactive.detach()
    let menuControls = ReaderControls([ReaderControl("reader-preview", preview), ReaderControl("reader-source", source)])
    for _ in 0..<4 {
        source.performClick(nil)
        precondition(recorder.ids.last == "reader-source")
        precondition(source.state == .off && preview.state == .on, "Dispatch must wait for the document's mode")
        try update(preview: false)
        precondition(source === button("reader-source") && preview === button("reader-preview"))
        precondition(source.state == .on && preview.state == .off)
        precondition(menuControls.menu().items.map { $0.state } == [.off, .on])
        source.performClick(nil)
        precondition(source.state == .on, "Clicking the active mode cannot deselect it")
        preview.performClick(nil)
        try update(preview: true)
        precondition(preview.state == .on && source.state == .off && wrap.state == .on)
    }
    try update(preview: true, enabled: false, extra: true)
    precondition(source === button("reader-source"), "Changing capabilities must retain surviving controls")
    let count = recorder.ids.count
    source.performClick(nil)
    precondition(recorder.ids.count == count, "Disabled actions cannot execute")
    let find = button("find-in-file")
    find.performClick(nil)
    precondition(find.state == .off && find.selection == nil)
    precondition(ReaderControls([ReaderControl("find-in-file", find)]).menu().items[0].state == .off)
    print("Reader action state: passed")
}
