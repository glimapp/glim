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
    let parent = NSView(frame: NSRect(x: 0, y: 0, width: 900, height: 600))
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
        toolbar.update(parent: parent, rect: parent.bounds, data: try JSONSerialization.data(withJSONObject: items), visible: true, collapsed: false)
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
    let menuControls = ReaderControls([ReaderControl("preview", preview), ReaderControl("source", source)])
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
    precondition(ReaderControls([ReaderControl("find", find)]).menu().items[0].state == .off)
    print("Reader action state: passed")
}
