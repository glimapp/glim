import Cocoa
import WebKit
import PDFKit

// The shared GPUI FindBar owns all search controls. Native readers retain only
// backend state and route their toolbar/keyboard requests to that component.
final class ReaderFindSession: NSObject {
    private weak var host: NSView?
    private weak var content: NSView?
    private var monitor: Any?
    private var generation = 0
    private var revision = 0
    private var polledRevision = -1
    private var command = 0
    private var label = ""
    private var found = false
    private var searching = false
    private let search: (String, Bool, Bool, Bool, @escaping (String, Bool) -> Void) -> Void
    private let cancel: () -> Void
    private let telemetryKind: Int32
    let button = ReaderActionButton()
    private(set) var isOpen = false

    init(host: NSView, content: NSView, telemetryKind: Int32,
         search: @escaping (String, Bool, Bool, Bool, @escaping (String, Bool) -> Void) -> Void,
         cancel: @escaping () -> Void = {}) {
        self.host = host; self.content = content; self.search = search; self.cancel = cancel
        self.telemetryKind = telemetryKind
        super.init()
        let title = L("Find in file · ⌘F")
        button.title = title; button.toolTip = title; button.setAccessibilityLabel(title)
        button.image = NSImage(systemSymbolName: "magnifyingglass", accessibilityDescription: title)
        button.imagePosition = .imageOnly; button.isBordered = false; button.selection = false
        button.target = self; button.action = #selector(toggle)
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self = self, let host = self.host, !host.isHiddenOrHasHiddenAncestor,
                  event.window === host.window,
                  let responder = event.window?.firstResponder as? NSView,
                  responder === host || responder.isDescendant(of: host) else { return event }
            let flags = event.modifierFlags.intersection([.command, .control, .option, .shift])
            let key = event.charactersIgnoringModifiers?.lowercased()
            if flags == .command && key == "f" {
                ReaderTelemetry.shortcut(self.telemetryKind, 37, event: event)
                self.show(); return nil
            }
            if self.isOpen && event.keyCode == 53 { self.requestClose(); return nil }
            if self.isOpen && key == "g" && (flags == .command || flags == [.command, .shift]) {
                self.command = flags.contains(.shift) ? 4 : 3
                self.revision += 1; return nil
            }
            return event
        }
    }

    @objc func toggle() { if isOpen { requestClose() } else { show() } }
    func show() {
        guard content?.isHiddenOrHasHiddenAncestor == false else { return }
        applyOpen(true); command = 1; revision += 1
        ReaderTelemetry.record(telemetryKind, 37)
    }
    private func requestClose() {
        hide(); command = 2; revision += 1
        ReaderTelemetry.record(telemetryKind, 45)
    }
    func applyOpen(_ open: Bool) {
        isOpen = open; button.selection = open; revision += 1
        if !open { command = 0; resetSearch() }
    }
    func hide() { applyOpen(false) }
    func focusContent() { host?.window?.makeFirstResponder(content) }
    func run(_ query: String, sensitive: Bool, backwards: Bool, changed: Bool) {
        generation += 1; cancel()
        let current = generation
        label = query.isEmpty ? "" : "…"; found = false; searching = !query.isEmpty; revision += 1
        guard isOpen, !query.isEmpty else { return }
        search(query, sensitive, backwards, changed) { [weak self] label, found in
            guard let self = self, self.generation == current, self.isOpen else { return }
            self.label = label; self.found = found
            self.searching = label == "…" || label.hasSuffix("+")
            self.revision += 1
        }
    }
    private func resetSearch() {
        generation += 1; cancel(); label = ""; found = false; searching = false
    }
    func takeEvent() -> [String: Any]? {
        guard polledRevision != revision else { return nil }
        polledRevision = revision
        let event: [String: Any] = ["command": command, "open": isOpen, "label": label,
                                    "found": found, "searching": searching]
        command = 0
        return event
    }
    func prepareForDocumentReplacement() { hide(); command = 0; revision += 1 }
    func close() {
        hide()
        if let monitor = monitor { NSEvent.removeMonitor(monitor); self.monitor = nil }
    }
    deinit { if let monitor = monitor { NSEvent.removeMonitor(monitor) } }

    static func web(host: NSView, web: WKWebView, telemetryKind: Int32 = 7) -> ReaderFindSession {
        ReaderFindSession(host: host, content: web, telemetryKind: telemetryKind, search: { [weak web] query, sensitive, backwards, _, completion in
            guard let web = web else { return }
            let config = WKFindConfiguration()
            config.caseSensitive = sensitive; config.backwards = backwards; config.wraps = true
            web.find(query, configuration: config) { result in
                // WebKit exposes match presence, not a total.
                completion(result.matchFound ? "" : L("No matches"), result.matchFound)
            }
        }, cancel: { [weak web] in web?.find("", configuration: WKFindConfiguration(), completionHandler: { _ in }) })
    }
}

// Keep only the active selection. Navigating rescans asynchronously instead of
// retaining a PDFSelection (and its page graph) for every match in a large PDF.
final class PDFReaderFind: NSObject, PDFDocumentDelegate {
    private weak var view: PDFView?
    private weak var document: PDFDocument?
    private var total = 0
    private var seen = 0
    private var index = 0
    private var selected: PDFSelection?
    private var completion: ((String, Bool) -> Void)?
    private var backwards = false
    private var searching = false
    init(view: PDFView) {
        self.view = view; self.document = view.document
        super.init(); document?.delegate = self
    }
    func rebindDocument() {
        cancel()
        document?.delegate = nil
        document = view?.document
        document?.delegate = self
        total = 0; seen = 0; index = 0
    }
    func search(_ query: String, sensitive: Bool, backwards: Bool, changed: Bool, completion: @escaping (String, Bool) -> Void) {
        let target = changed || total == 0 ? 0 : (index + (backwards ? total - 1 : 1)) % total
        cancel(); index = target; seen = 0; selected = nil
        self.backwards = changed && backwards
        self.completion = completion; searching = true
        document?.beginFindString(query, withOptions: sensitive ? [] : [.caseInsensitive])
    }
    func didMatchString(_ instance: PDFSelection) {
        guard searching else { return }
        if seen == index || backwards { selected = instance }
        if seen == index && !backwards { reveal() }
        seen += 1
        report()
    }
    func documentDidEndDocumentFind(_ notification: Notification) {
        guard searching else { return }
        searching = false; total = seen
        if backwards && total > 0 { index = total - 1; reveal() }
        report()
    }
    private func reveal() {
        guard let selection = selected else { return }
        selection.color = .selectedTextBackgroundColor
        view?.setCurrentSelection(selection, animate: false)
        view?.scrollSelectionToVisible(nil)
    }
    private func report() {
        completion?(seen == 0 ? (searching ? "…" : L("No matches")) : "\(min(index + 1, seen))/\(seen)\(searching ? "+" : "")", !searching && seen > 0)
    }
    func cancel() {
        searching = false; completion = nil; document?.cancelFindString()
        view?.clearSelection(); selected = nil
    }
}
