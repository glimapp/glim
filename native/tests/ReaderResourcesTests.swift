import Cocoa

private enum ResourceLedger {
    static let lock = NSLock()
    static var next: UInt64 = 0
    static var entries: [UInt64: UInt64] = [:]
    static var denyGrowth = false
    static func update(_ operation: Int32, _ id: UInt64, _ bytes: UInt64) -> UInt64 {
        lock.lock(); defer { lock.unlock() }
        switch ReaderResourceOperation(rawValue: operation) {
        case .track, .reserve:
            precondition(id == 0)
            next += 1; entries[next] = bytes
            return next
        case .resize:
            guard let previous = entries[id] else { preconditionFailure("Unknown reservation") }
            if denyGrowth && bytes > previous { return 0 }
            entries[id] = bytes
            return id
        case .release:
            precondition(entries.removeValue(forKey: id) != nil)
            return 0
        case nil:
            preconditionFailure("Unknown resource operation")
        }
    }
    static var bytes: UInt64 {
        lock.lock(); defer { lock.unlock() }
        return entries.values.reduce(0, +)
    }
}

func readerResourceOwnership() {
    glimSetReaderResources { operation, id, bytes in ResourceLedger.update(operation, id, bytes) }
    defer { glimSetReaderResources(nil) }
    let context = CGContext(data: nil, width: 19, height: 7, bitsPerComponent: 8,
                            bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    let image = context.makeImage()!
    let expected = UInt64(image.bytesPerRow * image.height)
    let firstHandle = ResourceLedger.next + 1
    var raster = NativeRaster.admitted(width: 19, height: 7) {
        precondition(ResourceLedger.bytes > expected)
        return image
    }
    weak let observed = raster
    precondition(ResourceLedger.next == firstHandle)
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

    ResourceLedger.denyGrowth = true
    precondition(NativeRaster.admitted(width: 1, height: 1) { image } == nil)
    ResourceLedger.denyGrowth = false
    precondition(ResourceLedger.bytes == 0)
    var tracked: NativeRaster? = NativeRaster(image)
    withExtendedLifetime(tracked) { precondition(ResourceLedger.bytes == expected) }
    tracked = nil
    precondition(ResourceLedger.bytes == 0)
}
