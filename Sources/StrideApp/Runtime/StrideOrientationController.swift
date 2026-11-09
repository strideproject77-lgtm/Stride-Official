import UIKit

@MainActor
final class StrideOrientationController {
    static let shared = StrideOrientationController()
    private(set) var supportedMask: UIInterfaceOrientationMask = .allButUpsideDown
    private init() {}

    static func mask(for androidValue: Int32) -> UIInterfaceOrientationMask {
        switch androidValue {
        case 0, 6, 8, 11: return .landscape
        case 1, 7, 9, 12: return .portrait
        case 5: return .portrait
        case 14: return currentSceneMask()
        case -1, 2, 3, 4, 10, 13: return .allButUpsideDown
        default: return .allButUpsideDown
        }
    }

    func apply(androidValue: Int32) {
        supportedMask = Self.mask(for: androidValue)
        requestSceneGeometry()
    }

    func reset() {
        supportedMask = .allButUpsideDown
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

    private static func currentSceneMask() -> UIInterfaceOrientationMask {
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
