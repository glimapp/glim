import Cocoa

final class SettingsApp: NSObject, NSApplicationDelegate {
    var settings: AssociationWindow?
    func applicationDidFinishLaunching(_ notification: Notification) {
        do {
            let data = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))
            let request = try NativePreviewRequest.decode(data)
            nativeTranslations = request.localizations
            if let menu = NSApp.mainMenu { localizeMenu(menu) }
            guard case .associations(let associations) = request.content else { NSApp.terminate(nil); return }
            settings = try AssociationWindow(bundleURL: URL(fileURLWithPath: associations.bundle))
        } catch {
            NSAlert(error: error).runModal(); NSApp.terminate(nil)
        }
    }
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        settings?.window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        return true
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}
// Foundation performs a recoverable move, including volume-specific Trash handling.
if CommandLine.arguments.count == 3 && CommandLine.arguments[1] == "--trash" {
    do {
        try FileManager.default.trashItem(at: URL(fileURLWithPath: CommandLine.arguments[2]), resultingItemURL: nil)
        exit(0)
    } catch {
        FileHandle.standardError.write(Data(error.localizedDescription.utf8))
        exit(1)
    }
}
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = SettingsApp()
app.delegate = delegate
let menu = NSMenu(); let appItem = NSMenuItem(); let appMenu = NSMenu()
appMenu.addItem(withTitle: L("Quit Glim Settings"), action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
appItem.submenu = appMenu; menu.addItem(appItem)
let fileItem = NSMenuItem(); let fileMenu = NSMenu(title: "File")
fileMenu.addItem(withTitle: L("Close Window"), action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
fileItem.submenu = fileMenu; menu.addItem(fileItem)
let editItem = NSMenuItem(); let editMenu = NSMenu(title: "Edit")
editMenu.addItem(withTitle: L("Copy"), action: #selector(NSText.copy(_:)), keyEquivalent: "c")
editMenu.addItem(withTitle: L("Select All"), action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
editItem.submenu = editMenu; menu.addItem(editItem); app.mainMenu = menu
app.run()
