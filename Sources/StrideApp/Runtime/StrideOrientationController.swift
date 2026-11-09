import UIKit

@MainActor
final class StrideOrientationController {
    static let shared = StrideOrientationController()

    private(set) var supportedMask: UIInterfaceOrientationMask = .allButUpsideDown
    private var previousMask: UIInterfaceOrientationMask?

    private init() {}

    /// Maps Android ActivityInfo.screenOrientation values to the closest
    /// iOS scene orientation policy.
    static func mask(for androidValue: Int32) -> UIInterfaceOrientationMask {
        switch androidValue {
        case 0, 6, 8, 11: // landscape, sensorLandscape, reverseLandscape, userLandscape
            return .landscape
        case 1, 7, 9, 12: // portrait, sensorPortrait, reversePortrait, userPortrait
            return .portrait
        case 5, 14: // nosensor and locked preserve the orientation at launch time
            return currentSceneMask()
        case -1, 2, 3, 4, 10, 13: // unspecified, user, behind, sensor, fullSensor, fullUser
            return .allButUpsideDown
        default:
            return .allButUpsideDown
        }
    }

    /// Captures Stride's current orientation only once per execution session.
    /// Repeated apply calls (for example, Activity recreation) must not replace
    /// the original orientation that will be restored when the session closes.
    func apply(androidValue: Int32) {
        if previousMask == nil {
            previousMask = Self.currentSceneMask()
        }
        supportedMask = Self.mask(for: androidValue)
        requestSceneGeometry()
    }

    /// Restores the orientation captured before the first APK orientation change.
    /// Safe to call from both Close and SwiftUI onDisappear.
    func reset() {
        guard let previousMask else { return }
        supportedMask = previousMask
        self.previousMask = nil
        requestSceneGeometry()
    }

    private func requestSceneGeometry() {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        if #available(iOS 16.0, *) {
            for scene in scenes {
                scene.requestGeometryUpdate(.iOS(interfaceOrientations: supportedMask)) { error in
                    NSLog("[Stride][Orientation] Scene orientation request rejected: %@", error.localizedDescription)
                }
            }
        } else {
            UIViewController.attemptRotationToDeviceOrientation()
        }
    }

    /// Reduces the current interface orientation to the portrait/landscape
    /// axis. This is the state Stride must restore after the guest session.
    static func currentSceneMask() -> UIInterfaceOrientationMask {
        guard let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first else {
            return .allButUpsideDown
        }
        switch scene.interfaceOrientation {
        case .portrait: return .portrait
        case .landscapeLeft, .landscapeRight: return .landscape
        default: return .allButUpsideDown
        }
    }
}

@MainActor
final class StrideAppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, supportedInterfaceOrientationsFor window: UIWindow?) -> UIInterfaceOrientationMask {
        StrideOrientationController.shared.supportedMask
    }
}
