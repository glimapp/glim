import Cocoa
import WebKit
import PDFKit

private final class ReaderFindPanel: NSView {
    var inputFocused = false { didSet { needsDisplay = true } }
    override var wantsUpdateLayer: Bool { true }
    override func updateLayer() {
        layer?.borderColor = (inputFocused ? NSColor.keyboardFocusIndicatorColor.withAlphaComponent(0.5) : NSColor.separatorColor).cgColor
        layer?.backgroundColor = NSColor.controlBackgroundColor.cgColor
    }
}

// Each native reader owns its search surface; it never changes document layout.
final class ReaderFindBar: NSObject, NSTextFieldDelegate {
    private let panel = ReaderFindPanel()
    private let field = NSTextField()
    private let caseButton = NSButton()
    private let count = NSTextField(labelWithString: "")
    private let previous = NSButton()
    private let next = NSButton()
    private let dismiss = NSButton()
    private weak var host: NSView?
    private weak var content: NSView?
    private var monitor: Any?
    private var pending: DispatchWorkItem?
    private var generation = 0
    private let search: (String, Bool, Bool, Bool, @escaping (String, Bool) -> Void) -> Void
    private let cancel: () -> Void
    private let telemetryKind: Int32
    let button = ReaderActionButton()
    var isOpen: Bool { !panel.isHidden }

    init(host: NSView, content: NSView, telemetryKind: Int32,
         search: @escaping (String, Bool, Bool, Bool, @escaping (String, Bool) -> Void) -> Void,
         cancel: @escaping () -> Void = {}) {
        self.host = host; self.content = content; self.search = search; self.cancel = cancel
        self.telemetryKind = telemetryKind
        super.init()
        panel.wantsLayer = true
        panel.layer?.cornerRadius = 6; panel.layer?.borderWidth = 1
        panel.layer?.borderColor = NSColor.separatorColor.cgColor
        panel.layer?.backgroundColor = NSColor.controlBackgroundColor.cgColor
        panel.layer?.shadowOpacity = 0.08; panel.layer?.shadowRadius = 3
        panel.layer?.shadowOffset = NSSize(width: 0, height: -2)
        panel.translatesAutoresizingMaskIntoConstraints = false
        panel.isHidden = true; host.addSubview(panel)
        field.placeholderString = L("Find"); field.setAccessibilityLabel(L("Find"))
        field.font = .systemFont(ofSize: 13); field.delegate = self
        field.isBordered = false; field.drawsBackground = false; field.focusRingType = .none
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        configure(caseButton, label: L("Match case"), symbol: "textformat", action: #selector(toggleCase))
        caseButton.setButtonType(.toggle)
        configure(previous, label: L("Previous match · Shift+Enter"), symbol: "arrow.up", action: #selector(previousMatch))
        configure(next, label: L("Next match · Enter"), symbol: "arrow.down", action: #selector(nextMatch))
        configure(dismiss, label: L("Close find · Esc"), symbol: "xmark", action: #selector(dismissFind))
        configure(button, label: L("Find in file · ⌘F"), symbol: "magnifyingglass", action: #selector(show))
        count.font = .monospacedDigitSystemFont(ofSize: 12, weight: .regular)
        count.alignment = .center; count.textColor = .secondaryLabelColor
        count.widthAnchor.constraint(equalToConstant: 68).isActive = true
        let row = NSStackView(views: [field, caseButton, count, previous, next, dismiss])
        row.spacing = 4; row.translatesAutoresizingMaskIntoConstraints = false
        panel.addSubview(row)
        let width = panel.widthAnchor.constraint(equalToConstant: 420); width.priority = .defaultHigh
        NSLayoutConstraint.activate([
            width, panel.trailingAnchor.constraint(equalTo: host.trailingAnchor, constant: -12),
            panel.leadingAnchor.constraint(greaterThanOrEqualTo: host.leadingAnchor, constant: 12),
            panel.topAnchor.constraint(equalTo: host.topAnchor, constant: 12),
            row.leadingAnchor.constraint(equalTo: panel.leadingAnchor, constant: 6),
            row.trailingAnchor.constraint(equalTo: panel.trailingAnchor, constant: -6),
            row.topAnchor.constraint(equalTo: panel.topAnchor, constant: 6),
            row.bottomAnchor.constraint(equalTo: panel.bottomAnchor, constant: -6),
            field.heightAnchor.constraint(equalToConstant: 20)
        ])
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self = self, let host = self.host, !host.isHiddenOrHasHiddenAncestor,
                  event.window === host.window,
                  let responder = event.window?.firstResponder as? NSView,
                  responder === host || responder.isDescendant(of: host) else { return event }
            let flags = event.modifierFlags.intersection([.command, .control, .option, .shift])
            let key = event.charactersIgnoringModifiers?.lowercased()
            if flags == .command && key == "f" {
                ReaderTelemetry.shortcut(self.telemetryKind, 37, event: event)
                ReaderTelemetry.withSource(2) { self.show() }; return nil
            }
            if !self.panel.isHidden {
                if event.keyCode == 53 {
                    ReaderTelemetry.shortcut(self.telemetryKind, 45, event: event)
                    ReaderTelemetry.withSource(2) { self.dismissFind() }; return nil
                }
                if key == "g" && (flags == .command || flags == [.command, .shift]) {
                    ReaderTelemetry.shortcut(self.telemetryKind, flags.contains(.shift) ? 44 : 43, event: event)
                    ReaderTelemetry.withSource(2) {
                        ReaderTelemetry.record(self.telemetryKind, flags.contains(.shift) ? 44 : 43)
                        self.run(backwards: flags.contains(.shift), changed: false)
                    }; return nil
                }
            }
            return event
        }
    }

    private func configure(_ button: NSButton, label: String, symbol: String, action: Selector) {
        button.title = ""; button.toolTip = label; button.setAccessibilityLabel(label)
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: label)?
            .withSymbolConfiguration(.init(pointSize: 16, weight: .regular))
        button.imagePosition = .imageOnly; button.bezelStyle = .accessoryBarAction
        button.isBordered = true; button.showsBorderOnlyWhileMouseInside = true
        button.target = self; button.action = action
        button.widthAnchor.constraint(equalToConstant: 28).isActive = true
        button.heightAnchor.constraint(equalToConstant: 28).isActive = true
    }

    @objc func show() {
        guard content?.isHiddenOrHasHiddenAncestor == false else { return }
        ReaderTelemetry.record(telemetryKind, 37)
        panel.isHidden = false
        host?.addSubview(panel, positioned: .above, relativeTo: nil)
        field.selectText(nil)
        run(backwards: false, changed: true)
    }
    @objc func hide() {
        let window = host?.window
        let restoreContentFocus: Bool
        if !panel.isHiddenOrHasHiddenAncestor, let responder = window?.firstResponder {
            restoreContentFocus = responder === field.currentEditor()
                || (responder as? NSView)?.isDescendant(of: panel) == true
        } else {
            restoreContentFocus = false
        }
        generation += 1; pending?.cancel(); pending = nil; cancel()
        panel.isHidden = true
        // Background resets must not move focus out of another reader or settings.
        if restoreContentFocus { window?.makeFirstResponder(content) }
    }
    @objc private func dismissFind() { ReaderTelemetry.record(telemetryKind, 45); hide() }
    @objc private func toggleCase() { ReaderTelemetry.record(telemetryKind, 46, value: caseButton.state == .on ? 1 : 0); run(backwards: false, changed: true) }
    @objc private func previousMatch() { ReaderTelemetry.record(telemetryKind, 44); run(backwards: true, changed: pending != nil) }
    @objc private func nextMatch() { ReaderTelemetry.record(telemetryKind, 43); run(backwards: false, changed: pending != nil) }
    func controlTextDidBeginEditing(_ notification: Notification) { panel.inputFocused = true }
    func controlTextDidEndEditing(_ notification: Notification) { panel.inputFocused = false }
    func controlTextDidChange(_ notification: Notification) {
        generation += 1; pending?.cancel(); cancel()
        previous.isEnabled = false; next.isEnabled = false; count.stringValue = "…"
        let work = DispatchWorkItem { [weak self] in self?.run(backwards: false, changed: true) }
        pending = work; DispatchQueue.main.asyncAfter(deadline: .now() + 0.12, execute: work)
    }
    func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        if selector == #selector(NSResponder.insertNewline(_:)) {
            if let event = NSApp.currentEvent, event.type == .keyDown {
                let action: Int32 = event.modifierFlags.contains(.shift) ? 44 : 43
                ReaderTelemetry.shortcut(telemetryKind, action, event: event)
                ReaderTelemetry.record(telemetryKind, action)
            }
            run(backwards: NSApp.currentEvent?.modifierFlags.contains(.shift) == true, changed: pending != nil)
            return true
        }
        if selector == #selector(NSResponder.cancelOperation(_:)) { hide(); return true }
        return false
    }
    private func run(backwards: Bool, changed: Bool) {
        let changed = changed || pending != nil
        pending?.cancel(); pending = nil; generation += 1
        let current = generation
        let query = field.stringValue
        if query.isEmpty {
            cancel(); count.stringValue = ""; previous.isEnabled = false; next.isEnabled = false; return
        }
        count.stringValue = "…"; count.textColor = .secondaryLabelColor
        previous.isEnabled = false; next.isEnabled = false
        search(query, caseButton.state == .on, backwards, changed) { [weak self] label, found in
            guard let self = self, self.generation == current, !self.panel.isHidden else { return }
            self.count.stringValue = label
            self.count.textColor = label == L("No matches") ? .systemRed : .secondaryLabelColor
            self.previous.isEnabled = found; self.next.isEnabled = found
        }
    }
    // Reset document-specific search without destroying the live keyboard monitor.
    func prepareForDocumentReplacement() {
        hide()
        count.stringValue = ""; previous.isEnabled = false; next.isEnabled = false
    }
    func close() {
        generation += 1; pending?.cancel(); pending = nil; cancel()
        if let monitor = monitor { NSEvent.removeMonitor(monitor); self.monitor = nil }
    }
    deinit { if let monitor = monitor { NSEvent.removeMonitor(monitor) } }

    static func web(host: NSView, web: WKWebView, telemetryKind: Int32 = 7) -> ReaderFindBar {
        ReaderFindBar(host: host, content: web, telemetryKind: telemetryKind, search: { [weak web] query, sensitive, backwards, _, completion in
            guard let web = web else { return }
            let config = WKFindConfiguration()
            config.caseSensitive = sensitive; config.backwards = backwards; config.wraps = true
            web.find(query, configuration: config) { result in
                // WebKit exposes match presence, not a total. Do not invent a count.
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
