import Foundation
import ZIPFoundation

struct APKInspection: Equatable {
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
    // Bound decompression and FFI input for untrusted APKs.
    private let maxDEXEntryBytes: UInt64 = 64 * 1_024 * 1_024
    private let maxTotalDEXBytes: UInt64 = 128 * 1_024 * 1_024
    private let maxDEXFileCount = 32

    func inspect(_ url: URL) throws -> APKInspection {
        guard let archive = Archive(url: url, accessMode: .read) else {
            throw APKInspectionError.unreadableArchive
        }

        var count = 0
        var hasManifest = false
        var dexEntries: [Entry] = []
        var abis = Set<String>()
        var hasResources = false

        for entry in archive {
            count += 1
            if entry.path == "AndroidManifest.xml" { hasManifest = true }
            if entry.path == "resources.arsc" { hasResources = true }
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
            _ = try archive.extract(entry) { chunk in
                let (newSize, overflow) = extractedBytes.addingReportingOverflow(UInt64(chunk.count))
                guard !overflow, newSize <= maxDEXEntryBytes else {
                    exceededLimit = true
                    return
                }
                dexData.append(chunk)
                extractedBytes = newSize
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
            isValidPackage: true
        )
    }
}
