import XCTest
@testable import Stride

final class StrideDiagnosticsTests: XCTestCase {
    func testDiagnosticEntryContainsTimestampSequenceAndStage() {
        let entry = StrideDiagnostics.Entry(
            id: UUID(),
            timestamp: Date(timeIntervalSince1970: 1_800_000_000),
            sequence: 42,
            sessionID: "session-12345678",
            uptimeSeconds: 123.5,
            severity: .error,
            stage: "DEX",
            code: "UNSUPPORTED_OPCODE",
            message: "Interpreter rejected opcode",
            recovery: "Check opcode coverage"
        )
        XCTAssertTrue(entry.line.contains("[#42]"))
        XCTAssertTrue(entry.line.contains("[S:session-"))
        XCTAssertTrue(entry.line.contains("[ERROR] [DEX] [UNSUPPORTED_OPCODE]"))
        XCTAssertTrue(entry.line.contains("recovery: Check opcode coverage"))
    }

    func testDiagnosticEntryEncodesAsStructuredJSON() throws {
        let entry = StrideDiagnostics.Entry(
            id: UUID(),
            timestamp: Date(),
            sequence: 7,
            sessionID: "session-json",
            uptimeSeconds: 9.25,
            severity: .warning,
            stage: "ELF",
            code: "UNRESOLVED_IMPORT",
            message: "Missing symbol",
            recovery: nil
        )
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        let data = try encoder.encode(entry)
        let decoded = try JSONDecoder().decode(StrideDiagnostics.Entry.self, from: data)
        XCTAssertEqual(decoded.sequence, 7)
        XCTAssertEqual(decoded.sessionID, "session-json")
        XCTAssertEqual(decoded.stage, "ELF")
        XCTAssertEqual(decoded.code, "UNRESOLVED_IMPORT")
    }
}
