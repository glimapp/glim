import Cocoa
import FinderSync
import os

@objc(GlimFinderSync)
final class GlimFinderSync: FIFinderSync {
    private let logger = Logger(subsystem: "io.github.jony4.glimpse.finder", category: "workspace")

    override init() {
        super.init()
        FIFinderSyncController.default().directoryURLs = [URL(fileURLWithPath: "/", isDirectory: true)]
    }

    override func menu(for menuKind: FIMenuKind) -> NSMenu? {
        guard let folders = folders(for: menuKind), !folders.isEmpty else { return nil }
        let menu = NSMenu()
        let item = NSMenuItem(
            title: NSLocalizedString("Open as Glim Workspace", comment: "Finder context menu"),
            action: #selector(openWorkspace(_:)), keyEquivalent: "")
        item.target = self
        item.tag = Int(menuKind.rawValue)
        let app = Bundle.main.bundleURL.deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        item.image = NSWorkspace.shared.icon(forFile: app.path)
        item.image?.size = NSSize(width: 16, height: 16)
        menu.addItem(item)
        return menu
    }

    private func folders(for kind: FIMenuKind) -> [URL]? {
        let controller = FIFinderSyncController.default()
        let urls: [URL]
        switch kind {
        case .contextualMenuForItems:
            urls = controller.selectedItemURLs() ?? []
        case .contextualMenuForContainer, .contextualMenuForSidebar:
            urls = controller.targetedURL().map { [$0] } ?? []
        default:
            return nil
        }
        guard !urls.isEmpty, urls.allSatisfy({ url in
            guard url.isFileURL,
                  let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isPackageKey])
            else { return false }
            return values.isDirectory == true && values.isPackage != true
        }) else { return nil }
        return urls
    }

    @objc private func openWorkspace(_ sender: NSMenuItem) {
        guard let kind = FIMenuKind(rawValue: UInt(sender.tag)),
              let folders = folders(for: kind), !folders.isEmpty else {
            logger.error("Finder workspace action has no selected folders")
            return
        }
        logger.info("Opening \(folders.count) Finder workspace(s)")
        for folder in folders {
            var request = URLComponents()
            request.scheme = "glim"
            request.host = "open-workspace"
            request.queryItems = [URLQueryItem(name: "path", value: folder.path)]
            guard let url = request.url, NSWorkspace.shared.open(url) else {
                logger.error("Cannot deliver the workspace request to Glim")
                continue
            }
        }
    }
}
