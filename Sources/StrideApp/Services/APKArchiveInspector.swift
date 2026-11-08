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
    private let knownABIs: Set<String> = ["armeabi-v7a", "arm64-v8a", "x86", "x86_64", "armeabi", "mips", "mips64"]

    func inspect(_ url: URL) throws -> APKInspection {
        guard let archive = Archive(url: url, accessMode: .read) else {
            StrideDiagnostics.shared.record(.fatal, stage: "ARCHIVE", code: "ZIP_OPEN_FAILED",
                "The selected file could not be opened as a ZIP archive",
                recovery: "This package cannot be inspected; other selected APKs can continue")
            throw APKInspectionError.unreadableArchive
        }

        var count = 0
        var hasManifest = false
        var dexPaths: [String] = []
        var abis = Set<String>()
        var hasResources = false
        var seenPaths = Set<String>()
        var duplicatePaths = Set<String>()

        // Inventory every entry before deciding whether a missing required file is fatal.
        for entry in archive {
            count += 1
            if !seenPaths.insert(entry.path).inserted { duplicatePaths.insert(entry.path) }
            if entry.path == "AndroidManifest.xml" { hasManifest = true }
            if entry.path == "resources.arsc" { hasResources = true }
            if entry.path == "classes.dex" ||
                entry.path.range(of: #"^classes[0-9]+\.dex$"#, options: .regularExpression) != nil {
                dexPaths.append(entry.path)
            }
            let parts = entry.path.split(separator: "/")
            if parts.count >= 3, parts[0] == "lib", let filename = parts.last, filename.hasSuffix(".so") {
                abis.insert(String(parts[1]))
            }
        }

        let diagnostics = StrideDiagnostics.shared
        diagnostics.record(.info, stage: "ARCHIVE", code: "ENTRY_SCAN",
            "Scanned \(count) ZIP entries; DEX files=\(dexPaths.count); native ABIs=\(abis.sorted().joined(separator: ", "))")
        if count == 0 {
            diagnostics.record(.error, stage: "ARCHIVE", code: "EMPTY_ARCHIVE",
                "The ZIP archive contains no entries",
                recovery: "Package inspection stopped; other selected APKs remain isolated")
        }
        if !duplicatePaths.isEmpty {
            diagnostics.record(.warning, stage: "ARCHIVE", code: "DUPLICATE_ENTRIES",
                "Duplicate ZIP entry paths: \(duplicatePaths.sorted().joined(separator: ", "))",
                recovery: "Treat duplicate-path packages as ambiguous; do not silently choose one copy")
        }
        if !hasManifest {
            diagnostics.record(.error, stage: "MANIFEST", code: "MANIFEST_MISSING",
                "AndroidManifest.xml is missing",
                recovery: "Manifest-dependent checks are unavailable; DEX, resources and native inventory were still collected")
        }

        if dexPaths.isEmpty {
            diagnostics.record(.warning, stage: "DEX", code: "NO_DEX_FOUND",
                "No classes.dex or secondary DEX file was found",
                recovery: "Continue native-library checks; this may be a native-only package")
        } else {
            diagnostics.record(.info, stage: "DEX", code: "DEX_INVENTORY",
                "Found DEX files: \(dexPaths.sorted().joined(separator: ", "))")
            let gaps = Self.multidexSequenceGaps(dexPaths)
            if !gaps.isEmpty {
                diagnostics.record(.warning, stage: "DEX", code: "MULTIDEX_GAP",
                    "Potential gaps in multidex numbering: \(gaps.joined(separator: ", "))",
                    recovery: "Verify APK packaging; do not assume missing DEX files are optional")
            }
        }

        if !hasResources {
            diagnostics.record(.warning, stage: "RESOURCES", code: "RESOURCE_TABLE_MISSING",
                "resources.arsc is absent; resource-table-dependent features may be unavailable",
                recovery: "Continue checks that do not require the Android resource table")
        }
        if abis.isEmpty {
            diagnostics.record(.info, stage: "NATIVE", code: "NO_NATIVE_LIBRARIES",
                "No packaged native .so libraries were found")
        } else {
            let unknown = abis.subtracting(knownABIs).sorted()
            if !unknown.isEmpty {
                diagnostics.record(.warning, stage: "NATIVE", code: "UNKNOWN_ABI",
                    "Unrecognized native ABI folder(s): \(unknown.joined(separator: ", "))",
                    recovery: "Keep the inventory; native compatibility cannot be inferred for these ABI names")
            }
            diagnostics.record(.info, stage: "NATIVE", code: "NATIVE_INVENTORY",
                "Native ABI folders: \(abis.sorted().joined(separator: ", "))")
        }

        // Missing manifest blocks package acceptance, but only after independent checks.
        guard hasManifest else { throw APKInspectionError.missingManifest }

        let report = APKInspection(
            archiveEntryCount: count,
            hasAndroidManifest: true,
            hasDEX: !dexPaths.isEmpty,
            dexFileCount: dexPaths.count,
            nativeABIs: abis.sorted(),
            hasResourceTable: hasResources,
            isValidPackage: true
        )
        diagnostics.record(.info, stage: "PREFLIGHT", code: "ARCHIVE_PREFLIGHT_COMPLETE",
            "Archive-level preflight complete: \(count) entries, \(dexPaths.count) DEX, \(abis.count) native ABI(s)")
        return report
    }

    static func multidexSequenceGaps(_ paths: [String]) -> [String] {
        let numbers = Set(paths.compactMap { path -> Int? in
            if path == "classes.dex" { return 1 }
            guard path.hasPrefix("classes"), path.hasSuffix(".dex") else { return nil }
            let digits = path.dropFirst("classes".count).dropLast(".dex".count)
            guard let number = Int(digits), number >= 2 else { return nil }
            return number
        })
        guard let maximum = numbers.max(), maximum > 1 else { return [] }
        return (2...maximum).filter { !numbers.contains($0) }.map { "classes\($0).dex" }
    }
}
