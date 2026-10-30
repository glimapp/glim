import Cocoa
import FinderSync
import os

private final class FinderSetup: NSObject {
    static let shared = FinderSetup()
    private var text: [String] = []
    private var observing = false
    private let preference = "GlimFinderSetupPresented.v1"
    private let logger = Logger(subsystem: "io.github.jony4.glimpse", category: "finder-setup")

    func offer(_ text: [String]) {
        logger.info("Finder setup: enabled=\(FIFinderSyncController.isExtensionEnabled), alreadyPresented=\(UserDefaults.standard.bool(forKey: self.preference))")
        guard let plugin = Bundle.main.builtInPlugInsURL?.appendingPathComponent("GlimFinder.appex"),
              (try? plugin.checkResourceIsReachable()) == true,
              !Bundle.main.bundlePath.hasPrefix("/Volumes/"),
              !Bundle.main.bundlePath.contains("/AppTranslocation/"),
              !UserDefaults.standard.bool(forKey: preference),
              !FIFinderSyncController.isExtensionEnabled else { return }
        self.text = text
        logger.info("Finder setup waiting for an active window")
        if !observing {
            observing = true
            NotificationCenter.default.addObserver(self, selector: #selector(presentIfReady),
                name: NSApplication.didBecomeActiveNotification, object: nil)
            NotificationCenter.default.addObserver(self, selector: #selector(presentIfReady),
                name: NSWindow.didBecomeMainNotification, object: nil)
            NotificationCenter.default.addObserver(self, selector: #selector(presentIfReady),
                name: NSWindow.didBecomeKeyNotification, object: nil)
        }
        DispatchQueue.main.async { self.presentIfReady() }
    }

    @objc private func presentIfReady() {
        guard !text.isEmpty, NSApp.isActive,
              let window = NSApp.mainWindow ?? NSApp.keyWindow ?? NSApp.orderedWindows.first(where: {
                  $0.isVisible && $0.canBecomeKey && $0.parent == nil
              }), window.attachedSheet == nil else { return }
        NotificationCenter.default.removeObserver(self)
        observing = false
        guard !FIFinderSyncController.isExtensionEnabled,
              !UserDefaults.standard.bool(forKey: preference) else { text = []; return }
        let alert = NSAlert()
        alert.alertStyle = .informational
        alert.messageText = text[0]
        alert.informativeText = text[1]
        alert.addButton(withTitle: text[2])
        alert.addButton(withTitle: text[3])
        text = []
        UserDefaults.standard.set(true, forKey: preference)
        logger.info("Presenting Finder setup")
        alert.beginSheetModal(for: window) { response in
            if response == .alertFirstButtonReturn {
                FIFinderSyncController.showExtensionManagementInterface()
            }
        }
    }
}

@_cdecl("glim_finder_offer_setup")
func glimFinderOfferSetup(_ title: UnsafePointer<CChar>, _ detail: UnsafePointer<CChar>,
                         _ settings: UnsafePointer<CChar>, _ later: UnsafePointer<CChar>) {
    precondition(Thread.isMainThread)
    FinderSetup.shared.offer([title, detail, settings, later].map { String(cString: $0) })
}

@_cdecl("glim_finder_manage")
func glimFinderManage() {
    precondition(Thread.isMainThread)
    FIFinderSyncController.showExtensionManagementInterface()
}
