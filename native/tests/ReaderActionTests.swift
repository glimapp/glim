import Cocoa
import PDFKit

private final class PDFControlsFixture: NSView {
    override func draw(_ dirtyRect: NSRect) {
        NSColor.white.setFill(); bounds.fill()
        ("Selectable text" as NSString).draw(at: NSPoint(x: 120, y: 160), withAttributes: [.font: NSFont.systemFont(ofSize: 24)])
    }
}

private final class PDFControlsWindow: NSWindow {
    var pointer = NSPoint.zero
    override var mouseLocationOutsideOfEventStream: NSPoint { pointer }
}

func readerPDFControls() {
    _ = NSApplication.shared
    let originalCursor = NSCursor.current
    defer { originalCursor.set() }
    for title in ["Previous", "Next", "Zoom out", "Zoom in", "Fit page", "Fit width", "Actual size",
                  "Play Slideshow", "Copy Preview Text", "Extract Text", "Find in file · ⌘F"] {
        guard let symbol = ReaderMenu.toolbarSymbol(for: L(title)) else { preconditionFailure("Missing icon for \(title)") }
        precondition(NSImage(systemSymbolName: symbol, accessibilityDescription: nil) != nil, "Unavailable icon: \(symbol)")
    }
    precondition(ReaderMenu.toolbarSymbol(for: L("Fit page")) != "arrow.up.left.and.arrow.down.right", "Page fitting must be distinct from entering fullscreen")

    let fixture = PDFControlsFixture(frame: NSRect(x: 0, y: 0, width: 600, height: 800))
    let document = PDFDocument(data: fixture.dataWithPDF(inside: fixture.bounds))!
    let page = document.page(at: 0)!
    precondition(page.numberOfCharacters > 0)
    let link = PDFAnnotation(bounds: NSRect(x: 350, y: 350, width: 90, height: 30), forType: .link, withProperties: nil)
    link.url = URL(string: "https://example.com")!
    page.addAnnotation(link)
    let field = PDFAnnotation(bounds: NSRect(x: 350, y: 450, width: 90, height: 30), forType: .widget, withProperties: nil)
    field.widgetFieldType = .text
    page.addAnnotation(field)

    let window = PDFControlsWindow(contentRect: NSRect(x: 0, y: 0, width: 700, height: 500), styleMask: [.titled], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    let pdf = ReaderPDFView(frame: window.contentView!.bounds)
    window.contentView!.addSubview(pdf)
    pdf.document = document; pdf.displayMode = .singlePage
    pdf.fitPage(); pdf.layoutSubtreeIfNeeded()
    func settle() { pdf.layoutSubtreeIfNeeded(); RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.05)) }
    settle()
    let clip = pdf.documentView!.enclosingScrollView!.contentView
    func show(_ point: NSPoint) -> NSPoint {
        let target = clip.convert(pdf.convert(point, from: page), from: pdf)
        var proposed = clip.bounds
        proposed.origin = NSPoint(x: target.x - proposed.width / 2, y: target.y - proposed.height / 2)
        clip.scroll(to: clip.constrainBoundsRect(proposed).origin)
        return pdf.convert(point, from: page)
    }
    func hit(_ point: NSPoint) -> NSView? { pdf.hitTest(pdf.superview!.convert(point, from: pdf)) }
    let blank = NSPoint(x: 250, y: 400)
    precondition(hit(pdf.convert(blank, from: page)) !== pdf, "A fitted single page must not capture panning")
    pdf.zoomIn(nil); pdf.scaleFactor = 2; settle()
    let point = show(blank)
    precondition(hit(point) === pdf, "Zoomed blank areas must capture panning")
    window.pointer = pdf.convert(point, to: nil)
    pdf.setCursorFor(pdf.areaOfInterest(for: point))
    precondition(NSCursor.current == .openHand, "Pannable blank areas need an open hand")
    let before = clip.bounds.origin
    func event(_ type: NSEvent.EventType, _ point: NSPoint) -> NSEvent {
        NSEvent.mouseEvent(with: type, location: pdf.convert(point, to: nil), modifierFlags: [], timestamp: 0,
                           windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
    }
    pdf.mouseDown(with: event(.leftMouseDown, point))
    precondition(NSCursor.current == .closedHand, "Dragging needs a closed hand")
    let dragged = NSPoint(x: point.x + 45, y: point.y - 35)
    let startInClip = clip.convert(point, from: pdf), endInClip = clip.convert(dragged, from: pdf)
    pdf.mouseDragged(with: event(.leftMouseDragged, dragged))
    pdf.mouseUp(with: event(.leftMouseUp, dragged))
    pdf.setCursorFor(pdf.areaOfInterest(for: point))
    precondition(NSCursor.current == .openHand, "Releasing must leave the dragging cursor")
    precondition(abs(clip.bounds.origin.x - before.x + endInClip.x - startInClip.x) < 1 &&
                 abs(clip.bounds.origin.y - before.y + endInClip.y - startInClip.y) < 1,
                 "Panning must follow the pointer at viewport speed on both axes: \(before) → \(clip.bounds.origin)")
    let character = page.characterBounds(at: 0)
    let textPoint = show(NSPoint(x: character.midX, y: character.midY))
    precondition(hit(textPoint) !== pdf, "Selectable text must remain with PDFKit")
    window.pointer = pdf.convert(textPoint, to: nil)
    pdf.setCursorFor(pdf.areaOfInterest(for: textPoint))
    precondition(NSCursor.current == .iBeam, "Selectable text needs the text cursor")
    for annotation in [link, field] {
        let rect = annotation.bounds
        precondition(hit(show(NSPoint(x: rect.midX, y: rect.midY))) !== pdf, "Links and forms must remain interactive")
    }
    let control = NSButton(frame: NSRect(x: point.x - 10, y: point.y - 10, width: 30, height: 30))
    pdf.addSubview(control)
    precondition(hit(point) === control, "Reader controls must not start a page drag")
    control.removeFromSuperview()
    page.rotation = 90; pdf.scaleFactor = 2; settle()
    precondition(hit(show(blank)) === pdf, "Rotated pages must still support panning")
    let image = NSImage(size: NSSize(width: 600, height: 800), flipped: false) { rect in NSColor.white.setFill(); rect.fill(); return true }
    let scanned = PDFDocument(); scanned.insert(PDFPage(image: image)!, at: 0)
    pdf.document = scanned; pdf.scaleFactor = 2; settle()
    precondition(hit(NSPoint(x: pdf.bounds.midX, y: pdf.bounds.midY)) === pdf, "Scanned pages must support panning")
    pdf.removeFromSuperview(); window.close()
    print("PDF controls, text/link/form hit testing, scanned/rotated pages and panning: passed")
}

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
    var hiddenTransitions = 0
    let observation = source.superview!.observe(\.isHidden, options: [.new]) { _, change in
        if change.newValue == true { hiddenTransitions += 1 }
    }
    let buttonFrame = source.convert(source.bounds, to: parent)
    for index in 0..<8 {
        try update(preview: index.isMultiple(of: 2))
        capsule(toolbar)!.needsLayout = true
        toolbar.layoutSubtreeIfNeeded()
        precondition(source.convert(source.bounds, to: parent) == buttonFrame)
    }
    try update(preview: true)
    precondition(hiddenTransitions == 0, "State refresh must not hide a control during mouse tracking")
    observation.invalidate()
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
