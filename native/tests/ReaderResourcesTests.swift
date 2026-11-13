import Cocoa

private enum ResourceLedger {
    static let lock = NSLock()
    static var next: UInt64 = 0
    static var entries: [UInt64: UInt64] = [:]
    static func update(_ id: UInt64, _ bytes: UInt64) -> UInt64 {
        lock.lock(); defer { lock.unlock() }
        if id != 0 {
            precondition(entries.removeValue(forKey: id) != nil)
            return 0
        }
        next += 1; entries[next] = bytes
        return next
    }
    static var bytes: UInt64 {
        lock.lock(); defer { lock.unlock() }
        return entries.values.reduce(0, +)
    }
}

func readerResourceOwnership() {
    glimSetReaderResources { id, bytes, _ in ResourceLedger.update(id, bytes) }
    defer { glimSetReaderResources(nil) }
    let context = CGContext(data: nil, width: 19, height: 7, bitsPerComponent: 8,
                            bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    let image = context.makeImage()!
    var raster: NativeRaster? = NativeRaster(image)
    weak var observed = raster
    let expected = UInt64(image.bytesPerRow * image.height)
    precondition(ResourceLedger.bytes == expected)

    let queue = DispatchQueue(label: "glim.resource-lifetime-test")
    let started = DispatchSemaphore(value: 0), finish = DispatchSemaphore(value: 0)
    var work: DispatchWorkItem? = DispatchWorkItem { [retained = raster!] in
        defer { withExtendedLifetime(retained) {} }
        started.signal()
        precondition(finish.wait(timeout: .now() + 5) == .success)
        precondition(retained.image.width == 19)
    }
    queue.async(execute: work!)
    precondition(started.wait(timeout: .now() + 5) == .success)
    raster = nil
    work?.cancel()
    precondition(observed != nil && ResourceLedger.bytes == expected)
    finish.signal()
    queue.sync {}
    work = nil
    precondition(observed == nil && ResourceLedger.bytes == 0)
}
