import XCTest
import UIKit
@testable import Stride

@MainActor
final class StrideOrientationTests: XCTestCase {
    func testAPKOrientationNeverLocksHostPortraitOrLandscape() {
        for value: Int32 in -1...14 {
            XCTAssertEqual(StrideOrientationController.mask(for: value), .allButUpsideDown)
        }
        XCTAssertEqual(StrideOrientationController.mask(for: 99), .allButUpsideDown)
    }

    func testHostSupportsPortraitAndBothLandscapeDirections() {
        let mask = StrideOrientationController.shared.supportedMask
        XCTAssertTrue(mask.contains(.portrait))
        XCTAssertTrue(mask.contains(.landscapeLeft))
        XCTAssertTrue(mask.contains(.landscapeRight))
        XCTAssertFalse(mask.contains(.portraitUpsideDown))
    }

    func testApplyAndResetDoNotInstallAnOrientationLock() {
        StrideOrientationController.shared.apply(androidValue: 1)
        XCTAssertEqual(StrideOrientationController.shared.supportedMask, .allButUpsideDown)
        StrideOrientationController.shared.reset()
        XCTAssertEqual(StrideOrientationController.shared.supportedMask, .allButUpsideDown)
    }
}
