import Foundation
import StrideDexFFI

struct DEXInventoryCounts: Equatable {
    let classCount: UInt32
    let methodCount: UInt32
}

enum DEXInventoryError: Error {
    case invalidBuffer
    case invalidDEX
    case parserFailure
}

/// Swift-facing adapter for the Rust DEX metadata parser.
enum StrideDexInventory {
    static func inspect(_ dexData: Data) throws -> DEXInventoryCounts {
        var summary = StrideDexSummary()
        let status = dexData.withUnsafeBytes { rawBuffer -> Int32 in
            guard let bytes = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return 1
            }
            return stride_dex_inventory_counts(bytes, rawBuffer.count, &summary)
        }

        switch status {
        case 0:
            return DEXInventoryCounts(
                classCount: summary.class_count,
                methodCount: summary.method_count
            )
        case 1, 2:
            throw DEXInventoryError.invalidBuffer
        case 3:
            throw DEXInventoryError.invalidDEX
        default:
            throw DEXInventoryError.parserFailure
        }
    }
}
