import SwiftUI
import UIKit

@main
struct StrideApp: App {
    @UIApplicationDelegateAdaptor(StrideAppDelegate.self) private var appDelegate
    @StateObject private var library = APKLibraryStore()

    init() {
        _ = StrideDiagnostics.shared
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(library)
                .preferredColorScheme(.dark)
        }
    }
}
