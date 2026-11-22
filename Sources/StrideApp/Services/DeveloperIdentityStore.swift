import Foundation
import Security
import CryptoKit

/// Stores an encrypted PKCS#12 container in the iOS Keychain.
/// The passphrase is deliberately never persisted; it is requested again
/// whenever the identity is loaded for a signing operation.
final class DeveloperIdentityStore {
    struct IdentitySummary: Equatable {
        let subject: String
        let sha256Fingerprint: String
    }

    enum StoreError: LocalizedError {
        case invalidPKCS12
        case identityMissing
        case certificateMissing
        case keychain(OSStatus)

        var errorDescription: String? {
            switch self {
            case .invalidPKCS12:
                return "The selected file is not a valid PKCS#12 identity or its passphrase is incorrect."
            case .identityMissing:
                return "The PKCS#12 file does not contain a private-key identity."
            case .certificateMissing:
                return "The imported identity does not contain a certificate."
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

    /// Validates the PKCS#12 before storing its encrypted bytes in Keychain.
    /// No passphrase or private-key material is written to app preferences or logs.
    @discardableResult
    func importPKCS12(_ data: Data, passphrase: String) throws -> IdentitySummary {
        let identity = try extractIdentity(from: data, passphrase: passphrase)
        let summary = try makeSummary(for: identity)

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
        return try extractIdentity(from: data, passphrase: passphrase)
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
        guard let identity = first[kSecImportItemIdentity as String] as? SecIdentity else {
            throw StoreError.identityMissing
        }
        return identity
    }

    private func makeSummary(for identity: SecIdentity) throws -> IdentitySummary {
        var certificate: SecCertificate?
        let status = SecIdentityCopyCertificate(identity, &certificate)
        guard status == errSecSuccess, let certificate else {
            throw StoreError.certificateMissing
        }
        let subject = SecCertificateCopySubjectSummary(certificate) as String? ?? "Developer identity"
        let certificateData = SecCertificateCopyData(certificate) as Data
        let digest = SHA256.hash(data: certificateData)
        let fingerprint = digest.map { String(format: "%02X", $0) }.joined(separator: ":")
        return IdentitySummary(subject: subject, sha256Fingerprint: fingerprint)
    }
}
