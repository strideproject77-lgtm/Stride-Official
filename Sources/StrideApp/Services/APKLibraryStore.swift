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
        let destination = try copyPackageToLibrary(from: sourceURL)
        do {
            let report = try inspector.inspect(destination)
            packages.removeAll { $0.fileURL == destination }
            packages.insert(ImportedAPK(fileURL: destination, inspection: report), at: 0)
        } catch {
            try? fileManager.removeItem(at: destination)
            throw error
        }
    }

    /// Copies the provider document, then scans the archive away from the UI thread.
    func importPackageAsync(from sourceURL: URL) async throws {
        let destination = try copyPackageToLibrary(from: sourceURL)
        do {
            let archiveInspector = inspector
            let report = try await Task.detached(priority: .userInitiated) {
                try archiveInspector.inspect(destination)
            }.value
            packages.removeAll { $0.fileURL == destination }
            packages.insert(ImportedAPK(fileURL: destination, inspection: report), at: 0)
        } catch {
            try? fileManager.removeItem(at: destination)
            throw error
        }
    }

    private func copyPackageToLibrary(from sourceURL: URL) throws -> URL {
        guard sourceURL.pathExtension.lowercased() == "apk" else {
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
