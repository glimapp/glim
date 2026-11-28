import Foundation
import CoreGraphics

// Numeric handles stay with their Swift owners across queue handoffs.
typealias ReaderResourceCallback = @convention(c) (Int32, UInt64, UInt64) -> UInt64

enum ReaderResourceOperation: Int32 {
    case track = 0
    case reserve = 1
    case resize = 2
    case release = 3
}

private enum ReaderResources {
    static let lock = NSLock()
    static var callback: ReaderResourceCallback?

    static func registeredCallback() -> ReaderResourceCallback? {
        lock.lock(); defer { lock.unlock() }
        return callback
    }
}

@_cdecl("glim_set_reader_resources")
func glimSetReaderResources(_ callback: ReaderResourceCallback?) {
    ReaderResources.lock.lock(); defer { ReaderResources.lock.unlock() }
    ReaderResources.callback = callback
}

final class NativeResourceLease {
    private let callback: ReaderResourceCallback?
    private let id: UInt64

    init(bytes: UInt64) {
        callback = ReaderResources.registeredCallback()
        id = callback?(ReaderResourceOperation.track.rawValue, 0, bytes) ?? 0
    }

    init?(reservingBytes bytes: UInt64) {
        callback = ReaderResources.registeredCallback()
        id = callback?(ReaderResourceOperation.reserve.rawValue, 0, bytes) ?? 0
        if id == 0 { return nil }
    }

    func resize(to bytes: UInt64) -> Bool {
        id != 0 && callback?(ReaderResourceOperation.resize.rawValue, id, bytes) == id
    }

    deinit {
        if id != 0 { _ = callback?(ReaderResourceOperation.release.rawValue, id, 0) }
    }
}

// Consumers share this owner rather than creating another charge for the same
// pixels. A cancelled worker keeps its charge until its actual image use ends.
final class NativeRaster {
    let image: CGImage
    private let lease: NativeResourceLease

    // Decode, conversion and publication can overlap. Admit that envelope
    // before ImageIO allocates; the delivered owner tracks its actual stride.
    static func admitted(width: Double, height: Double, decode: () -> CGImage?) -> NativeRaster? {
        let row = ceil(ceil(max(1, width)) * 4 / 64) * 64
        let bytes = row * ceil(max(1, height)) * 3
        guard bytes.isFinite, bytes < Double(UInt64.max),
              let memory = NativeResourceLease(reservingBytes: UInt64(bytes)),
              let image = decode(), memory.resize(to: byteCount(image)) else { return nil }
        return NativeRaster(image, lease: memory)
    }

    convenience init(_ image: CGImage) {
        self.init(image, lease: NativeResourceLease(bytes: Self.byteCount(image)))
    }

    private init(_ image: CGImage, lease: NativeResourceLease) {
        self.image = image
        self.lease = lease
    }

    private static func byteCount(_ image: CGImage) -> UInt64 {
        let (bytes, overflow) = UInt64(image.bytesPerRow).multipliedReportingOverflow(by: UInt64(image.height))
        return overflow ? UInt64.max : bytes
    }
}
