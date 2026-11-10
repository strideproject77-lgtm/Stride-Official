import Foundation
import ZIPFoundation

enum AndroidPackageFormat: String, CaseIterable, Sendable, Hashable { case apk, xapk, apks, apkm }
struct PreparedAndroidPackage: Sendable {
    let baseAPKURL: URL
    let packageDirectoryURL: URL
    let format: AndroidPackageFormat
    let splitAPKURLs: [URL]
}

/// Normalizes Android distribution containers into a bounded, persistent package directory.
struct AndroidPackageImporter: @unchecked Sendable {
    private let fileManager = FileManager.default
    private let maxAPKBytes: UInt64 = 512 * 1_024 * 1_024
    private let maxTotalAPKBytes: UInt64 = 1_024 * 1_024 * 1_024
    private let maxManifestBytes: UInt64 = 4 * 1_024 * 1_024

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
            try fileManager.copyItem(at: sourceURL, to: directory.appendingPathComponent("source.\(format.rawValue)"))
            if format == .apk {
                let base = directory.appendingPathComponent("base.apk")
                try fileManager.copyItem(at: sourceURL, to: base)
                return PreparedAndroidPackage(baseAPKURL: base, packageDirectoryURL: directory, format: format, splitAPKURLs: [])
            }
            let parts = try extractAPKComponents(from: sourceURL, into: directory)
            return PreparedAndroidPackage(baseAPKURL: parts.base, packageDirectoryURL: directory, format: format, splitAPKURLs: parts.splits)
        } catch {
            try? fileManager.removeItem(at: directory)
            throw error
        }
    }

    private func extractAPKComponents(from sourceURL: URL, into directory: URL) throws -> (base: URL, splits: [URL]) {
        guard let archive = Archive(url: sourceURL, accessMode: .read) else { throw APKLibraryImportError.invalidPackageArchive }
        let entries = archive.filter { $0.path.lowercased().hasSuffix(".apk") }
        guard !entries.isEmpty else { throw APKLibraryImportError.missingBaseAPK }
        var manifestBase: String?
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
                    if let base = items.first(where: {
                        let id = ($0["id"] as? String ?? "").lowercased()
                        let type = ($0["type"] as? String ?? "").lowercased()
                        return id == "base" || type == "base"
                    }), let file = base["file"] as? String {
                        manifestBase = file
                        break
                    }
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
            let destination = pathParts.reduce(componentFolder) { partial, part in
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
        guard let sourceBase = extracted[baseEntry.path] else { throw APKLibraryImportError.missingBaseAPK }
        let base = directory.appendingPathComponent("base.apk")
        try fileManager.copyItem(at: sourceBase, to: base)
        let splits = entries.filter { $0.path != baseEntry.path }.compactMap { extracted[$0.path] }
        return (base, splits)
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
