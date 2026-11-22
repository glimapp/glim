import AppKit
import PDFKit
import WebKit

// Runs in GlimRenderer with access to the prepared document directory only.
private final class MarkdownPDFPrint: NSObject, WKNavigationDelegate {
    private let web: WKWebView
    private let printInfo = NSPrintInfo()
    private let source: URL
    private var outcome: Int32?

    init(source: URL, destination: URL) {
        self.source = source
        printInfo.paperSize = NSSize(width: 595.28, height: 841.89)
        printInfo.topMargin = 42
        printInfo.bottomMargin = 42
        printInfo.leftMargin = 42
        printInfo.rightMargin = 42
        printInfo.isHorizontallyCentered = false
        printInfo.isVerticallyCentered = false
        printInfo.horizontalPagination = .fit
        printInfo.verticalPagination = .automatic
        printInfo.jobDisposition = .save
        printInfo.dictionary()[NSPrintInfo.AttributeKey.jobSavingURL] = destination

        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false
        web = WKWebView(frame: NSRect(
            x: 0, y: 0,
            width: printInfo.paperSize.width - printInfo.leftMargin - printInfo.rightMargin,
            height: printInfo.paperSize.height - printInfo.topMargin - printInfo.bottomMargin
        ), configuration: configuration)
        super.init()
        web.navigationDelegate = self
    }

    func run(destination: URL) -> Int32 {
        _ = host
        web.loadFileURL(source, allowingReadAccessTo: source.deletingLastPathComponent())
        while outcome == nil {
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.05))
        }
        if outcome == 0, let document = PDFDocument(url: destination), document.pageCount > 0 {
            return 0
        }
        return 1
    }

    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
                 decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        decisionHandler(action.request.url == source ? .allow : .cancel)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        // Run printing after WebKit returns from its navigation callback.
        DispatchQueue.main.async { [self] in
            let operation = webView.printOperation(with: printInfo)
            operation.jobTitle = webView.title ?? source.deletingPathExtension().lastPathComponent
            operation.showsPrintPanel = false
            operation.showsProgressPanel = false
            // WebKit prepares the page range asynchronously for AppKit's print thread.
            operation.canSpawnSeparateThread = true
            operation.runModal(for: host, delegate: self,
                               didRun: #selector(printFinished(_:success:context:)), contextInfo: nil)
        }
    }

    private lazy var host: NSWindow = {
        let window = NSWindow(contentRect: web.frame, styleMask: .borderless, backing: .buffered, defer: false)
        window.contentView = web
        return window
    }()

    @objc private func printFinished(
        _ operation: NSPrintOperation, success: Bool, context: UnsafeMutableRawPointer?
    ) {
        outcome = success ? 0 : 1
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        outcome = 1
    }

    func webView(
        _ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error
    ) {
        outcome = 1
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        outcome = 1
    }
}

@_cdecl("glim_print_markdown_pdf")
public func glimPrintMarkdownPDF(_ input: UnsafePointer<CChar>, _ output: UnsafePointer<CChar>) -> Int32 {
    let source = URL(fileURLWithPath: String(cString: input))
    let destination = URL(fileURLWithPath: String(cString: output))
    _ = NSApplication.shared.setActivationPolicy(.prohibited)
    return MarkdownPDFPrint(source: source, destination: destination).run(destination: destination)
}

@_cdecl("glim_open_markdown_in_browser")
public func glimOpenMarkdownInBrowser(
    _ path: UnsafePointer<CChar>,
    _ context: UnsafeMutableRawPointer?,
    _ complete: @escaping @convention(c) (UnsafeMutableRawPointer?, Int32) -> Void
) {
    let workspace = NSWorkspace.shared
    guard let browser = workspace.urlForApplication(toOpen: URL(string: "https://localhost")!) else {
        complete(context, 1)
        return
    }
    let file = URL(fileURLWithPath: String(cString: path))
    workspace.open([file], withApplicationAt: browser, configuration: .init()) { app, error in
        complete(context, error == nil && app != nil ? 0 : 2)
    }
}

private final class DocumentPrintSession: NSObject {
    let finished: () -> Void
    init(finished: @escaping () -> Void) { self.finished = finished }

    @objc func didPrint(_ operation: NSPrintOperation, success: Bool, context: UnsafeMutableRawPointer?) {
        finished()
        Unmanaged<DocumentPrintSession>.fromOpaque(context!).release()
    }
}

@_cdecl("glim_show_print_panel")
func showDocumentPrintPanel(_ parent: UnsafeRawPointer, _ kind: Int32, _ bytes: UnsafePointer<UInt8>,
                            _ length: Int, _ title: UnsafePointer<CChar>, _ context: UnsafeMutableRawPointer,
                            _ release: @escaping @convention(c) (UnsafeMutableRawPointer) -> Void) -> Bool {
    let parentView = Unmanaged<NSView>.fromOpaque(parent).takeUnretainedValue()
    guard let window = parentView.window, window.attachedSheet == nil else { return false }
    let info = NSPrintInfo.shared.copy() as! NSPrintInfo
    info.horizontalPagination = .fit
    info.verticalPagination = .automatic
    let operation: NSPrintOperation?
    func pdfOperation(_ document: PDFDocument?) -> NSPrintOperation? {
        guard let document = document, document.allowsPrinting else { return nil }
        return document.printOperation(for: info, scalingMode: .pageScaleToFit, autoRotate: true)
    }
    switch kind {
    case 0:
        // Search only the visible reader in the requesting workspace window.
        func previewOperation(_ view: NSView, inReader: Bool) -> NSPrintOperation? {
            guard !view.isHidden else { return nil }
            if inReader, let pdf = view as? PDFView { return pdfOperation(pdf.document) }
            if inReader, let web = view as? WKWebView { return web.printOperation(with: info) }
            for child in view.subviews.reversed() {
                if let op = previewOperation(child, inReader: inReader || view is NativePreviewHost) { return op }
            }
            return nil
        }
        operation = window.contentView.flatMap { previewOperation($0, inReader: false) }
    case 1, 3:
        let url = Array(UnsafeBufferPointer(start: bytes, count: length)) + [0]
        let source = url.withUnsafeBytes { URL(fileURLWithFileSystemRepresentation: $0.bindMemory(to: CChar.self).baseAddress!, isDirectory: false, relativeTo: nil) }
        if kind == 1 {
            operation = pdfOperation(PDFDocument(url: source))
        } else {
            guard let image = NSImage(contentsOf: source), let page = PDFPage(image: image) else { return false }
            let document = PDFDocument()
            document.insert(page, at: 0)
            operation = pdfOperation(document)
        }
    case 2:
        let width = max(1, info.paperSize.width - info.leftMargin - info.rightMargin)
        let text = NSTextView(frame: NSRect(x: 0, y: 0, width: width, height: 1))
        text.isVerticallyResizable = true
        text.textContainer?.widthTracksTextView = true
        text.font = .monospacedSystemFont(ofSize: 10, weight: .regular)
        text.textColor = .black
        text.drawsBackground = false
        text.string = String(decoding: UnsafeBufferPointer(start: bytes, count: length), as: UTF8.self)
        text.sizeToFit()
        operation = NSPrintOperation(view: text, printInfo: info)
    default: return false
    }
    guard let operation = operation else { return false }
    operation.jobTitle = String(cString: title)
    operation.canSpawnSeparateThread = true
    let session = DocumentPrintSession { release(context) }
    DispatchQueue.main.async {
        let retained = Unmanaged.passRetained(session).toOpaque()
        operation.runModal(for: window, delegate: session,
                           didRun: #selector(DocumentPrintSession.didPrint(_:success:context:)), contextInfo: retained)
    }
    return true
}
