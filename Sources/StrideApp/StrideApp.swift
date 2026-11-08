import SwiftUI

@main
struct StrideApp: App {
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
