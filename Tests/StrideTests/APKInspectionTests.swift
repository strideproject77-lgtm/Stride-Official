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

    func testMultidexSequenceGapIsReported() {
        XCTAssertEqual(APKArchiveInspector.multidexSequenceGaps(["classes.dex", "classes3.dex"]), ["classes2.dex"])
    }

    func testContinuousMultidexSequenceHasNoGap() {
        XCTAssertTrue(APKArchiveInspector.multidexSequenceGaps(["classes.dex", "classes2.dex", "classes3.dex"]).isEmpty)
    }

    func testSecondaryDexWithoutPrimaryDoesNotInventMissingPrimaryGap() {
        XCTAssertEqual(APKArchiveInspector.multidexSequenceGaps(["classes2.dex", "classes3.dex"]), [])
    }

    func testRuntimeBackendNeverClaimsExecution() async {
        do {
            try await UnavailableExecutionBackend().launch(packageAt: URL(fileURLWithPath: "/nonexistent.apk"))
            XCTFail("An unavailable runtime must not report a successful launch.")
        } catch is RuntimeNotImplementedError {
            // Expected until a real execution backend exists.
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }
}
