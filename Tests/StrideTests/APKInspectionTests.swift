import XCTest
import ZIPFoundation
@testable import Stride

final class APKInspectionTests: XCTestCase {
    func testInvalidFileIsNotReportedAsRunnable() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension("apk")
        try Data("not a zip archive".utf8).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }

        XCTAssertThrowsError(try APKArchiveInspector().inspect(url))
    }

    @MainActor
    func testImportRejectsNonAPKFileBeforeCopying() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension("txt")
        try Data("not an APK".utf8).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }

        XCTAssertThrowsError(try APKLibraryStore().importPackage(from: url)) { error in
            guard let importError = error as? APKLibraryImportError,
                  case .unsupportedFile = importError else {
                return XCTFail("Expected unsupportedFile, got: \(error)")
            }
        }
    }

    func testInspectorExtractsLauncherIconFromAPKResources() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension("apk")
        defer { try? FileManager.default.removeItem(at: url) }

        let archive = try XCTUnwrap(Archive(url: url, accessMode: .create))
        let manifest = Data([0x01])
        let icon = try XCTUnwrap(Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jVZsAAAAASUVORK5CYII="))

        try archive.addEntry(
            with: "AndroidManifest.xml",
            type: .file,
            uncompressedSize: UInt32(manifest.count),
            provider: { position, size in
                manifest.subdata(in: Int(position)..<(Int(position) + size))
            }
        )
        try archive.addEntry(
            with: "res/mipmap-xhdpi/ic_launcher.png",
            type: .file,
            uncompressedSize: UInt32(icon.count),
            provider: { position, size in
                icon.subdata(in: Int(position)..<(Int(position) + size))
            }
        )

        let report = try APKArchiveInspector().inspect(url)
        XCTAssertEqual(report.iconData, icon)
    }

    func testRuntimeBackendNeverClaimsExecution() async {
        do {
            try await UnavailableExecutionBackend().launch(
                packageAt: URL(fileURLWithPath: "/nonexistent.apk")
            )
            XCTFail("An unavailable runtime must not report a successful launch.")
        } catch is RuntimeNotImplementedError {
            // Expected until a real execution backend is implemented.
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }
}
