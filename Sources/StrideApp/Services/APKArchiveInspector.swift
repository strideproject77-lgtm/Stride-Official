import Foundation
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
        guard let archive = Archive(url: url, accessMode: .read) else {
            throw APKInspectionError.unreadableArchive
        }

        var count = 0
        var hasManifest = false
        var dexEntries: [Entry] = []
        var iconEntries: [Entry] = []
        var abis = Set<String>()
        var hasResources = false

        for entry in archive {
            count += 1
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

        guard hasManifest else { throw APKInspectionError.missingManifest }

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

    /// Extract conventional launcher images while preserving their original artwork.
    /// Adaptive-icon XML is not rasterized; density-specific image fallbacks are used.
    private func extractIcon(from entries: [Entry], in archive: Archive) -> Data? {
        let candidates = entries
            .filter { UInt64($0.uncompressedSize) <= maxIconEntryBytes }
            .sorted { Self.iconScore($0.path) > Self.iconScore($1.path) }

        for entry in candidates {
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
                continue
            }
            guard !exceededLimit, Self.looksLikeSupportedImage(data) else { continue }
            return data
        }
        return nil
    }

    private static func isLauncherIconCandidate(_ path: String) -> Bool {
        let lower = path.lowercased()
        guard lower.hasPrefix("res/"),
              ["png", "webp", "jpg", "jpeg"].contains(URL(fileURLWithPath: lower).pathExtension) else {
            return false
        }
        let name = URL(fileURLWithPath: lower).deletingPathExtension().lastPathComponent
        return ["ic_launcher", "ic_launcher_round", "app_icon", "launcher_icon", "icon"].contains(name)
    }

    private static func iconScore(_ path: String) -> Int {
        let lower = path.lowercased()
        let name = URL(fileURLWithPath: lower).deletingPathExtension().lastPathComponent
        let nameScore: Int
        switch name {
        case "ic_launcher": nameScore = 500
        case "ic_launcher_round": nameScore = 450
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
