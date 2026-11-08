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

        let diagnostics = StrideDiagnostics.shared
        diagnostics.record(.info, stage: "ARCHIVE", code: "ENTRY_SCAN",
                           "Scanned \(count) ZIP entries; DEX files=\(dexPaths.count); native ABIs=\(abis.sorted().joined(separator: ", "))")
        if !hasManifest {
            diagnostics.record(.error, stage: "ARCHIVE", code: "MANIFEST_MISSING",
                               "AndroidManifest.xml is missing",
                               recovery: "Manifest-dependent checks cannot run; remaining batch APKs will still be inspected")
            throw APKInspectionError.missingManifest
        }
        if dexPaths.isEmpty {
            diagnostics.record(.warning, stage: "DEX", code: "NO_DEX_FOUND",
                               "No classes.dex found; this may be a native-only package or an incomplete APK",
                               recovery: "Continue native-library checks; do not assume Java entry points exist")
        } else {
            diagnostics.record(.info, stage: "DEX", code: "DEX_INVENTORY",
                               "Found DEX files: \(dexPaths.sorted().joined(separator: ", "))")
        }
        if !hasResources {
            diagnostics.record(.warning, stage: "RESOURCES", code: "RESOURCE_TABLE_MISSING",
                               "resources.arsc is absent; resource-table-dependent features may be unavailable",
                               recovery: "Continue checks that do not require the Android resource table")
        }
        if abis.isEmpty {
            diagnostics.record(.info, stage: "NATIVE", code: "NO_NATIVE_LIBRARIES",
                               "No packaged native libraries; continue with DEX/runtime compatibility checks")
        }
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
