import Foundation

struct ImportedAPK: Identifiable, Hashable {
    let id: UUID
    let fileURL: URL
    let packageDirectoryURL: URL
    let packageFormat: AndroidPackageFormat
    let splitAPKURLs: [URL]
    let components: [AndroidPackageComponent]
    let inspection: APKInspection

    var iconData: Data? { inspection.iconData }
    var displayName: String {
        packageDirectoryURL.pathExtension.lowercased() == "apk"
            ? packageDirectoryURL.deletingPathExtension().lastPathComponent
            : packageDirectoryURL.lastPathComponent
    }

    init(id: UUID = UUID(), fileURL: URL, packageDirectoryURL: URL? = nil,
         packageFormat: AndroidPackageFormat = .apk, splitAPKURLs: [URL] = [], components: [AndroidPackageComponent] = [], inspection: APKInspection) {
        self.id = id
        self.fileURL = fileURL
        self.packageDirectoryURL = packageDirectoryURL ?? fileURL
        self.packageFormat = packageFormat
        self.splitAPKURLs = splitAPKURLs
        self.components = components
        self.inspection = inspection
    }

    static func == (lhs: ImportedAPK, rhs: ImportedAPK) -> Bool {
        lhs.packageDirectoryURL == rhs.packageDirectoryURL
    }
    func hash(into hasher: inout Hasher) { hasher.combine(packageDirectoryURL) }
}
