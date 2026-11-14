import Foundation
import ZIPFoundation

enum AndroidPackageFormat: String, CaseIterable, Sendable, Hashable { case apk, xapk, apks, apkm }

/// Stable mapping between an archive component and its extracted package path.
/// Optional split metadata is retained verbatim when supplied by the container.
struct AndroidPackageComponent: Codable, Sendable, Hashable {
    let archivePath: String
    let localPath: String
    let splitID: String?
    let componentType: String?
    let configForSplit: String?
    let dependencies: [String]
    let abi: String?
    let density: String?
    let language: String?
    let isBase: Bool
}

struct PreparedAndroidPackage: Sendable {
    let baseAPKURL: URL
    let packageDirectoryURL: URL
    let format: AndroidPackageFormat
    let splitAPKURLs: [URL]
    let components: [AndroidPackageComponent]
}

/// Normalizes Android distribution containers into a bounded, persistent package directory.
struct AndroidPackageImporter: @unchecked Sendable {
    private let fileManager = FileManager.default
    private let maxAPKBytes: UInt64 = 512 * 1_024 * 1_024
    private let maxTotalAPKBytes: UInt64 = 1_024 * 1_024 * 1_024
    private let maxManifestBytes: UInt64 = 4 * 1_024 * 1_024
    private let maxAPKComponentCount = 256

    func prepare(_ sourceURL: URL, in libraryFolder: URL) throws -> PreparedAndroidPackage {
        guard let format = AndroidPackageFormat(rawValue: sourceURL.pathExtension.lowercased()) else {
            throw APKLibraryImportError.unsupportedFile
        }
        let didAccess = sourceURL.startAccessingSecurityScopedResource()
        defer { if didAccess { sourceURL.stopAccessingSecurityScopedResource() } }
        guard sourceURL.isFileURL, fileManager.isReadableFile(atPath: sourceURL.path) else {
            throw APKLibraryImportError.unreadableSource
        }
        let directory = uniquePackageDirectory(for: sourceURL, in: libraryFolder)
        try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        do {
            // Keep a zero-byte format marker for library restoration. Never retain the
            // original XAPK/APKS/APKM container alongside its extracted APK components.
            try Data().write(to: directory.appendingPathComponent("source.\(format.rawValue)"))
            // Persist only normalized APK components so a plain APK is stored once.
            if format == .apk {
                let base = directory.appendingPathComponent("base.apk")
                try fileManager.copyItem(at: sourceURL, to: base)
                let components = [AndroidPackageComponent(
                    archivePath: sourceURL.lastPathComponent, localPath: "base.apk",
                    splitID: nil, componentType: "base", configForSplit: nil,
                    dependencies: [], abi: nil, density: nil, language: nil, isBase: true
                )]
                try writeComponentIndex(components, in: directory)
                return PreparedAndroidPackage(baseAPKURL: base, packageDirectoryURL: directory,
                    format: format, splitAPKURLs: [], components: components)
            }
            let parts = try extractAPKComponents(from: sourceURL, into: directory)
            try writeComponentIndex(parts.components, in: directory)
            return PreparedAndroidPackage(baseAPKURL: parts.base, packageDirectoryURL: directory,
                format: format, splitAPKURLs: parts.splits, components: parts.components)
        } catch {
            try? fileManager.removeItem(at: directory)
            throw error
        }
    }

    private func extractAPKComponents(from sourceURL: URL, into directory: URL) throws -> (base: URL, splits: [URL], components: [AndroidPackageComponent]) {
        guard let archive = Archive(url: sourceURL, accessMode: .read) else { throw APKLibraryImportError.invalidPackageArchive }
        let entries = archive.filter { $0.path.lowercased().hasSuffix(".apk") }
        guard !entries.isEmpty else { throw APKLibraryImportError.missingBaseAPK }
        guard entries.count <= maxAPKComponentCount else { throw APKLibraryImportError.packageArchiveTooManyComponents }
        var manifestBase: String?
        var manifestComponents: [String: [String: Any]] = [:]
        if let manifest = archive.first(where: {
            let name = URL(fileURLWithPath: $0.path).lastPathComponent.lowercased()
            return name == "manifest.json" || name == "info.json"
        }), UInt64(manifest.uncompressedSize) <= maxManifestBytes {
            var data = Data()
            _ = try archive.extract(manifest) { chunk in
                if data.count + chunk.count <= Int(self.maxManifestBytes) { data.append(chunk) }
            }
            if let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                for key in ["split_apks", "splits", "files"] {
                    guard let items = object[key] as? [[String: Any]] else { continue }
                    for item in items {
                        guard let file = (item["file"] as? String) ?? (item["path"] as? String) else { continue }
                        manifestComponents[file] = item
                    }
                    if let base = items.first(where: {
                        let id = (($0["id"] as? String) ?? ($0["split_id"] as? String) ?? "").lowercased()
                        let type = ($0["type"] as? String ?? "").lowercased()
                        return id == "base" || type == "base"
                    }), let file = (base["file"] as? String) ?? (base["path"] as? String) {
                        manifestBase = file
                    }
                    if !manifestComponents.isEmpty { break }
                }
            }
        }
        let baseEntry = manifestBase.flatMap { path in entries.first { $0.path == path } }
            ?? entries.first(where: {
                let name = URL(fileURLWithPath: $0.path).lastPathComponent.lowercased()
                return name == "base.apk" || name == "base-master.apk"
            })
        guard let baseEntry else { throw APKLibraryImportError.missingBaseAPK }

        let componentFolder = directory.appendingPathComponent("components", isDirectory: true)
        try fileManager.createDirectory(at: componentFolder, withIntermediateDirectories: true)
        var total: UInt64 = 0
        var extracted: [String: URL] = [:]
        var normalizedPaths = Set<String>()
        for entry in entries {
            let size = UInt64(entry.uncompressedSize)
            guard size <= maxAPKBytes, total <= maxTotalAPKBytes, size <= maxTotalAPKBytes - total else {
                throw APKLibraryImportError.packageArchiveTooLarge
            }
            total += size
            // Preserve the archive-relative component path. XAPK/APKS bundles
            // may legally contain equal APK basenames under different directories;
            // flattening them loses split identity and can reject valid packages.
            let pathParts = entry.path.split(separator: "/", omittingEmptySubsequences: false)
            guard !pathParts.isEmpty,
                  pathParts.allSatisfy({ !$0.isEmpty && $0 != "." && $0 != ".." }),
                  !entry.path.hasPrefix("/"),
                  !entry.path.contains("\\"),
                  !entry.path.contains(":"),
                  !entry.path.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }),
                  pathParts.count <= 64,
                  pathParts.allSatisfy({ $0.utf8.count <= 255 }) else {
                throw APKLibraryImportError.invalidPackageArchive
            }
            let relativePath = pathParts.map(String.init).joined(separator: "/")
            let collisionKey = relativePath.lowercased()
            guard normalizedPaths.insert(collisionKey).inserted else {
                throw APKLibraryImportError.duplicatePackageEntry(relativePath)
            }
            // Extract the base APK directly to the canonical root path. Previously it
            // was extracted under components/ and copied again to base.apk, doubling the
            // largest component's storage footprint.
            let destination = entry.path == baseEntry.path
                ? directory.appendingPathComponent("base.apk")
                : pathParts.reduce(componentFolder) { partial, part in
                    partial.appendingPathComponent(String(part), isDirectory: true)
                }
            let parent = destination.deletingLastPathComponent()
            try fileManager.createDirectory(at: parent, withIntermediateDirectories: true)
            guard !fileManager.fileExists(atPath: destination.path) else {
                throw APKLibraryImportError.duplicatePackageEntry(relativePath)
            }
            var data = Data()
            data.reserveCapacity(Int(size))
            var exceeded = false
            do {
                _ = try archive.extract(entry) { chunk in
                    guard data.count + chunk.count <= Int(self.maxAPKBytes) else { exceeded = true; return }
                    data.append(chunk)
                }
            } catch { throw APKLibraryImportError.invalidPackageArchive }
            guard !exceeded, UInt64(data.count) == size else { throw APKLibraryImportError.invalidPackageArchive }
            try data.write(to: destination, options: .atomic)
            extracted[entry.path] = destination
        }
        guard let base = extracted[baseEntry.path],
              fileManager.fileExists(atPath: base.path) else { throw APKLibraryImportError.missingBaseAPK }
        let splits = entries.filter { $0.path != baseEntry.path }.compactMap { extracted[$0.path] }
        let components = entries.compactMap { entry -> AndroidPackageComponent? in
            guard extracted[entry.path] != nil else { return nil }
            let metadata = manifestComponents[entry.path] ?? [:]
            let splitID = (metadata["id"] as? String) ?? (metadata["split_id"] as? String)
            let componentType = metadata["type"] as? String
            let dependencies = (metadata["dependencies"] as? [String]) ?? []
            let localPath = entry.path == baseEntry.path ? "base.apk" : "components/\(entry.path)"
            let splitIdentity = ("\((splitID ?? "") + " " + entry.path)".lowercased())
                .replacingOccurrences(of: "arm64_v8a", with: "arm64-v8a")
                .replacingOccurrences(of: "armeabi_v7a", with: "armeabi-v7a")
            let inferredABI = ["arm64-v8a", "armeabi-v7a", "x86_64", "x86"].first {
                splitIdentity.contains($0)
            }
            let inferredDensity = ["xxxhdpi", "xxhdpi", "xhdpi", "hdpi", "mdpi", "ldpi", "tvdpi", "nodpi"].first {
                splitIdentity.contains($0)
            }
            return AndroidPackageComponent(
                archivePath: entry.path, localPath: localPath,
                splitID: splitID, componentType: componentType,
                configForSplit: (metadata["configForSplit"] as? String) ?? (metadata["config_for_split"] as? String),
                dependencies: dependencies,
                abi: (metadata["abi"] as? String) ?? inferredABI,
                density: (metadata["density"] as? String) ?? inferredDensity,
                language: metadata["language"] as? String,
                isBase: entry.path == baseEntry.path
            )
        }
        return (base, splits, components)
    }

    private func writeComponentIndex(_ components: [AndroidPackageComponent], in directory: URL) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(components)
        try data.write(to: directory.appendingPathComponent("components.json"), options: .atomic)
    }

    private func uniquePackageDirectory(for sourceURL: URL, in folder: URL) -> URL {
        let stem = sourceURL.deletingPathExtension().lastPathComponent
        let first = folder.appendingPathComponent(stem, isDirectory: true)
        if !fileManager.fileExists(atPath: first.path) { return first }
        var suffix = 2
        while true {
            let next = folder.appendingPathComponent("\(stem)-\(suffix)", isDirectory: true)
            if !fileManager.fileExists(atPath: next.path) { return next }
            suffix += 1
        }
    }
}
