import XCTest
import UIKit
@testable import Stride

@MainActor
final class StrideOrientationTests: XCTestCase {
    func testAndroidFixedPortraitAndLandscapeValues() {
        XCTAssertEqual(StrideOrientationController.mask(for: 1), .portrait)
        XCTAssertEqual(StrideOrientationController.mask(for: 7), .portrait)
        XCTAssertEqual(StrideOrientationController.mask(for: 9), .portrait)
        XCTAssertEqual(StrideOrientationController.mask(for: 12), .portrait)
        XCTAssertEqual(StrideOrientationController.mask(for: 0), .landscape)
        XCTAssertEqual(StrideOrientationController.mask(for: 6), .landscape)
        XCTAssertEqual(StrideOrientationController.mask(for: 8), .landscape)
        XCTAssertEqual(StrideOrientationController.mask(for: 11), .landscape)
    }

    func testSensorAndUnspecifiedValuesAllowBothMainOrientations() {
        for value: Int32 in [-1, 2, 3, 4, 10, 13] {
            XCTAssertEqual(StrideOrientationController.mask(for: value), .allButUpsideDown)
        }
    }

    func testNoSensorAndLockedPreserveCurrentStrideOrientation() {
        let original = StrideOrientationController.currentSceneMask()
        XCTAssertEqual(StrideOrientationController.mask(for: 5), original)
        XCTAssertEqual(StrideOrientationController.mask(for: 14), original)
    }

    func testUnknownOrientationValueIsSafe() {
        XCTAssertEqual(StrideOrientationController.mask(for: 99), .allButUpsideDown)
    }
}
