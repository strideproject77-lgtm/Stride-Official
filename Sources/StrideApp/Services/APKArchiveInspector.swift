import Foundation
import ZIPFoundation

struct APKInspection: Equatable {
    let archiveEntryCount: Int
    let hasAndroidManifest: Bool
    let hasDEX: Bool
    let dexFileCount: Int
    let nativeABIs: [String]
    let hasResourceTable: Bool
    let isValidPackage: Bool

    var summary: String {
        guard isValidPackage else { return "Invalid APK archive" }
        return "\(dexFileCount) DEX · \(nativeABIs.isEmpty ? "No native libraries" : nativeABIs.joined(separator: ", "))"
    }
}

enum APKInspectionError: LocalizedError {
    case unreadableArchive
    case missingManifest

    var errorDescription: String? {
        switch self {
        case .unreadableArchive: return "The selected file is not a readable APK archive."
        case .missingManifest: return "The archive does not contain AndroidManifest.xml."
        }
    }
}

struct APKArchiveInspector {
    func inspect(_ url: URL) throws -> APKInspection {
        guard let archive = Archive(url: url, accessMode: .read) else {
            throw APKInspectionError.unreadableArchive
        }

        var count = 0
        var hasManifest = false
        var dexPaths: [String] = []
        var abis = Set<String>()
        var hasResources = false

        for entry in archive {
            count += 1
            if entry.path == "AndroidManifest.xml" { hasManifest = true }
            if entry.path == "resources.arsc" { hasResources = true }
            if entry.path == "classes.dex" || entry.path.range(of: #"^classes[0-9]+\.dex$"#, options: .regularExpression) != nil {
                dexPaths.append(entry.path)
            }
            if entry.path.hasPrefix("lib/") {
                let parts = entry.path.split(separator: "/")
                if parts.count >= 3, parts.last == "lib" || parts.last?.hasSuffix(".so") == true {
                    abis.insert(String(parts[1]))
                }
            }
        }

        guard hasManifest else { throw APKInspectionError.missingManifest }
        return APKInspection(
            archiveEntryCount: count,
            hasAndroidManifest: hasManifest,
            hasDEX: !dexPaths.isEmpty,
            dexFileCount: dexPaths.count,
            nativeABIs: abis.sorted(),
            hasResourceTable: hasResources,
            isValidPackage: true
        )
    }
}
