import Foundation
import SwiftUI

@MainActor
final class APKLibraryStore: ObservableObject {
    @Published private(set) var packages: [ImportedAPK] = []

    private let inspector = APKArchiveInspector()
    private let fileManager = FileManager.default

    init() {
        reload()
    }

    func importPackage(from sourceURL: URL) throws {
        let didAccess = sourceURL.startAccessingSecurityScopedResource()
        defer {
            if didAccess { sourceURL.stopAccessingSecurityScopedResource() }
        }

        let folder = try libraryFolder()
        let destination = uniqueDestination(for: sourceURL.lastPathComponent, in: folder)
        try fileManager.copyItem(at: sourceURL, to: destination)

        do {
            let report = try inspector.inspect(destination)
            packages.removeAll { $0.fileURL == destination }
            packages.insert(ImportedAPK(fileURL: destination, inspection: report), at: 0)
        } catch {
            try? fileManager.removeItem(at: destination)
            throw error
        }
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
