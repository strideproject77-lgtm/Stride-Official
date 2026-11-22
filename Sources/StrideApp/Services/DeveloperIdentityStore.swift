import Foundation
import Security
import CryptoKit

/// Stores an encrypted PKCS#12 identity and verifies it belongs to the
/// same Apple Developer Team as the installed Stride app.
final class DeveloperIdentityStore {
    struct IdentitySummary: Equatable {
        let subject: String
        let sha256Fingerprint: String
        let teamIdentifier: String
    }

    enum StoreError: LocalizedError {
        case invalidPKCS12
        case identityMissing
        case certificateMissing
        case certificateTeamMissing
        case provisioningProfileMissing
        case invalidProvisioningProfile
        case teamMismatch(certificate: String, installed: String)
        case keychain(OSStatus)

        var errorDescription: String? {
            switch self {
            case .invalidPKCS12:
                return "The selected file is not a valid PKCS#12 identity or its passphrase is incorrect."
            case .identityMissing:
                return "The PKCS#12 file does not contain a private-key identity."
            case .certificateMissing:
                return "The imported identity does not contain a certificate."
            case .certificateTeamMissing:
                return "The certificate does not contain a valid Apple Developer Team ID."
            case .provisioningProfileMissing:
                return "Stride's embedded provisioning profile is unavailable, so its signing team cannot be verified."
            case .invalidProvisioningProfile:
                return "Stride's embedded provisioning profile could not be decoded."
            case .teamMismatch(let certificate, let installed):
                return "This P12 belongs to team \(certificate), but this Stride installation belongs to team \(installed). Import the identity used to sign this copy of Stride."
            case .keychain(let status):
                return "Keychain operation failed (status \(status))."
            }
        }
    }

    static let shared = DeveloperIdentityStore()

    private let service = "com.strideproject.stride.developer-identity"
    private let account = "primary-p12"
    private let lock = NSLock()

    private init() {}

    /// Validates the PKCS#12 and requires its certificate Team ID to match
    /// the Team ID in this app's embedded provisioning profile before storage.
    /// The passphrase and private-key material are never persisted in preferences or logs.
    @discardableResult
    func importPKCS12(_ data: Data, passphrase: String) throws -> IdentitySummary {
        let identity = try extractIdentity(from: data, passphrase: passphrase)
        let teamIdentifier = try certificateTeamIdentifier(for: identity)
        let installedTeamIdentifier = try installedAppTeamIdentifier()
        guard teamIdentifier == installedTeamIdentifier else {
            throw StoreError.teamMismatch(certificate: teamIdentifier, installed: installedTeamIdentifier)
        }
        let summary = try makeSummary(for: identity, teamIdentifier: teamIdentifier)

        lock.lock()
        defer { lock.unlock() }

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        let update: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        ]

        let status = SecItemUpdate(query as CFDictionary, update as CFDictionary)
        if status == errSecItemNotFound {
            var insert = query
            update.forEach { insert[$0.key] = $0.value }
            let addStatus = SecItemAdd(insert as CFDictionary, nil)
            guard addStatus == errSecSuccess else { throw StoreError.keychain(addStatus) }
        } else if status != errSecSuccess {
            throw StoreError.keychain(status)
        }

        return summary
    }

    /// Rehydrates the identity only for the duration of the caller's operation.
    /// Callers must not log the passphrase or export the private key.
    func loadIdentity(passphrase: String) throws -> SecIdentity {
        lock.lock()
        defer { lock.unlock() }

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status == errSecSuccess else { throw StoreError.keychain(status) }
        guard let data = result as? Data else { throw StoreError.invalidPKCS12 }
        let identity = try extractIdentity(from: data, passphrase: passphrase)

        // Re-check at use time: an app refresh can change its embedded signing team.
        let certificateTeam = try certificateTeamIdentifier(for: identity)
        let installedTeam = try installedAppTeamIdentifier()
        guard certificateTeam == installedTeam else {
            throw StoreError.teamMismatch(certificate: certificateTeam, installed: installedTeam)
        }
        return identity
    }

    func hasStoredIdentity() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: false,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]
        return SecItemCopyMatching(query as CFDictionary, nil) == errSecSuccess
    }

    func removeIdentity() throws {
        lock.lock()
        defer { lock.unlock() }
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw StoreError.keychain(status)
        }
    }

    private func extractIdentity(from data: Data, passphrase: String) throws -> SecIdentity {
        let options = [kSecImportExportPassphrase as String: passphrase] as CFDictionary
        var imported: CFArray?
        let status = SecPKCS12Import(data as CFData, options, &imported)
        guard status == errSecSuccess,
              let items = imported as? [[String: Any]],
              let first = items.first else {
            throw StoreError.invalidPKCS12
        }
        guard let identityValue = first[kSecImportItemIdentity as String] else {
            throw StoreError.identityMissing
        }
        let rawIdentity = identityValue as! CFTypeRef
        guard CFGetTypeID(rawIdentity) == SecIdentityGetTypeID() else {
            throw StoreError.identityMissing
        }
        return unsafeBitCast(rawIdentity, to: SecIdentity.self)
    }

    private func makeSummary(for identity: SecIdentity, teamIdentifier: String) throws -> IdentitySummary {
        var certificate: SecCertificate?
        let status = SecIdentityCopyCertificate(identity, &certificate)
        guard status == errSecSuccess, let certificate else {
            throw StoreError.certificateMissing
        }
        let subject = SecCertificateCopySubjectSummary(certificate) as String? ?? "Developer identity"
        let certificateData = SecCertificateCopyData(certificate) as Data
        let digest = SHA256.hash(data: certificateData)
        let fingerprint = digest.map { String(format: "%02X", $0) }.joined(separator: ":")
        return IdentitySummary(subject: subject, sha256Fingerprint: fingerprint, teamIdentifier: teamIdentifier)
    }

    /// Apple Developer signing certificates carry the Team ID in the subject's
    /// Organizational Unit (OID 2.5.4.11).
    private func certificateTeamIdentifier(for identity: SecIdentity) throws -> String {
        var certificate: SecCertificate?
        let status = SecIdentityCopyCertificate(identity, &certificate)
        guard status == errSecSuccess, let certificate else {
            throw StoreError.certificateMissing
        }
        var error: Unmanaged<CFError>?
        guard let values = SecCertificateCopyValues(
            certificate,
            [kSecOIDX509V1SubjectName] as CFArray,
            &error
        ) as? [String: Any] else {
            throw StoreError.certificateTeamMissing
        }
        let organizationalUnits = valuesForOID("2.5.4.11", in: values)
        guard let team = organizationalUnits.first(where: {
            $0.range(of: "^[A-Z0-9]{10}$", options: .regularExpression) != nil
        }) else {
            throw StoreError.certificateTeamMissing
        }
        return team
    }

    /// Reads TeamIdentifier from the CMS-wrapped embedded.mobileprovision file.
    private func installedAppTeamIdentifier() throws -> String {
        guard let profileURL = Bundle.main.url(forResource: "embedded", withExtension: "mobileprovision"),
              let profileData = try? Data(contentsOf: profileURL) else {
            throw StoreError.provisioningProfileMissing
        }

        var decoder: CMSDecoder?
        guard CMSDecoderCreate(&decoder) == errSecSuccess, let decoder else {
            throw StoreError.invalidProvisioningProfile
        }
        guard CMSDecoderUpdateMessage(decoder, profileData as CFData) == errSecSuccess,
              CMSDecoderFinalizeMessage(decoder) == errSecSuccess else {
            throw StoreError.invalidProvisioningProfile
        }
        var decodedContent: CFData?
        guard CMSDecoderCopyContent(decoder, &decodedContent) == errSecSuccess,
              let decodedContent else {
            throw StoreError.invalidProvisioningProfile
        }
        let plistData = decodedContent as Data
        guard let plist = try? PropertyListSerialization.propertyList(
            from: plistData,
            options: [],
            format: nil
        ) as? [String: Any],
        let identifiers = plist["TeamIdentifier"] as? [String],
        let team = identifiers.first,
        team.range(of: "^[A-Z0-9]{10}$", options: .regularExpression) != nil else {
            throw StoreError.invalidProvisioningProfile
        }
        return team
    }

    private func valuesForOID(_ oid: String, in value: Any) -> [String] {
        if let dictionary = value as? [String: Any] {
            var matches: [String] = []
            for (key, nested) in dictionary {
                if key == oid {
                    matches.append(contentsOf: stringLeaves(in: nested))
                } else {
                    matches.append(contentsOf: valuesForOID(oid, in: nested))
                }
            }
            return matches
        }
        if let array = value as? [Any] {
            return array.flatMap { valuesForOID(oid, in: $0) }
        }
        return []
    }

    private func stringLeaves(in value: Any) -> [String] {
        if let string = value as? String { return [string] }
        if let dictionary = value as? [String: Any] {
            return dictionary.values.flatMap { stringLeaves(in: $0) }
        }
        if let array = value as? [Any] {
            return array.flatMap { stringLeaves(in: $0) }
        }
        return []
    }
}
