import SwiftUI
import UniformTypeIdentifiers

/// UI for securely importing and managing a developer PKCS#12 identity.
/// The stored identity is not yet connected to an executable-code signing pipeline.
struct DeveloperIdentitySettingsView: View {
    let accent: Color
    let background: Color
    let panel: Color
    let muted: Color

    @Environment(\.dismiss) private var dismiss
    @State private var showingImporter = false
    @State private var showingRemoveConfirmation = false
    @State private var passphrase = ""
    @State private var hasStoredIdentity = false
    @State private var subject = UserDefaults.standard.string(forKey: Self.subjectKey) ?? ""
    @State private var fingerprint = UserDefaults.standard.string(forKey: Self.fingerprintKey) ?? ""
    @State private var teamIdentifier = UserDefaults.standard.string(forKey: Self.teamIdentifierKey) ?? ""
    @State private var errorMessage: String?
    @State private var statusMessage: String?

    private static let subjectKey = "stride.developerIdentity.subject"
    private static let fingerprintKey = "stride.developerIdentity.sha256"
    private static let teamIdentifierKey = "stride.developerIdentity.teamIdentifier"

    var body: some View {
        NavigationView {
            ScrollView(showsIndicators: false) {
                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 7) {
                        Text("DEVELOPER IDENTITY")
                            .font(.system(size: 10, weight: .heavy))
                            .tracking(1.3)
                            .foregroundStyle(muted)
                        Text("Import a .p12 or .pfx certificate identity. Stride validates the passphrase and stores the encrypted container in the iOS Keychain.")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(muted)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    HStack(spacing: 12) {
                        Image(systemName: hasStoredIdentity ? "checkmark.shield.fill" : "shield.lefthalf.filled")
                            .font(.system(size: 21, weight: .semibold))
                            .foregroundStyle(hasStoredIdentity ? accent : muted)
                            .frame(width: 48, height: 48)
                            .background((hasStoredIdentity ? accent : muted).opacity(0.13), in: RoundedRectangle(cornerRadius: 14))
                        VStack(alignment: .leading, spacing: 4) {
                            Text(hasStoredIdentity ? "Identity stored in Keychain" : "No identity imported")
                                .font(.system(size: 14, weight: .bold))
                                .foregroundStyle(.white)
                            Text(hasStoredIdentity ? "Encrypted PKCS#12 data is available on this device." : "Import a certificate file to register an identity.")
                                .font(.system(size: 10, weight: .medium))
                                .foregroundStyle(muted)
                        }
                        Spacer(minLength: 0)
                    }
                    .padding(14)
                    .background(panel, in: RoundedRectangle(cornerRadius: 16))
                    .overlay(RoundedRectangle(cornerRadius: 16).stroke(Color.white.opacity(0.10), lineWidth: 1))

                    if hasStoredIdentity {
                        VStack(alignment: .leading, spacing: 9) {
                            Text("CERTIFICATE DETAILS")
                                .font(.system(size: 10, weight: .heavy))
                                .tracking(1.2)
                                .foregroundStyle(muted)
                            detailRow("Subject", value: subject.isEmpty ? "Imported identity" : subject)
                            detailRow("Apple Team ID", value: teamIdentifier.isEmpty ? "Unavailable" : teamIdentifier)
                            detailRow("SHA-256", value: fingerprint.isEmpty ? "Fingerprint unavailable" : fingerprint)
                        }
                        .padding(14)
                        .background(panel, in: RoundedRectangle(cornerRadius: 16))
                        .overlay(RoundedRectangle(cornerRadius: 16).stroke(Color.white.opacity(0.10), lineWidth: 1))
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        Text("IMPORT IDENTITY")
                            .font(.system(size: 10, weight: .heavy))
                            .tracking(1.2)
                            .foregroundStyle(muted)
                        SecureField("PKCS#12 passphrase", text: $passphrase)
                            .textContentType(.password)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .padding(.horizontal, 13)
                            .frame(height: 46)
                            .background(Color.white.opacity(0.055), in: RoundedRectangle(cornerRadius: 12))
                            .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.white.opacity(0.10), lineWidth: 1))

                        Button {
                            showingImporter = true
                        } label: {
                            Label("Choose .p12 / .pfx File", systemImage: "folder")
                                .font(.system(size: 13, weight: .bold))
                                .foregroundStyle(.white)
                                .frame(maxWidth: .infinity)
                                .frame(height: 46)
                                .background(accent, in: RoundedRectangle(cornerRadius: 13))
                        }
                        .buttonStyle(.plain)
                        .disabled(passphrase.isEmpty)
                        .opacity(passphrase.isEmpty ? 0.55 : 1)
                    }
                    .padding(14)
                    .background(panel, in: RoundedRectangle(cornerRadius: 16))
                    .overlay(RoundedRectangle(cornerRadius: 16).stroke(Color.white.opacity(0.10), lineWidth: 1))

                    VStack(alignment: .leading, spacing: 7) {
                        Label("Passphrase is not saved", systemImage: "lock.fill")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(accent)
                        Text("You will need to enter it again when a future signing operation requests the identity. Never share your certificate or passphrase.")
                            .font(.system(size: 10, weight: .medium))
                            .foregroundStyle(muted)
                            .fixedSize(horizontal: false, vertical: true)
                        Text("Stride now checks that the P12 Team ID matches the provisioning profile used to install this copy of Stride. Converted-library signing is still a separate implementation step.")
                            .font(.system(size: 10, weight: .medium))
                            .foregroundStyle(muted)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(14)
                    .background(panel, in: RoundedRectangle(cornerRadius: 16))
                    .overlay(RoundedRectangle(cornerRadius: 16).stroke(Color.white.opacity(0.10), lineWidth: 1))

                    if let statusMessage {
                        Label(statusMessage, systemImage: "checkmark.circle.fill")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(accent)
                    }

                    if hasStoredIdentity {
                        Button(role: .destructive) {
                            showingRemoveConfirmation = true
                        } label: {
                            Label("Remove Stored Identity", systemImage: "trash")
                                .font(.system(size: 12, weight: .semibold))
                                .frame(maxWidth: .infinity)
                                .frame(height: 44)
                        }
                        .buttonStyle(.bordered)
                    }
                }
                .padding(18)
            }
            .background(background.ignoresSafeArea())
            .navigationTitle("Developer Identity")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("Done") { dismiss() }
                        .foregroundStyle(accent)
                }
            }
        }
        .preferredColorScheme(.dark)
        .onAppear { refreshStoredState() }
        .fileImporter(
            isPresented: $showingImporter,
            // Some Files providers do not advertise PKCS#12 UTTypes correctly.
            // Accept generic data here and validate the selected extension/content
            // through SecPKCS12Import before anything is stored.
            allowedContentTypes: [.data],
            allowsMultipleSelection: false,
            onCompletion: handleImport
        )
        .confirmationDialog("Remove the stored developer identity?", isPresented: $showingRemoveConfirmation, titleVisibility: .visible) {
            Button("Remove Identity", role: .destructive) { removeIdentity() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This removes the encrypted PKCS#12 container from this device's Keychain.")
        }
        .alert("Developer Identity", isPresented: Binding(
            get: { errorMessage != nil },
            set: { if !$0 { errorMessage = nil } }
        )) {
            Button("OK", role: .cancel) { errorMessage = nil }
        } message: {
            Text(errorMessage ?? "An unexpected error occurred.")
        }
    }

    private enum ImportFileError: LocalizedError {
        case unsupportedType
        case unreadable

        var errorDescription: String? {
            switch self {
            case .unsupportedType:
                return "Choose a .p12 or .pfx certificate file."
            case .unreadable:
                return "The selected certificate file could not be read. If it is in iCloud Drive, wait for it to finish downloading and try again."
            }
        }
    }

    private func detailRow(_ title: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title.uppercased())
                .font(.system(size: 8, weight: .heavy))
                .tracking(0.8)
                .foregroundStyle(muted)
            Text(value)
                .font(.system(size: 10, weight: .medium, design: .monospaced))
                .foregroundStyle(.white.opacity(0.9))
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func refreshStoredState() {
        hasStoredIdentity = DeveloperIdentityStore.shared.hasStoredIdentity()
        subject = UserDefaults.standard.string(forKey: Self.subjectKey) ?? ""
        fingerprint = UserDefaults.standard.string(forKey: Self.fingerprintKey) ?? ""
    }

    private func handleImport(_ result: Result<[URL], Error>) {
        switch result {
        case .failure(let error):
            errorMessage = error.localizedDescription
        case .success(let urls):
            guard let url = urls.first else { return }
            let hasAccess = url.startAccessingSecurityScopedResource()
            defer {
                if hasAccess { url.stopAccessingSecurityScopedResource() }
            }
            do {
                let ext = url.pathExtension.lowercased()
                guard ext == "p12" || ext == "pfx" else {
                    throw ImportFileError.unsupportedType
                }

                // Coordinate reads from iCloud Drive and third-party document providers.
                var coordinatedData: Data?
                var coordinationError: NSError?
                let coordinator = NSFileCoordinator()
                coordinator.coordinate(readingItemAt: url, options: .withoutChanges, error: &coordinationError) { readableURL in
                    coordinatedData = try? Data(contentsOf: readableURL, options: .mappedIfSafe)
                }
                if let coordinationError { throw coordinationError }
                guard let data = coordinatedData, !data.isEmpty else {
                    throw ImportFileError.unreadable
                }

                let summary = try DeveloperIdentityStore.shared.importPKCS12(data, passphrase: passphrase)
                subject = summary.subject
                fingerprint = summary.sha256Fingerprint
                teamIdentifier = summary.teamIdentifier
                UserDefaults.standard.set(subject, forKey: Self.subjectKey)
                UserDefaults.standard.set(fingerprint, forKey: Self.fingerprintKey)
                UserDefaults.standard.set(teamIdentifier, forKey: Self.teamIdentifierKey)
                passphrase = ""
                statusMessage = "Identity validated and securely stored."
                refreshStoredState()
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    private func removeIdentity() {
        do {
            try DeveloperIdentityStore.shared.removeIdentity()
            UserDefaults.standard.removeObject(forKey: Self.subjectKey)
            UserDefaults.standard.removeObject(forKey: Self.fingerprintKey)
            UserDefaults.standard.removeObject(forKey: Self.teamIdentifierKey)
            passphrase = ""
            statusMessage = "Stored identity removed."
            refreshStoredState()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
