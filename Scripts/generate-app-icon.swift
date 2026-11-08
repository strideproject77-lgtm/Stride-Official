import AppKit
import Foundation

let canvas = 1024
let image = NSImage(size: NSSize(width: canvas, height: canvas))
image.lockFocus()

let bounds = NSRect(x: 0, y: 0, width: canvas, height: canvas)
let background = NSGradient(colors: [
    NSColor(calibratedRed: 0.025, green: 0.030, blue: 0.035, alpha: 1),
    NSColor(calibratedRed: 0.075, green: 0.090, blue: 0.078, alpha: 1)
])!
background.draw(in: bounds, angle: 45)

let inset = NSRect(x: 30, y: 30, width: 964, height: 964)
let border = NSBezierPath(roundedRect: inset, xRadius: 210, yRadius: 210)
NSColor(calibratedWhite: 1, alpha: 0.055).setStroke()
border.lineWidth = 3
border.stroke()

// Subtle speed marks behind the monogram.
let streakColor = NSColor(calibratedRed: 0.72, green: 0.96, blue: 0.30, alpha: 0.20)
streakColor.setStroke()
for index in 0..<3 {
    let streak = NSBezierPath()
    streak.move(to: NSPoint(x: 675 + index * 28, y: 250 + index * 32))
    streak.line(to: NSPoint(x: 820 + index * 28, y: 395 + index * 32))
    streak.lineWidth = 13
    streak.lineCapStyle = .round
    streak.stroke()
}

let attributes: [NSAttributedString.Key: Any] = [
    .font: NSFont.systemFont(ofSize: 735, weight: .black),
    .foregroundColor: NSColor(calibratedRed: 0.74, green: 0.97, blue: 0.31, alpha: 1)
]
let mark = "S" as NSString
let markSize = mark.size(withAttributes: attributes)
mark.draw(
    at: NSPoint(x: (CGFloat(canvas) - markSize.width) / 2 - 12,
                y: (CGFloat(canvas) - markSize.height) / 2 - 54),
    withAttributes: attributes
)

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:]) else {
    fatalError("Could not render Stride app icon")
}

let output = URL(fileURLWithPath: "Assets.xcassets/AppIcon.appiconset/StrideAppIcon.png")
try FileManager.default.createDirectory(
    at: output.deletingLastPathComponent(),
    withIntermediateDirectories: true
)
try png.write(to: output, options: .atomic)
print("Generated \(output.path) (\(png.count) bytes)")
