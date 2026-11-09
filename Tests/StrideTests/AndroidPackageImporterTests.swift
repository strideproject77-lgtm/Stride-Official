import XCTest
import ZIPFoundation
@testable import Stride

final class AndroidPackageImporterTests: XCTestCase {
    func testXAPKUsesManifestBaseAndKeepsEverySplit() throws {
        let root = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let archiveURL = root.appendingPathComponent("2048.xapk")
        let archive = try XCTUnwrap(Archive(url: archiveURL, accessMode: .create))
        let base = try makeAPKData()
        let language = try makeAPKData()
        let density = try makeAPKData()
        let manifest = Data(#"{"split_apks":[{"file":"com.example.apk","id":"base"},{"file":"config.en.apk","id":"config.en"},{"file":"config.xxhdpi.apk","id":"config.xxhdpi"}]}"#.utf8)
        try add(manifest, path: "manifest.json", to: archive)
        try add(base, path: "com.example.apk", to: archive)
        try add(language, path: "config.en.apk", to: archive)
        try add(density, path: "config.xxhdpi.apk", to: archive)

        let library = root.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: library, withIntermediateDirectories: true)
        let prepared = try AndroidPackageImporter().prepare(archiveURL, in: library)
        XCTAssertEqual(prepared.format, .xapk)
        XCTAssertEqual(prepared.splitAPKURLs.count, 2)
        XCTAssertEqual(try Data(contentsOf: prepared.baseAPKURL), base)
        XCTAssertTrue(prepared.splitAPKURLs.allSatisfy { FileManager.default.fileExists(atPath: $0.path) })
    }

    func testAPKSFindsBaseMasterAndPreservesOtherSplits() throws {
        let root = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let archiveURL = root.appendingPathComponent("bundle.apks")
        let archive = try XCTUnwrap(Archive(url: archiveURL, accessMode: .create))
        let base = try makeAPKData()
        try add(base, path: "splits/base-master.apk", to: archive)
        try add(try makeAPKData(), path: "splits/base-en.apk", to: archive)
        let library = root.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: library, withIntermediateDirectories: true)
        let prepared = try AndroidPackageImporter().prepare(archiveURL, in: library)
        XCTAssertEqual(prepared.format, .apks)
        XCTAssertEqual(prepared.splitAPKURLs.count, 1)
        XCTAssertEqual(try Data(contentsOf: prepared.baseAPKURL), base)
    }

    func testAPKMReadsInfoJSONBaseEntry() throws {
        let root = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let archiveURL = root.appendingPathComponent("bundle.apkm")
        let archive = try XCTUnwrap(Archive(url: archiveURL, accessMode: .create))
        let base = try makeAPKData()
        try add(Data(#"{"splits":[{"file":"base.apk","type":"base"}]}"#.utf8), path: "info.json", to: archive)
        try add(base, path: "base.apk", to: archive)
        let library = root.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: library, withIntermediateDirectories: true)
        let prepared = try AndroidPackageImporter().prepare(archiveURL, in: library)
        XCTAssertEqual(prepared.format, .apkm)
        XCTAssertEqual(try Data(contentsOf: prepared.baseAPKURL), base)
    }

    func testArchiveWithoutBaseAPKIsRejected() throws {
        let root = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let archiveURL = root.appendingPathComponent("split-only.xapk")
        let archive = try XCTUnwrap(Archive(url: archiveURL, accessMode: .create))
        try add(try makeAPKData(), path: "config.en.apk", to: archive)
        let library = root.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: library, withIntermediateDirectories: true)
        XCTAssertThrowsError(try AndroidPackageImporter().prepare(archiveURL, in: library))
    }

    private func temporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private func makeAPKData() throws -> Data {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).appendingPathExtension("apk")
        defer { try? FileManager.default.removeItem(at: url) }
        let archive = try XCTUnwrap(Archive(url: url, accessMode: .create))
        try add(Data([1, 2, 3, 4]), path: "AndroidManifest.xml", to: archive)
        return try Data(contentsOf: url)
    }

    private func add(_ data: Data, path: String, to archive: Archive) throws {
        try archive.addEntry(with: path, type: .file, uncompressedSize: UInt32(data.count)) { position, size in
            data.subdata(in: Int(position)..<(Int(position) + size))
        }
    }
}
