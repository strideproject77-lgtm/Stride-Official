import SwiftUI

struct StrideWelcomeView: View {
    @Binding var selectedModeRaw: String
    let accent: Color
    let background: Color
    let panel: Color
    let muted: Color
    let onComplete: () -> Void

    @State private var showingIdentityImporter = false
    @State private var showingP12RequiredAlert = false

    private var selectedMode: StrideExecutionMode {
        StrideExecutionMode(rawValue: selectedModeRaw) ?? .jitless
    }

    var body: some View {
        NavigationView {
            ScrollView(showsIndicators: false) {
                VStack(alignment: .leading, spacing: 20) {
                    VStack(alignment: .leading, spacing: 10) {
                        Text("WELCOME TO STRIDE")
                            .font(.system(size: 10, weight: .heavy)).tracking(1.5).foregroundStyle(accent)
                        Text("Your Android apps, in one iOS library.")
                            .font(.system(size: 29, weight: .bold, design: .rounded)).foregroundStyle(.white)
                        Text("Stride inspects Android packages and uses its experimental managed runtime to try running compatible apps. Compatibility is still in development, so not every APK or game will work.")
                            .font(.system(size: 13, weight: .medium)).foregroundStyle(muted)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    Text("HOW DO YOU WANT TO USE STRIDE?")
                        .font(.system(size: 10, weight: .heavy)).tracking(1.2).foregroundStyle(muted)

                    ForEach(StrideExecutionMode.allCases) { mode in
                        Button {
                            selectedModeRaw = mode.rawValue
                        } label: {
                            HStack(alignment: .top, spacing: 12) {
                                Image(systemName: selectedMode == mode ? "largecircle.fill.circle" : "circle")
                                    .font(.system(size: 19, weight: .semibold)).foregroundStyle(accent)
                                    .padding(.top, 1)
                                VStack(alignment: .leading, spacing: 6) {
                                    Text(mode.title).font(.system(size: 14, weight: .bold)).foregroundStyle(.white)
                                    Text(mode.detail).font(.system(size: 11, weight: .medium)).foregroundStyle(muted)
                                        .fixedSize(horizontal: false, vertical: true)
                                }
                                Spacer(minLength: 0)
                            }
                            .padding(14)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(panel, in: RoundedRectangle(cornerRadius: 15))
                            .overlay(RoundedRectangle(cornerRadius: 15).stroke(selectedMode == mode ? accent : Color.white.opacity(0.09), lineWidth: selectedMode == mode ? 1.5 : 1))
                        }
                        .buttonStyle(.plain)
                    }

                    if selectedMode == .jitlessP12 {
                        Button {
                            showingIdentityImporter = true
                        } label: {
                            Label(DeveloperIdentityStore.shared.hasStoredIdentity() ? "Manage imported P12 identity" : "Import P12 identity now", systemImage: "checkmark.shield")
                                .font(.system(size: 12, weight: .bold))
                                .foregroundStyle(.white)
                                .frame(maxWidth: .infinity).frame(height: 46)
                                .background(accent, in: RoundedRectangle(cornerRadius: 12))
                        }
                        .buttonStyle(.plain)
                    }

                    if selectedMode == .externalJIT {
                        Text("External JIT is not active yet. Stride will save your preference, but will not attempt an unauthorized JIT operation. You can switch to JIT-less mode in Settings.")
                            .font(.system(size: 11, weight: .medium)).foregroundStyle(.orange)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    Button(action: continueSetup) {
                        Text("Continue")
                            .font(.system(size: 14, weight: .bold)).foregroundStyle(Color.black)
                            .frame(maxWidth: .infinity).frame(height: 48)
                            .background(accent, in: RoundedRectangle(cornerRadius: 13))
                    }
                    .buttonStyle(.plain)
                }
                .padding(20)
                .padding(.top, 24)
            }
            .background(background.ignoresSafeArea())
            .navigationBarTitleDisplayMode(.inline)
        }
        .preferredColorScheme(.dark)
        .sheet(isPresented: $showingIdentityImporter, onDismiss: {
            if selectedMode == .jitlessP12 && DeveloperIdentityStore.shared.hasStoredIdentity() {
                onComplete()
            }
        }) {
            DeveloperIdentitySettingsView(accent: accent, background: background, panel: panel, muted: muted)
        }
        .alert("P12 identity required", isPresented: $showingP12RequiredAlert) {
            Button("Import P12") { showingIdentityImporter = true }
            Button("Choose another mode", role: .cancel) {}
        } message: {
            Text("Import and validate a P12/PFX identity before completing setup for this option.")
        }
    }

    private func continueSetup() {
        if selectedMode == .jitlessP12 && !DeveloperIdentityStore.shared.hasStoredIdentity() {
            showingP12RequiredAlert = true
            return
        }
        onComplete()
    }
}
