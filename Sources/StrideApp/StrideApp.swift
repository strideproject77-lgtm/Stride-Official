import SwiftUI
import UIKit

@main
struct StrideApp: App {
    @UIApplicationDelegateAdaptor(StrideAppDelegate.self) private var appDelegate
    @Environment(\.scenePhase) private var scenePhase
    @StateObject private var library = APKLibraryStore()

    init() {
        _ = StrideDiagnostics.shared
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(library)
                .preferredColorScheme(.dark)
                .onChange(of: scenePhase) { phase in
                    if phase == .background || phase == .inactive {
                        StrideDiagnostics.shared.flush()
                    }
                }
        }
    }
}
