import XCTest
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
