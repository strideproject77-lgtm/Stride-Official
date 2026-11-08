import Foundation
import Combine
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
        let diagnostics = StrideDiagnostics.shared
        diagnostics.beginSession(apkName: sourceURL.lastPathComponent)
        diagnostics.record(.info, stage: "IMPORT", code: "SOURCE_SELECTED",
                           "User selected APK: \(sourceURL.lastPathComponent)")
        let didAccess = sourceURL.startAccessingSecurityScopedResource()
        defer {
            if didAccess { sourceURL.stopAccessingSecurityScopedResource() }
        }

        var destination: URL?
        do {
            let folder = try libraryFolder()
            let target = uniqueDestination(for: sourceURL.lastPathComponent, in: folder)
            destination = target
            try fileManager.copyItem(at: sourceURL, to: target)
            diagnostics.record(.info, stage: "IMPORT", code: "COPY_OK", "APK copied into private library")
            diagnostics.record(.info, stage: "ARCHIVE", code: "SCAN_START", "Scanning ZIP entries and required APK structure")
            let report = try inspector.inspect(target)
            diagnostics.record(.info, stage: "ARCHIVE", code: "SCAN_OK", "Archive scan complete: entries=\(report.archiveEntryCount), manifest=\(report.hasAndroidManifest), dex=\(report.dexFileCount), nativeABIs=\(report.nativeABIs.joined(separator: ", "))")
            packages.removeAll { $0.fileURL == target }
            packages.insert(ImportedAPK(fileURL: target, inspection: report), at: 0)
            diagnostics.finishSession(status: "IMPORT_INSPECTION_COMPLETE")
        } catch {
            diagnostics.record(.error, stage: "IMPORT", code: "IMPORT_FAILED", error.localizedDescription,
                               recovery: "The failed APK was isolated; continue inspecting other selected APKs")
            if let destination { try? fileManager.removeItem(at: destination) }
            diagnostics.finishSession(status: "IMPORT_FAILED")
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
