import XCTest
@testable import Stride

final class StrideDexInventoryTests: XCTestCase {
    func testReadsClassAndMethodCountsThroughRustFFI() throws {
        let counts = try StrideDexInventory.inspect(minimalDEXFixture())
        XCTAssertEqual(counts.classCount, 1)
        XCTAssertEqual(counts.methodCount, 1)
    }

    func testRejectsMalformedDEXThroughRustFFI() {
        XCTAssertThrowsError(try StrideDexInventory.inspect(Data(repeating: 0, count: 8)))
    }

    func testRejectsEmptyBufferWithoutCallingRustWithNullPointer() {
        XCTAssertThrowsError(try StrideDexInventory.inspect(Data()))
    }

    private func minimalDEXFixture() -> Data {
        let stringsOffset = 0x70
        let typesOffset = stringsOffset + 8
        let protosOffset = typesOffset + 8
        let methodsOffset = protosOffset + 12
        let definitionsOffset = methodsOffset + 8
        let classDataOffset = definitionsOffset + 32
        let classData: [UInt8] = [0, 0, 1, 0, 0, 1, 0]
        let stringData: [UInt8] = [3, 0x4c, 0x78, 0x3b, 0, 3, 0x66, 0x6f, 0x6f, 0]
        let stringDataOffset = classDataOffset + classData.count
        var bytes = Data(repeating: 0, count: stringDataOffset + stringData.count)

        bytes.replaceSubrange(0..<8, with: Array("dex\n039\0".utf8))
        write32(UInt32(bytes.count), at: 0x20, into: &bytes)
        write32(0x70, at: 0x24, into: &bytes)
        write32(0x1234_5678, at: 0x28, into: &bytes)

        let tables: [(Int, UInt32, Int, UInt32)] = [
            (0x38, 2, 0x3c, UInt32(stringsOffset)),
            (0x40, 2, 0x44, UInt32(typesOffset)),
            (0x48, 1, 0x4c, UInt32(protosOffset)),
            (0x50, 0, 0x54, 0),
            (0x58, 1, 0x5c, UInt32(methodsOffset)),
            (0x60, 1, 0x64, UInt32(definitionsOffset))
        ]
        for (countOffset, count, offsetOffset, offset) in tables {
            write32(count, at: countOffset, into: &bytes)
            write32(offset, at: offsetOffset, into: &bytes)
        }

        write32(UInt32(stringDataOffset), at: stringsOffset, into: &bytes)
        write32(UInt32(stringDataOffset + 5), at: stringsOffset + 4, into: &bytes)
        write32(0, at: typesOffset, into: &bytes)
        write32(0, at: typesOffset + 4, into: &bytes)
        write32(0, at: protosOffset, into: &bytes)
        write32(0, at: protosOffset + 4, into: &bytes)
        write32(0, at: protosOffset + 8, into: &bytes)
        write16(0, at: methodsOffset, into: &bytes)
        write16(0, at: methodsOffset + 2, into: &bytes)
        write32(1, at: methodsOffset + 4, into: &bytes)
        write32(0, at: definitionsOffset, into: &bytes)
        write32(UInt32.max, at: definitionsOffset + 8, into: &bytes)
        write32(UInt32(classDataOffset), at: definitionsOffset + 24, into: &bytes)
        bytes.replaceSubrange(classDataOffset..<(classDataOffset + classData.count), with: classData)
        bytes.replaceSubrange(stringDataOffset..<(stringDataOffset + stringData.count), with: stringData)
        return bytes
    }

    private func write16(_ value: UInt16, at offset: Int, into data: inout Data) {
        data[offset] = UInt8(value & 0xff)
        data[offset + 1] = UInt8(value >> 8)
    }

    private func write32(_ value: UInt32, at offset: Int, into data: inout Data) {
        for byte in 0..<4 {
            data[offset + byte] = UInt8((value >> UInt32(byte * 8)) & 0xff)
        }
    }
}
