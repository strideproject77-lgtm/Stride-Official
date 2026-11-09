import UIKit

/// Keeps iOS rotation under the user's device rotation setting.
///
/// Android APK screenOrientation is recorded for diagnostics, but it must not
/// lock Stride's host interface. Both Home and the full-screen execution view
/// continue to support portrait and landscape rotations.
@MainActor
final class StrideOrientationController {
    static let shared = StrideOrientationController()

    private(set) var supportedMask: UIInterfaceOrientationMask = .allButUpsideDown

    private init() {}

    /// APK orientation hints never restrict the iPhone's interface orientation.
    static func mask(for androidValue: Int32) -> UIInterfaceOrientationMask {
        _ = androidValue
        return .allButUpsideDown
    }

    func apply(androidValue: Int32) {
        _ = androidValue
        // Intentionally do not request scene geometry: iOS auto-rotation stays enabled.
    }

    func reset() {
        // No orientation override is installed, so there is nothing to restore.
    }
}

@MainActor
final class StrideAppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, supportedInterfaceOrientationsFor window: UIWindow?) -> UIInterfaceOrientationMask {
        .allButUpsideDown
    }
}
