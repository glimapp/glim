import Cocoa
import Sparkle

@main
struct UpdaterTests {
    static func main() {
        _ = NSApplication.shared
        let updater = GlimUpdater()
        precondition(!updater.configured)
        updater.command("automatic")
        precondition(!updater.started)
        precondition(!GlimUpdater.validFeed("https://evil.invalid/appcast.xml"))
        precondition(!GlimUpdater.validFeed("https://github.com/another/repo/releases/latest/download/appcast.xml"))
        precondition(GlimUpdater.validFeed("https://github.com/glimapp/glim/releases/latest/download/appcast-macos-arm64.xml"))
        updater.configured = true
        var replies: [SPUUserUpdateChoice] = []
        updater.showReady(toInstallAndRelaunch: { replies.append($0) })
        precondition(updater.installOnQuit)
        precondition(updater.terminationReply() == .terminateCancel)
        precondition(updater.needsQuitApproval)
        updater.command("quit-request-received")
        precondition(!updater.needsQuitApproval)
        updater.command("download")
        updater.command("later")
        precondition(replies.isEmpty, "Downloading or dismissing cannot bypass the install gate")
        updater.command("install-on-quit")
        precondition(replies.isEmpty)
        updater.command("cancel")
        precondition(replies == [.skip])
        precondition(!updater.installOnQuit)
        precondition(updater.choice == nil)
        replies.removeAll()
        updater.showReady(toInstallAndRelaunch: { replies.append($0) })
        updater.command("approval-denied")
        precondition(replies.isEmpty)
        updater.command("install-approved")
        precondition(replies == [.install])
        precondition(updater.terminationReply() == .terminateNow)
        updater.command("withdraw-approval")
        precondition(updater.terminationReply() == .terminateCancel)
        updater.command("install-approved")
        precondition(replies == [.install], "Each native continuation may only be consumed once")
        let snapshot = updater.snapshot()
        precondition(JSONSerialization.isValidJSONObject(snapshot))
        updater.phase = "downloading"
        let network = NSError(domain: NSURLErrorDomain, code: NSURLErrorNotConnectedToInternet)
        let failed = NSError(domain: SUSparkleErrorDomain, code: 2001, userInfo: [NSUnderlyingErrorKey: network])
        updater.showUpdaterError(failed, acknowledgement: {})
        precondition(updater.errorStage == "downloading" && updater.errorKind == "network")
        updater.showUpdaterError(failed, acknowledgement: {})
        precondition(updater.errorStage == "downloading", "Duplicate callbacks preserve the failure stage")
        precondition(updater.snapshot()["error_code"] as? Int == 2001)
        updater.showUserInitiatedUpdateCheck(cancellation: {})
        precondition(updater.lastError == nil && updater.errorKind.isEmpty && updater.errorStage.isEmpty)
        updater.phase = "extracting"
        updater.showUpdaterError(NSError(domain: SUSparkleErrorDomain, code: 3001), acknowledgement: {})
        precondition(updater.errorKind == "verification")
        updater.showUpdateNotFoundWithError(NSError(domain: SUSparkleErrorDomain, code: 1001), acknowledgement: {})
        precondition(updater.lastError == nil && updater.errorKind.isEmpty)
        print("Updater: disabled builds, feed allowlist, cancel semantics and install approval passed")
    }
}
