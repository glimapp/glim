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
