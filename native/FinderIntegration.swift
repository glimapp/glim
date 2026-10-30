import Cocoa
import FinderSync

@_cdecl("glim_finder_manage")
func glimFinderManage() {
    precondition(Thread.isMainThread)
    FIFinderSyncController.showExtensionManagementInterface()
}
