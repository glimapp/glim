import Foundation
import CoreGraphics

// ID 0 creates a working-buffer lease; a nonzero ID releases it. No pointers
// cross this boundary, and registration is independent of telemetry consent.
typealias ReaderResourceCallback = @convention(c) (UInt64, UInt64, Bool) -> UInt64

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
        id = callback?(0, bytes, false) ?? 0
    }

    init?(reservingBytes bytes: UInt64) {
        callback = ReaderResources.registeredCallback()
        id = callback?(0, bytes, true) ?? 0
        if id == 0 { return nil }
    }

    deinit {
        if id != 0 { _ = callback?(id, 0, false) }
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
              let memory = NativeResourceLease(reservingBytes: UInt64(bytes)) else { return nil }
        return withExtendedLifetime(memory) { decode().map(NativeRaster.init) }
    }

    init(_ image: CGImage) {
        self.image = image
        let (bytes, overflow) = UInt64(image.bytesPerRow).multipliedReportingOverflow(by: UInt64(image.height))
        lease = NativeResourceLease(bytes: overflow ? UInt64.max : bytes)
    }
}
