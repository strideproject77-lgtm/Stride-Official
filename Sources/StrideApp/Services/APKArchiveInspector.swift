import Foundation
import UIKit
import ZIPFoundation

struct APKInspection: Equatable, Sendable {
    let archiveEntryCount: Int
    let hasAndroidManifest: Bool
    let hasDEX: Bool
    let dexFileCount: Int
    let analyzedDexFileCount: Int
    let skippedDexFileCount: Int
    let dexClassCount: UInt64
    let dexMethodCount: UInt64
    let nativeABIs: [String]
    let hasResourceTable: Bool
    let isValidPackage: Bool
    let iconData: Data?

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

struct APKArchiveInspector: Sendable {
    // Bound decompression and FFI input for untrusted APKs.
    private let maxDEXEntryBytes: UInt64 = 64 * 1_024 * 1_024
    private let maxTotalDEXBytes: UInt64 = 128 * 1_024 * 1_024
    private let maxDEXFileCount = 32
    private let maxIconEntryBytes: UInt64 = 8 * 1_024 * 1_024

    func inspect(_ url: URL) throws -> APKInspection {
        StrideDiagnostics.recordAsync(severity: "INFO", stage: "ARCHIVE", code: "SCAN_START",
            message: "Inspecting APK archive: \(url.lastPathComponent)")
        guard let archive = Archive(url: url, accessMode: .read) else {
            StrideDiagnostics.recordAsync(severity: "FATAL", stage: "ARCHIVE", code: "ZIP_OPEN_FAILED",
                message: "Selected file is not a readable ZIP/APK archive",
                recovery: "Keep the original file and verify that it is a complete APK")
            throw APKInspectionError.unreadableArchive
        }

        var count = 0
        var hasManifest = false
        var dexEntries: [Entry] = []
        var iconEntries: [Entry] = []
        var abis = Set<String>()
        var hasResources = false
        var seenPaths = Set<String>()
        var duplicatePaths = Set<String>()

        for entry in archive {
            count += 1
            if !seenPaths.insert(entry.path).inserted { duplicatePaths.insert(entry.path) }
            if entry.path == "AndroidManifest.xml" { hasManifest = true }
            if entry.path == "resources.arsc" { hasResources = true }
            if Self.isLauncherIconCandidate(entry.path) { iconEntries.append(entry) }
            if entry.path == "classes.dex" || entry.path.range(of: #"^classes[0-9]+\.dex$"#, options: .regularExpression) != nil {
                dexEntries.append(entry)
            }
            if entry.path.hasPrefix("lib/") {
                let parts = entry.path.split(separator: "/")
                if parts.count >= 3, parts.last == "lib" || parts.last?.hasSuffix(".so") == true {
                    abis.insert(String(parts[1]))
                }
            }
        }

        StrideDiagnostics.recordAsync(severity: "INFO", stage: "ARCHIVE", code: "ENTRY_SCAN",
            message: "Scanned \(count) entries; DEX=\(dexEntries.count); native ABIs=\(abis.sorted().joined(separator: ", "))")
        if !duplicatePaths.isEmpty {
            StrideDiagnostics.recordAsync(severity: "WARN", stage: "ARCHIVE", code: "DUPLICATE_ENTRIES",
                message: "Duplicate ZIP paths: \(duplicatePaths.sorted().joined(separator: ", "))",
                recovery: "Treat duplicate-path packages as ambiguous; do not silently select one copy")
        }
        if !hasManifest {
            StrideDiagnostics.recordAsync(severity: "ERROR", stage: "MANIFEST", code: "MANIFEST_MISSING",
                message: "AndroidManifest.xml is missing",
                recovery: "Manifest-based execution cannot continue; independent archive findings were still collected")
            throw APKInspectionError.missingManifest
        }
        if dexEntries.isEmpty {
            StrideDiagnostics.recordAsync(severity: "WARN", stage: "DEX", code: "NO_DEX_FOUND",
                message: "No classes.dex or secondary DEX file was found",
                recovery: "This may be a native-only package; inspect native libraries separately")
        } else {
            let paths = dexEntries.map(\.path)
            let gaps = APKArchiveInspector.multidexSequenceGaps(paths)
            StrideDiagnostics.recordAsync(severity: "INFO", stage: "DEX", code: "DEX_INVENTORY",
                message: "Found DEX entries: \(paths.sorted().joined(separator: ", "))")
            if !gaps.isEmpty {
                StrideDiagnostics.recordAsync(severity: "WARN", stage: "DEX", code: "MULTIDEX_GAP",
                    message: "Potential multidex numbering gaps: \(gaps.joined(separator: ", "))",
                    recovery: "Verify split/multidex packaging before attempting execution")
            }
        }
        if !hasResources {
            StrideDiagnostics.recordAsync(severity: "WARN", stage: "RESOURCES", code: "RESOURCE_TABLE_MISSING",
                message: "resources.arsc is absent; resource-table-dependent features may be unavailable")
        }
        if abis.isEmpty {
            StrideDiagnostics.recordAsync(severity: "INFO", stage: "NATIVE", code: "NO_NATIVE_LIBRARIES",
                message: "No native .so libraries were found")
        } else {
            StrideDiagnostics.recordAsync(severity: "INFO", stage: "NATIVE", code: "NATIVE_INVENTORY",
                message: "Native ABI folders: \(abis.sorted().joined(separator: ", "))")
        }

        var analyzedDexCount = 0
        var skippedDexCount = 0
        var dexClassCount: UInt64 = 0
        var dexMethodCount: UInt64 = 0
        var declaredDEXBytes: UInt64 = 0

        for entry in dexEntries.prefix(maxDEXFileCount) {
            let declaredSize = UInt64(entry.uncompressedSize)
            guard declaredSize <= maxDEXEntryBytes,
                  declaredDEXBytes <= maxTotalDEXBytes,
                  declaredSize <= maxTotalDEXBytes - declaredDEXBytes else {
                skippedDexCount += 1
                continue
            }
            declaredDEXBytes += declaredSize

            var dexData = Data()
            dexData.reserveCapacity(Int(declaredSize))
            var extractedBytes: UInt64 = 0
            var exceededLimit = false
            do {
                _ = try archive.extract(entry) { chunk in
                    let (newSize, overflow) = extractedBytes.addingReportingOverflow(UInt64(chunk.count))
                    guard !overflow, newSize <= maxDEXEntryBytes else {
                        exceededLimit = true
                        return
                    }
                    dexData.append(chunk)
                    extractedBytes = newSize
                }
            } catch {
                skippedDexCount += 1
                continue
            }
            guard !exceededLimit else {
                skippedDexCount += 1
                continue
            }

            do {
                let inventory = try StrideDexInventory.inspect(dexData)
                analyzedDexCount += 1
                dexClassCount += UInt64(inventory.classCount)
                dexMethodCount += UInt64(inventory.methodCount)
            } catch {
                skippedDexCount += 1
            }
        }
        skippedDexCount += max(0, dexEntries.count - min(dexEntries.count, maxDEXFileCount))

        if skippedDexCount > 0 {
            StrideDiagnostics.recordAsync(severity: "WARN", stage: "DEX", code: "DEX_SCAN_PARTIAL",
                message: "Analyzed \(analyzedDexCount) DEX file(s); skipped \(skippedDexCount)",
                recovery: "Check DEX size limits, archive extraction integrity and DEX parser diagnostics")
        }
        StrideDiagnostics.recordAsync(severity: "INFO", stage: "PREFLIGHT", code: "PREFLIGHT_COMPLETE",
            message: "APK preflight complete: entries=\(count), DEX=\(dexEntries.count), analyzed=\(analyzedDexCount), classes=\(dexClassCount), methods=\(dexMethodCount)")
        let iconData = extractIcon(from: iconEntries, in: archive)
        return APKInspection(
            archiveEntryCount: count,
            hasAndroidManifest: hasManifest,
            hasDEX: !dexEntries.isEmpty,
            dexFileCount: dexEntries.count,
            analyzedDexFileCount: analyzedDexCount,
            skippedDexFileCount: skippedDexCount,
            dexClassCount: dexClassCount,
            dexMethodCount: dexMethodCount,
            nativeABIs: abis.sorted(),
            hasResourceTable: hasResources,
            isValidPackage: true,
            iconData: iconData
        )
    }

    /// Inspects the base APK and every preserved split. Native libraries such as
    /// ARM64 libgame.so may live only in config.arm64_v8a, not in the base APK.
    /// The base manifest remains authoritative; split findings are aggregated for
    /// preflight visibility without pretending that split resources are merged.
    func inspect(_ baseURL: URL, including splitURLs: [URL]) throws -> APKInspection {
        let base = try inspect(baseURL)
        guard !splitURLs.isEmpty else { return base }

        var splitReports: [APKInspection] = []
        splitReports.reserveCapacity(splitURLs.count)
        for splitURL in splitURLs {
            let report = try inspect(splitURL)
            splitReports.append(report)
            StrideDiagnostics.recordAsync(severity: "INFO", stage: "SPLIT", code: "SPLIT_INSPECTED",
                message: "Inspected split \(splitURL.lastPathComponent): DEX=\(report.dexFileCount), native ABIs=\(report.nativeABIs.joined(separator: ", "))")
        }

        let all = [base] + splitReports
        let combined = APKInspection(
            archiveEntryCount: all.reduce(0) { $0 + $1.archiveEntryCount },
            hasAndroidManifest: base.hasAndroidManifest,
            hasDEX: all.contains(where: \.hasDEX),
            dexFileCount: all.reduce(0) { $0 + $1.dexFileCount },
            analyzedDexFileCount: all.reduce(0) { $0 + $1.analyzedDexFileCount },
            skippedDexFileCount: all.reduce(0) { $0 + $1.skippedDexFileCount },
            dexClassCount: all.reduce(0) { $0 + $1.dexClassCount },
            dexMethodCount: all.reduce(0) { $0 + $1.dexMethodCount },
            nativeABIs: Array(Set(all.flatMap(\.nativeABIs))).sorted(),
            hasResourceTable: all.contains(where: \.hasResourceTable),
            isValidPackage: base.isValidPackage,
            iconData: base.iconData ?? splitReports.compactMap(\.iconData).first
        )
        StrideDiagnostics.recordAsync(severity: "INFO", stage: "PREFLIGHT", code: "PACKAGE_SPLITS_AGGREGATED",
            message: "Base plus \(splitReports.count) split(s): total DEX=\(combined.dexFileCount), native ABIs=\(combined.nativeABIs.joined(separator: ", "))")
        return combined
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

    /// Extract conventional launcher images while preserving their original artwork.
    /// Adaptive-icon XML is not rasterized; density-specific image fallbacks are used.
    private func extractIcon(from entries: [Entry], in archive: Archive) -> Data? {
        let candidates = entries
            .filter { UInt64($0.uncompressedSize) <= maxIconEntryBytes }
            .sorted { Self.iconScore($0.path) > Self.iconScore($1.path) }

        // Adaptive launcher icons often store foreground and background as
        // separate transparent raster assets. Composite them before displaying.
        let foregroundEntry = candidates.first {
            URL(fileURLWithPath: $0.path).deletingPathExtension().lastPathComponent.lowercased() == "ic_launcher_foreground"
        }
        let backgroundEntry = candidates.first {
            URL(fileURLWithPath: $0.path).deletingPathExtension().lastPathComponent.lowercased() == "ic_launcher_background"
        }

        if let foregroundEntry, let backgroundEntry,
           let foregroundData = readIconImage(foregroundEntry, in: archive),
           let backgroundData = readIconImage(backgroundEntry, in: archive),
           let foreground = UIImage(data: foregroundData),
           let background = UIImage(data: backgroundData) {
            let size = CGSize(
                width: max(foreground.size.width, background.size.width),
                height: max(foreground.size.height, background.size.height)
            )
            let format = UIGraphicsImageRendererFormat()
            format.scale = 1
            format.opaque = false
            let renderer = UIGraphicsImageRenderer(size: size, format: format)
            let composed = renderer.image { _ in
                let rect = CGRect(origin: .zero, size: size)
                background.draw(in: rect)
                foreground.draw(in: rect)
            }
            if let png = composed.pngData() { return png }
        }

        for entry in candidates {
            if let data = readIconImage(entry, in: archive) { return data }
        }
        return nil
    }

    private func readIconImage(_ entry: Entry, in archive: Archive) -> Data? {
        var data = Data()
        data.reserveCapacity(Int(entry.uncompressedSize))
        var exceededLimit = false
        do {
            _ = try archive.extract(entry) { chunk in
                guard data.count + chunk.count <= Int(self.maxIconEntryBytes) else {
                    exceededLimit = true
                    return
                }
                data.append(chunk)
            }
        } catch {
            return nil
        }
        guard !exceededLimit, Self.looksLikeSupportedImage(data) else { return nil }
        return data
    }

    private static func isLauncherIconCandidate(_ path: String) -> Bool {
        let lower = path.lowercased()
        guard lower.hasPrefix("res/"),
              ["png", "webp", "jpg", "jpeg"].contains(URL(fileURLWithPath: lower).pathExtension) else {
            return false
        }
        let name = URL(fileURLWithPath: lower).deletingPathExtension().lastPathComponent
        return ["ic_launcher", "ic_launcher_round", "ic_launcher_foreground", "ic_launcher_background", "app_icon", "launcher_icon", "icon"].contains(name)
            || name.hasPrefix("ic_launcher_")
            || name.contains("launcher")
    }

    private static func iconScore(_ path: String) -> Int {
        let lower = path.lowercased()
        let name = URL(fileURLWithPath: lower).deletingPathExtension().lastPathComponent
        let nameScore: Int
        switch name {
        case "ic_launcher": nameScore = 500
        case "ic_launcher_round": nameScore = 450
        case "ic_launcher_foreground": nameScore = 430
        case "ic_launcher_background": nameScore = 420
        case "app_icon": nameScore = 400
        case "launcher_icon": nameScore = 350
        default: nameScore = 300
        }
        let densityScore: Int
        if lower.contains("xxxhdpi") { densityScore = 80 }
        else if lower.contains("xxhdpi") { densityScore = 70 }
        else if lower.contains("xhdpi") { densityScore = 60 }
        else if lower.contains("hdpi") { densityScore = 50 }
        else if lower.contains("mdpi") { densityScore = 40 }
        else if lower.contains("tvdpi") { densityScore = 35 }
        else if lower.contains("nodpi") { densityScore = 30 }
        else { densityScore = 20 }
        let formatScore = lower.hasSuffix(".png") ? 3 : (lower.hasSuffix(".webp") ? 2 : 1)
        return nameScore + densityScore + formatScore
    }

    private static func looksLikeSupportedImage(_ data: Data) -> Bool {
        let bytes = [UInt8](data.prefix(12))
        if bytes.count >= 8, bytes[0...7].elementsEqual([137, 80, 78, 71, 13, 10, 26, 10]) {
            return true
        }
        if bytes.count >= 12,
           String(bytes: bytes[0...3], encoding: .ascii) == "RIFF",
           String(bytes: bytes[8...11], encoding: .ascii) == "WEBP" {
            return true
        }
        return bytes.count >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF
    }
}
