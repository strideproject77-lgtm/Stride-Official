import Foundation
import Combine
import SwiftUI

enum APKLibraryImportError: LocalizedError {
    case unsupportedFile, unreadableSource, invalidPackageArchive, missingBaseAPK
    case packageArchiveTooLarge, duplicatePackageEntry(String)
    var errorDescription: String? {
        switch self {
        case .unsupportedFile: return "Supported Android packages are APK, XAPK, APKS and APKM."
        case .unreadableSource: return "iOS could not access this file. Save it in Files and try importing it again."
        case .invalidPackageArchive: return "The Android package archive is damaged or contains an invalid APK entry."
        case .missingBaseAPK: return "No base APK was found. A configuration split cannot be imported by itself."
        case .packageArchiveTooLarge: return "The package exceeds Stride's safe extraction limits."
        case .duplicatePackageEntry(let name): return "The package contains duplicate APK filenames: \(name)."
        }
    }
}

@MainActor
final class APKLibraryStore: ObservableObject {
    @Published private(set) var packages: [ImportedAPK] = []
    private let inspector = APKArchiveInspector()
    private let packageImporter = AndroidPackageImporter()
    private let fileManager = FileManager.default

    init() { reload() }

    func importPackage(from sourceURL: URL) throws {
        StrideDiagnostics.shared.record(.info, stage: "IMPORT", code: "IMPORT_START", "Starting Android package import: \(sourceURL.lastPathComponent)")
        let prepared = try preparePackage(from: sourceURL)
        do {
            let report = try inspector.inspect(prepared.baseAPKURL)
            let item = ImportedAPK(fileURL: prepared.baseAPKURL, packageDirectoryURL: prepared.packageDirectoryURL,
                packageFormat: prepared.format, splitAPKURLs: prepared.splitAPKURLs, inspection: report)
            packages.removeAll { $0.packageDirectoryURL == prepared.packageDirectoryURL }
            packages.insert(item, at: 0)
            StrideDiagnostics.shared.record(.info, stage: "IMPORT", code: "IMPORT_COMPLETE",
                "Imported \(sourceURL.lastPathComponent) as \(prepared.format.rawValue.uppercased()); DEX=\(report.dexFileCount), splits=\(prepared.splitAPKURLs.count), classes=\(report.dexClassCount)")
        } catch {
            try? fileManager.removeItem(at: prepared.packageDirectoryURL)
            StrideDiagnostics.shared.record(.error, stage: "IMPORT", code: "IMPORT_FAILED", error.localizedDescription,
                recovery: "The incomplete package was removed; existing library entries remain available")
            throw error
        }
    }

    func importPackageAsync(from sourceURL: URL) async throws {
        StrideDiagnostics.shared.record(.info, stage: "IMPORT", code: "IMPORT_START", "Starting Android package import: \(sourceURL.lastPathComponent)")
        let prepared = try await preparePackageAsync(from: sourceURL)
        do {
            let archiveInspector = inspector
            let report = try await Task.detached(priority: .userInitiated) {
                try archiveInspector.inspect(prepared.baseAPKURL)
            }.value
            let item = ImportedAPK(fileURL: prepared.baseAPKURL, packageDirectoryURL: prepared.packageDirectoryURL,
                packageFormat: prepared.format, splitAPKURLs: prepared.splitAPKURLs, inspection: report)
            packages.removeAll { $0.packageDirectoryURL == prepared.packageDirectoryURL }
            packages.insert(item, at: 0)
            StrideDiagnostics.shared.record(.info, stage: "IMPORT", code: "IMPORT_COMPLETE",
                "Imported \(sourceURL.lastPathComponent) as \(prepared.format.rawValue.uppercased()); DEX=\(report.dexFileCount), splits=\(prepared.splitAPKURLs.count), classes=\(report.dexClassCount)")
        } catch {
            try? fileManager.removeItem(at: prepared.packageDirectoryURL)
            StrideDiagnostics.shared.record(.error, stage: "IMPORT", code: "IMPORT_FAILED", error.localizedDescription,
                recovery: "The incomplete package was removed; existing library entries remain available")
            throw error
        }
    }

    func deletePackage(_ package: ImportedAPK) throws {
        try fileManager.removeItem(at: package.packageDirectoryURL)
        packages.removeAll { $0.id == package.id || $0.packageDirectoryURL == package.packageDirectoryURL }
        StrideDiagnostics.shared.record(.info, stage: "LIBRARY", code: "PACKAGE_DELETED",
            "Removed \(package.displayName) and its package components")
    }

    private func preparePackage(from sourceURL: URL) throws -> PreparedAndroidPackage {
        try packageImporter.prepare(sourceURL, in: libraryFolder())
    }

    /// ZIP extraction and component validation run away from the main actor.
    private func preparePackageAsync(from sourceURL: URL) async throws -> PreparedAndroidPackage {
        let importer = packageImporter
        let folder = try libraryFolder()
        return try await Task.detached(priority: .userInitiated) {
            try importer.prepare(sourceURL, in: folder)
        }.value
    }

    private func reload() {
        guard let folder = try? libraryFolder(),
              let children = try? fileManager.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.isDirectoryKey]) else { return }
        var restored: [ImportedAPK] = []
        for child in children {
            let isDirectory = (try? child.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) ?? false
            let base = isDirectory ? child.appendingPathComponent("base.apk") : child
            guard base.pathExtension.lowercased() == "apk", fileManager.fileExists(atPath: base.path),
                  let report = try? inspector.inspect(base) else { continue }
            let components = child.appendingPathComponent("components", isDirectory: true)
            let splits = (try? fileManager.contentsOfDirectory(at: components, includingPropertiesForKeys: nil))?
                .filter { $0.pathExtension.lowercased() == "apk" } ?? []
            let format: AndroidPackageFormat = {
                guard isDirectory,
                      let source = (try? fileManager.contentsOfDirectory(at: child, includingPropertiesForKeys: nil))?
                        .first(where: { $0.lastPathComponent.hasPrefix("source.") }),
                      let value = AndroidPackageFormat(rawValue: source.pathExtension.lowercased()) else { return .apk }
                return value
            }()
            restored.append(ImportedAPK(fileURL: base, packageDirectoryURL: isDirectory ? child : base,
                packageFormat: format, splitAPKURLs: splits, inspection: report))
        }
        packages = restored.sorted { $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending }
    }

    private func libraryFolder() throws -> URL {
        let documents = fileManager.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let folder = documents.appendingPathComponent("APKs", isDirectory: true)
        try fileManager.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }
}
