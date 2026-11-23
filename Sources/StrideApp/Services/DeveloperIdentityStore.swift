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

    /// Extracts the Apple Developer Team ID from the certificate subject's
    /// Organizational Unit (OID 2.5.4.11) using the X.509 DER structure.
    /// This avoids platform-specific certificate-display constants.
    private func certificateTeamIdentifier(for identity: SecIdentity) throws -> String {
        var certificate: SecCertificate?
        let status = SecIdentityCopyCertificate(identity, &certificate)
        guard status == errSecSuccess, let certificate else {
            throw StoreError.certificateMissing
        }
        let der = SecCertificateCopyData(certificate) as Data
        guard let team = organizationalUnitValues(inX509Certificate: der).first(where: isValidTeamIdentifier) else {
            throw StoreError.certificateTeamMissing
        }
        return team
    }

    /// Reads the XML plist embedded in Apple's CMS-wrapped mobileprovision.
    /// Provisioning profiles are CMS SignedData; the encapsulated plist is
    /// present as an XML payload and can be parsed without CMS private APIs.
    private func installedAppTeamIdentifier() throws -> String {
        guard let profileURL = Bundle.main.url(forResource: "embedded", withExtension: "mobileprovision"),
              let profileData = try? Data(contentsOf: profileURL) else {
            throw StoreError.provisioningProfileMissing
        }

        let opening = Data("<?xml".utf8)
        let closing = Data("</plist>".utf8)
        guard let xmlStart = profileData.range(of: opening)?.lowerBound,
              let plistEnd = profileData.range(of: closing, in: xmlStart..<profileData.endIndex)?.upperBound else {
            throw StoreError.invalidProvisioningProfile
        }
        let plistData = profileData.subdata(in: xmlStart..<plistEnd)
        guard let plist = try? PropertyListSerialization.propertyList(
            from: plistData,
            options: [],
            format: nil
        ) as? [String: Any],
        let identifiers = plist["TeamIdentifier"] as? [String],
        let team = identifiers.first,
        isValidTeamIdentifier(team) else {
            throw StoreError.invalidProvisioningProfile
        }
        return team
    }

    private func isValidTeamIdentifier(_ value: String) -> Bool {
        value.count == 10 && value.unicodeScalars.allSatisfy {
            CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789").contains($0)
        }
    }

    /// Minimal bounds-checked DER reader. Returns tag, content range and next offset.
    private func derElement(_ data: Data, at offset: Int) -> (tag: UInt8, content: Range<Int>, next: Int)? {
        guard offset >= 0, offset + 2 <= data.count else { return nil }
        let tag = data[offset]
        var cursor = offset + 1
        let firstLength = Int(data[cursor])
        cursor += 1
        let length: Int
        if firstLength & 0x80 == 0 {
            length = firstLength
        } else {
            let byteCount = firstLength & 0x7f
            guard byteCount > 0, byteCount <= 4, cursor + byteCount <= data.count else { return nil }
            var parsed = 0
            for _ in 0..<byteCount {
                parsed = (parsed << 8) | Int(data[cursor])
                cursor += 1
            }
            length = parsed
        }
        guard length >= 0, cursor <= data.count, length <= data.count - cursor else { return nil }
        return (tag, cursor..<(cursor + length), cursor + length)
    }

    private func organizationalUnitValues(inX509Certificate data: Data) -> [String] {
        guard let certificate = derElement(data, at: 0), certificate.tag == 0x30,
              let tbs = derElement(data, at: certificate.content.lowerBound), tbs.tag == 0x30 else {
            return []
        }
        var cursor = tbs.content.lowerBound
        guard cursor < tbs.content.upperBound else { return [] }

        // TBSCertificate: optional version [0], serial, signature, issuer, validity, subject.
        if let first = derElement(data, at: cursor), first.tag == 0xA0 {
            cursor = first.next
        }
        for _ in 0..<4 {
            guard let field = derElement(data, at: cursor) else { return [] }
            cursor = field.next
        }
        guard let subject = derElement(data, at: cursor), subject.tag == 0x30 else { return [] }

        var values: [String] = []
        var setCursor = subject.content.lowerBound
        while setCursor < subject.content.upperBound {
            guard let rdnSet = derElement(data, at: setCursor), rdnSet.tag == 0x31 else { break }
            var attributeCursor = rdnSet.content.lowerBound
            while attributeCursor < rdnSet.content.upperBound {
                guard let attribute = derElement(data, at: attributeCursor), attribute.tag == 0x30 else { break }
                var partCursor = attribute.content.lowerBound
                guard let oid = derElement(data, at: partCursor), oid.tag == 0x06 else {
                    attributeCursor = attribute.next
                    continue
                }
                partCursor = oid.next
                if Array(data[oid.content]) == [0x55, 0x04, 0x0B],
                   let value = derElement(data, at: partCursor) {
                    let raw = data.subdata(in: value.content)
                    let decoded = String(data: raw, encoding: .utf8)
                        ?? String(data: raw, encoding: .utf16BigEndian)
                    if let decoded { values.append(decoded) }
                }
                attributeCursor = attribute.next
            }
            setCursor = rdnSet.next
        }
        return values
    }

}}
