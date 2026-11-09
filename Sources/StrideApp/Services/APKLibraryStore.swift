import Foundation
import Combine
import SwiftUI

enum APKLibraryImportError: LocalizedError {
    case unsupportedFile
    case unreadableSource

    var errorDescription: String? {
        switch self {
        case .unsupportedFile:
            return "Please choose an .apk file. Other file types cannot be imported."
        case .unreadableSource:
            return "iOS could not access this file. Save it in Files and try importing it again."
        }
    }
}

@MainActor
final class APKLibraryStore: ObservableObject {
    @Published private(set) var packages: [ImportedAPK] = []

    private let inspector = APKArchiveInspector()
    private let fileManager = FileManager.default

    init() {
        reload()
    }

    func importPackage(from sourceURL: URL) throws {
        StrideDiagnostics.shared.record(.info, stage: "IMPORT", code: "IMPORT_START",
            "Starting APK import: \(sourceURL.lastPathComponent)")
        let destination = try copyPackageToLibrary(from: sourceURL)
        do {
            let report = try inspector.inspect(destination)
            packages.removeAll { $0.fileURL == destination }
            packages.insert(ImportedAPK(fileURL: destination, inspection: report), at: 0)
            StrideDiagnostics.shared.record(.info, stage: "IMPORT", code: "IMPORT_COMPLETE",
                "Imported \(sourceURL.lastPathComponent): \(report.dexFileCount) DEX, \(report.dexClassCount) classes, \(report.dexMethodCount) methods")
        } catch {
            StrideDiagnostics.shared.record(.error, stage: "IMPORT", code: "IMPORT_FAILED",
                error.localizedDescription, recovery: "This APK was isolated and removed from the library; other packages remain available")
            try? fileManager.removeItem(at: destination)
            throw error
        }
    }

    /// Copies the provider document, then scans the archive away from the UI thread.
    func importPackageAsync(from sourceURL: URL) async throws {
        StrideDiagnostics.shared.record(.info, stage: "IMPORT", code: "IMPORT_START",
            "Starting APK import: \(sourceURL.lastPathComponent)")
        let destination = try copyPackageToLibrary(from: sourceURL)
        do {
            let archiveInspector = inspector
            let report = try await Task.detached(priority: .userInitiated) {
                try archiveInspector.inspect(destination)
            }.value
            packages.removeAll { $0.fileURL == destination }
            packages.insert(ImportedAPK(fileURL: destination, inspection: report), at: 0)
            StrideDiagnostics.shared.record(.info, stage: "IMPORT", code: "IMPORT_COMPLETE",
                "Imported \(sourceURL.lastPathComponent): \(report.dexFileCount) DEX, \(report.dexClassCount) classes, \(report.dexMethodCount) methods")
        } catch {
            StrideDiagnostics.shared.record(.error, stage: "IMPORT", code: "IMPORT_FAILED",
                error.localizedDescription, recovery: "This APK was isolated and removed from the library; other packages remain available")
            try? fileManager.removeItem(at: destination)
            throw error
        }
    }

    func deletePackage(_ package: ImportedAPK) throws {
        guard package.fileURL.pathExtension.lowercased() == "apk" else {
            throw APKLibraryImportError.unreadableSource
        }
        try fileManager.removeItem(at: package.fileURL)
        packages.removeAll { $0.id == package.id || $0.fileURL == package.fileURL }
        StrideDiagnostics.shared.record(.info, stage: "LIBRARY", code: "APK_DELETED",
            "Removed \(package.displayName) from the local APK library")
    }

    private func copyPackageToLibrary(from sourceURL: URL) throws -> URL {
        guard sourceURL.pathExtension.lowercased() == "apk" else {
            StrideDiagnostics.shared.record(.warning, stage: "IMPORT", code: "UNSUPPORTED_FILE",
                "Rejected non-APK file: \(sourceURL.lastPathComponent)",
                recovery: "Select an .apk file; XAPK/APKS bundle support is not enabled in this import path yet")
            throw APKLibraryImportError.unsupportedFile
        }

        let didAccess = sourceURL.startAccessingSecurityScopedResource()
        defer {
            if didAccess { sourceURL.stopAccessingSecurityScopedResource() }
        }

        guard sourceURL.isFileURL,
              fileManager.isReadableFile(atPath: sourceURL.path) else {
            throw APKLibraryImportError.unreadableSource
        }

        let folder = try libraryFolder()
        let destination = uniqueDestination(for: sourceURL.lastPathComponent, in: folder)

        // UIDocumentPicker is configured with asCopy=true, so provider-backed
        // files arrive as a local sandbox copy. Copy directly while any optional
        // security-scoped access is active; coordinating the already-local copy
        // can cause a successful picker selection to produce no library item.
        do {
            try fileManager.copyItem(at: sourceURL, to: destination)
        } catch {
            try? fileManager.removeItem(at: destination)
            throw error
        }

        guard fileManager.fileExists(atPath: destination.path),
              fileManager.isReadableFile(atPath: destination.path) else {
            try? fileManager.removeItem(at: destination)
            throw APKLibraryImportError.unreadableSource
        }
        return destination
    }

    private func reload() {
        guard let folder = try? libraryFolder(),
              let files = try? fileManager.contentsOfDirectory(
                at: folder,
                includingPropertiesForKeys: nil
              ) else { return }

        packages = files
            .filter { $0.pathExtension.lowercased() == "apk" }
            .compactMap { url in
                guard let report = try? inspector.inspect(url) else { return nil }
                return ImportedAPK(fileURL: url, inspection: report)
            }
            .sorted { $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending }
    }

    private func libraryFolder() throws -> URL {
        let documents = fileManager.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let folder = documents.appendingPathComponent("APKs", isDirectory: true)
        try fileManager.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    private func uniqueDestination(for filename: String, in folder: URL) -> URL {
        let original = folder.appendingPathComponent(filename)
        guard fileManager.fileExists(atPath: original.path) else { return original }
        let base = original.deletingPathExtension().lastPathComponent
        let ext = original.pathExtension
        var suffix = 2
        while true {
            let candidate = folder.appendingPathComponent("\(base)-\(suffix).\(ext)")
            if !fileManager.fileExists(atPath: candidate.path) { return candidate }
            suffix += 1
        }
    }
}
