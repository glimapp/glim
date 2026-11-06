import Foundation

struct NativePreviewRequest: Decodable {
    let version: Int
    let localizations: [String: String]
    let content: NativePreviewContent

    static func decode(_ data: Data) throws -> NativePreviewRequest {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let request = try decoder.decode(Self.self, from: data)
        guard request.version == 1 else {
            throw DecodingError.dataCorrupted(.init(codingPath: [], debugDescription: "Unsupported native preview protocol"))
        }
        return request
    }
}

struct PreviewFile: Decodable { let source: String }
struct PreviewImage: Decodable {
    struct Size: Decodable { let width: Double; let height: Double }
    let source: String
    let prepared: String?
    let size: Size?
}
struct PreviewOffice: Decodable { let source: String; let reading: String?; let imagePages: Bool }
struct PreviewWorkbook: Decodable {
    let source: String
    let database: String
    let cache: String
    let helper: String
    let information: String
}
struct PreviewHTML: Decodable { let source: String; let text: String? }
struct PreviewOpenAPI: Decodable { let source: String; let database: String }
struct PreviewMedia: Decodable {
    struct Preparation: Decodable { let files: [String]; let helper: String; let directory: String }
    let files: [String]
    let shuffle: Bool
    let autoplay: Bool
    let preparation: Preparation?
}
struct PreviewAssociations: Decodable { let bundle: String }

enum NativePreviewContent: Decodable {
    case openApi(PreviewOpenAPI)
    case pdf(PreviewFile)
    case image(PreviewImage)
    case font(PreviewFile)
    case office(PreviewOffice)
    case workbook(PreviewWorkbook)
    case html(PreviewHTML)
    case media(PreviewMedia)
    case associations(PreviewAssociations)

    private enum CodingKeys: String, CodingKey { case kind }
    private enum Kind: String, Decodable { case pdf, image, font, office, workbook, html, media, associations; case openApi = "open_api" }
    init(from decoder: Decoder) throws {
        let kind = try decoder.container(keyedBy: CodingKeys.self).decode(Kind.self, forKey: .kind)
        switch kind {
        case .openApi: self = .openApi(try PreviewOpenAPI(from: decoder))
        case .pdf: self = .pdf(try PreviewFile(from: decoder))
        case .image: self = .image(try PreviewImage(from: decoder))
        case .font: self = .font(try PreviewFile(from: decoder))
        case .office: self = .office(try PreviewOffice(from: decoder))
        case .workbook: self = .workbook(try PreviewWorkbook(from: decoder))
        case .html: self = .html(try PreviewHTML(from: decoder))
        case .media: self = .media(try PreviewMedia(from: decoder))
        case .associations: self = .associations(try PreviewAssociations(from: decoder))
        }
    }
    var files: [String] {
        switch self {
        case .openApi(let file): return [file.source]
        case .pdf(let file), .font(let file): return [file.source]
        case .image(let file): return [file.source]
        case .office(let file): return [file.source]
        case .workbook(let file): return [file.source]
        case .html(let file): return [file.source]
        case .media(let media): return media.files
        case .associations: return []
        }
    }
}
