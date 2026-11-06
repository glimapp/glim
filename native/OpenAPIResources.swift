import Cocoa
import WebKit
import SQLite3

private final class APIResourceJob {
    private let lock = NSLock()
    private var stopped = false
    private var database: OpaquePointer?
    var cancelled: Bool { lock.lock(); defer { lock.unlock() }; return stopped }
    func cancel() {
        lock.lock(); stopped = true
        if let database = database { sqlite3_interrupt(database) }
        lock.unlock()
    }
    func open(_ path: String) throws -> OpaquePointer {
        var db: OpaquePointer?
        guard sqlite3_open_v2(path, &db, SQLITE_OPEN_READONLY | SQLITE_OPEN_PRIVATECACHE, nil) == SQLITE_OK, let db = db else {
            if let db = db { sqlite3_close(db) }; throw CocoaError(.fileReadUnknown)
        }
        lock.lock()
        if stopped { lock.unlock(); sqlite3_close(db); throw CocoaError(.userCancelled) }
        database = db; lock.unlock()
        sqlite3_exec(db, "PRAGMA query_only=ON; PRAGMA cache_size=-\(ReaderLimits.SQLITE_CACHE_KIB); PRAGMA temp_store=FILE;", nil, nil, nil)
        return db
    }
    func close() {
        lock.lock(); defer { lock.unlock() }
        if let database = database { sqlite3_close(database) }; database = nil
    }
}

// Only the signed viewer and bounded, read-only index queries use this scheme.
// Spec text is JSON data; it never becomes HTML, JavaScript or a network URL.
final class OpenAPIResources: NSObject, WKURLSchemeHandler {
    private let database: String
    private let document: URL
    private let translations: [String: String]
    private let queue = DispatchQueue(label: "glim.openapi.resources", qos: .userInitiated)
    private var active: [ObjectIdentifier: (WKURLSchemeTask, APIResourceJob)] = [:]
    init(database: String, document: URL) {
        self.database = database; self.document = document
        translations = nativeTranslations
    }
    private static func rows(_ db: OpaquePointer, _ sql: String, _ bindings: [Any] = [], job: APIResourceJob) throws -> [[String: Any]] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement = statement else { throw CocoaError(.fileReadCorruptFile) }
        defer { sqlite3_finalize(statement) }
        let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
        for (index, value) in bindings.enumerated() {
            if let number = value as? Int64 { sqlite3_bind_int64(statement, Int32(index + 1), number) }
            else if let value = value as? String { _ = value.withCString { sqlite3_bind_text(statement, Int32(index + 1), $0, -1, transient) } }
        }
        var result: [[String: Any]] = []
        while true {
            if job.cancelled { throw CocoaError(.userCancelled) }
            let code = sqlite3_step(statement)
            if code == SQLITE_DONE { return result }
            guard code == SQLITE_ROW else { throw CocoaError(.fileReadCorruptFile) }
            var row: [String: Any] = [:]
            for index in 0..<sqlite3_column_count(statement) {
                let key = String(cString: sqlite3_column_name(statement, index))
                if sqlite3_column_type(statement, index) == SQLITE_INTEGER { row[key] = sqlite3_column_int64(statement, index) }
                else if let bytes = sqlite3_column_text(statement, index) {
                    row[key] = String(decoding: UnsafeBufferPointer(start: bytes, count: Int(sqlite3_column_bytes(statement, index))), as: UTF8.self)
                }
            }
            result.append(row)
        }
    }
    private static let columns = """
    n.id,n.parent,n.name,n.name_complete,n.kind,n.value,n.value_complete,n.length,
    COALESCE((SELECT value FROM nodes WHERE parent=n.id AND name='name' LIMIT 1),'') AS title,
    COALESCE((SELECT value FROM nodes WHERE parent=n.id AND name='type' LIMIT 1),'') AS type,
    COALESCE((SELECT value FROM nodes WHERE parent=n.id AND name='in' LIMIT 1),'') AS location,
    COALESCE((SELECT value FROM nodes WHERE parent=n.id AND name='required' AND kind='scalar' LIMIT 1),'') AS required
    """
    private func query(_ url: URL, job: APIResourceJob) throws -> Data {
        let lease = NativeResourceLease(bytes: UInt64(ReaderLimits.OPENAPI_FIELD_BYTES * ReaderLimits.OPENAPI_PAGE_NODES * 16 + ReaderLimits.SQLITE_CACHE_KIB * 1024))
        defer { withExtendedLifetime(lease) {} }
        let db = try job.open(database); defer { job.close() }
        let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        func argument(_ name: String) -> String { items.first { $0.name == name }?.value ?? "" }
        func number(_ name: String) -> Int64 { max(0, Int64(argument(name)) ?? 0) }
        let limit = Int64(ReaderLimits.OPENAPI_PAGE_NODES)
        let result: Any
        switch url.path {
        case "/endpoints":
            let page = try Self.rows(db, "SELECT id,path,method,summary,tag FROM endpoints ORDER BY tag,path,method,id LIMIT ?1 OFFSET ?2", [limit + 1, number("offset")], job: job)
            result = ["items": Array(page.prefix(Int(limit))), "more": page.count > limit]
        case "/node":
            result = try Self.rows(db, "SELECT \(Self.columns) FROM nodes n WHERE n.id=?1", [number("id")], job: job).first ?? [:]
        case "/child":
            result = try Self.rows(db, "SELECT \(Self.columns) FROM nodes n WHERE n.parent=?1 AND n.name=?2 ORDER BY n.id DESC LIMIT 1", [number("id"), argument("name")], job: job).first ?? [:]
        case "/children":
            let page = try Self.rows(db, "SELECT \(Self.columns) FROM nodes n WHERE n.parent=?1 ORDER BY n.id LIMIT ?2 OFFSET ?3", [number("id"), limit + 1, number("offset")], job: job)
            result = ["items": Array(page.prefix(Int(limit))), "more": page.count > limit]
        case "/pointer":
            let pointer = argument("value")
            guard pointer.isEmpty || pointer.hasPrefix("/") else { throw CocoaError(.fileReadUnsupportedScheme) }
            var id: Int64 = 1
            if !pointer.isEmpty {
                for component in pointer.dropFirst().components(separatedBy: "/") {
                    let name = component.replacingOccurrences(of: "~1", with: "/").replacingOccurrences(of: "~0", with: "~")
                    guard let row = try Self.rows(db, "SELECT id FROM nodes WHERE parent=?1 AND name=?2 AND name_complete=1 ORDER BY id DESC LIMIT 1", [id, name], job: job).first,
                          let child = row["id"] as? Int64 else { throw CocoaError(.fileReadNoSuchFile) }
                    id = child
                }
            }
            result = try Self.rows(db, "SELECT \(Self.columns) FROM nodes n WHERE n.id=?1", [id], job: job).first ?? [:]
        case "/raw":
            guard let row = try Self.rows(db, "SELECT start,length FROM nodes WHERE id=?1", [number("id")], job: job).first,
                  let start = row["start"] as? Int64, let length = row["length"] as? Int64 else { throw CocoaError(.fileReadNoSuchFile) }
            var offset = min(length, number("offset"))
            let file = try FileHandle(forReadingFrom: URL(fileURLWithPath: database).deletingLastPathComponent().appendingPathComponent("source.json"))
            defer { try? file.close() }
            try file.seek(toOffset: UInt64(start + offset))
            var data = try file.read(upToCount: min(ReaderLimits.OPENAPI_FIELD_BYTES, Int(length - offset))) ?? Data()
            while let first = data.first, first & 0xc0 == 0x80 {
                data.removeFirst(); offset += 1
            }
            // Raw pages use byte offsets. Preserve split UTF-8 scalars between
            // adjacent pages by backing off the incomplete tail.
            var count = data.count
            if offset + Int64(count) < length {
                while count > 0 && String(data: data.prefix(count), encoding: .utf8) == nil { count -= 1 }
            }
            result = ["text": String(decoding: data.prefix(count), as: UTF8.self), "offset": offset,
                      "next": offset + Int64(count), "more": offset + Int64(count) < length]
        default: throw CocoaError(.fileReadNoSuchFile)
        }
        return try JSONSerialization.data(withJSONObject: result)
    }
    func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
        let id = ObjectIdentifier(task), job = APIResourceJob()
        active[id] = (task, job)
        guard let url = task.request.url else { task.didFailWithError(URLError(.badURL)); active.removeValue(forKey: id); return }
        var configuration: Data?
        if url.path == "/config" {
            var colors: [String: String] = [:]
            webView.effectiveAppearance.performAsCurrentDrawingAppearance {
                let background = NSColor.textBackgroundColor.usingColorSpace(.sRGB)!
                for (key, color) in ["background": NSColor.textBackgroundColor, "foreground": .labelColor,
                    "muted": .secondaryLabelColor, "border": .separatorColor, "control": .controlBackgroundColor,
                    "accent": .linkColor] {
                    if let rgb = color.usingColorSpace(.sRGB) {
                        let alpha = rgb.alphaComponent
                        colors[key] = String(format: "#%02x%02x%02x",
                            Int((rgb.redComponent * alpha + background.redComponent * (1-alpha)) * 255),
                            Int((rgb.greenComponent * alpha + background.greenComponent * (1-alpha)) * 255),
                            Int((rgb.blueComponent * alpha + background.blueComponent * (1-alpha)) * 255))
                    }
                }
            }
            configuration = try? JSONSerialization.data(withJSONObject: ["translations": translations, "colors": colors,
                "pageSize": ReaderLimits.OPENAPI_PAGE_NODES, "depth": ReaderLimits.OPENAPI_DEPTH,
                "fieldBytes": ReaderLimits.OPENAPI_FIELD_BYTES])
        }
        let config = configuration
        queue.async { [weak self] in
            guard let self = self, !job.cancelled else { return }
            do {
                let main = url.path == self.document.path
                let data = try main ? Data(OpenAPIAssets.html.utf8) : (config ?? self.query(url, job: job))
                guard !job.cancelled else { return }
                let mime = main ? "text/html" : "application/json"
                DispatchQueue.main.async {
                    guard let (task, _) = self.active.removeValue(forKey: id) else { return }
                    task.didReceive(URLResponse(url: url, mimeType: mime, expectedContentLength: data.count, textEncodingName: "utf-8"))
                    task.didReceive(data); task.didFinish()
                }
            } catch {
                DispatchQueue.main.async {
                    guard let (task, _) = self.active.removeValue(forKey: id) else { return }
                    // Query errors stay local and retryable; no document data is
                    // sent to a validator, a server or an external reference.
                    let data = (try? JSONSerialization.data(withJSONObject: ["error": error.localizedDescription])) ?? Data()
                    task.didReceive(URLResponse(url: url, mimeType: "application/json", expectedContentLength: data.count, textEncodingName: "utf-8"))
                    task.didReceive(data); task.didFinish()
                }
            }
        }
    }
    func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {
        active.removeValue(forKey: ObjectIdentifier(task))?.1.cancel()
    }
    func close() {
        for (_, job) in active.values { job.cancel() }; active.removeAll()
    }
}
