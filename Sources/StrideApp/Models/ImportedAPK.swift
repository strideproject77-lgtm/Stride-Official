import Foundation

struct ImportedAPK: Identifiable, Hashable {
    let id: UUID
    let fileURL: URL
    let inspection: APKInspection

    var iconData: Data? { inspection.iconData }

    var displayName: String {
        fileURL.deletingPathExtension().lastPathComponent
    }

    init(id: UUID = UUID(), fileURL: URL, inspection: APKInspection) {
        self.id = id
        self.fileURL = fileURL
        self.inspection = inspection
    }

    static func == (lhs: ImportedAPK, rhs: ImportedAPK) -> Bool {
        lhs.fileURL == rhs.fileURL
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(fileURL)
    }
}
