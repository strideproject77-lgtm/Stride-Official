import XCTest
import UIKit
@testable import Stride

@MainActor
final class StrideOrientationTests: XCTestCase {
    func testAndroidFixedPortraitAndLandscapeValues() {
        XCTAssertEqual(StrideOrientationController.mask(for: 1), .portrait)
        XCTAssertEqual(StrideOrientationController.mask(for: 7), .portrait)
        XCTAssertEqual(StrideOrientationController.mask(for: 9), .portrait)
        XCTAssertEqual(StrideOrientationController.mask(for: 0), .landscape)
        XCTAssertEqual(StrideOrientationController.mask(for: 6), .landscape)
        XCTAssertEqual(StrideOrientationController.mask(for: 8), .landscape)
        XCTAssertEqual(StrideOrientationController.mask(for: 11), .landscape)
    }

    func testSensorAndUnspecifiedValuesAllowBothMainOrientations() {
        XCTAssertEqual(StrideOrientationController.mask(for: -1), .allButUpsideDown)
        XCTAssertEqual(StrideOrientationController.mask(for: 2), .allButUpsideDown)
        XCTAssertEqual(StrideOrientationController.mask(for: 4), .allButUpsideDown)
        XCTAssertEqual(StrideOrientationController.mask(for: 10), .allButUpsideDown)
        XCTAssertEqual(StrideOrientationController.mask(for: 13), .allButUpsideDown)
    }

    func testNoSensorUsesPortraitFallbackAndUnknownValuesAreSafe() {
        XCTAssertEqual(StrideOrientationController.mask(for: 5), .portrait)
        XCTAssertEqual(StrideOrientationController.mask(for: 99), .allButUpsideDown)
    }
}
