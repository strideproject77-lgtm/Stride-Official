import AppKit
import Foundation

// Stride app icon — Option 2: Dynamic Speed.
let canvas = 1024
let image = NSImage(size: NSSize(width: canvas, height: canvas))
image.lockFocus()

let bounds = NSRect(x: 0, y: 0, width: canvas, height: canvas)
NSGradient(colors: [
    NSColor(calibratedWhite: 0.055, alpha: 1),
    NSColor(calibratedWhite: 0.005, alpha: 1)
])!.draw(in: bounds, angle: 45)

// Restrained purple light behind the mark.
NSGradient(colors: [
    NSColor(calibratedRed: 0.34, green: 0.04, blue: 0.72, alpha: 0.20),
    NSColor(calibratedRed: 0.12, green: 0.015, blue: 0.28, alpha: 0.0)
])!.draw(in: NSRect(x: 180, y: 170, width: 680, height: 680),
         relativeCenterPosition: NSPoint(x: 0, y: 0))

// Rounded app-tile edge.
let inset = NSBezierPath(roundedRect: NSRect(x: 28, y: 28, width: 968, height: 968),
                         xRadius: 215, yRadius: 215)
NSColor(calibratedWhite: 1, alpha: 0.10).setStroke()
inset.lineWidth = 3
inset.stroke()

// Fast diagonal speed streaks behind the S.
func streak(_ points: [NSPoint], width: CGFloat, alpha: CGFloat) {
    let path = NSBezierPath()
    guard let first = points.first else { return }
    path.move(to: first)
    for point in points.dropFirst() { path.line(to: point) }
    path.lineCapStyle = .round
    path.lineJoinStyle = .round
    path.lineWidth = width
    NSColor(calibratedRed: 0.58, green: 0.12, blue: 0.98, alpha: alpha).setStroke()
    path.stroke()
}
streak([NSPoint(x: 650, y: 790), NSPoint(x: 820, y: 930)], width: 15, alpha: 0.95)
streak([NSPoint(x: 700, y: 755), NSPoint(x: 900, y: 915)], width: 7, alpha: 0.72)
streak([NSPoint(x: 130, y: 180), NSPoint(x: 330, y: 355)], width: 14, alpha: 0.90)
streak([NSPoint(x: 105, y: 125), NSPoint(x: 250, y: 255)], width: 7, alpha: 0.65)

// Bold, angular S mark, matching the second reference icon.
let mark = NSBezierPath()
mark.move(to: NSPoint(x: 690, y: 690))
mark.line(to: NSPoint(x: 445, y: 690))
mark.curve(to: NSPoint(x: 365, y: 585),
           controlPoint1: NSPoint(x: 365, y: 690),
           controlPoint2: NSPoint(x: 330, y: 625))
mark.line(to: NSPoint(x: 625, y: 470))
mark.curve(to: NSPoint(x: 690, y: 365),
           controlPoint1: NSPoint(x: 715, y: 425),
           controlPoint2: NSPoint(x: 690, y: 365))
mark.line(to: NSPoint(x: 405, y: 365))
mark.lineCapStyle = .round
mark.lineJoinStyle = .round
mark.lineWidth = 112

// Purple outer glow, then a white-to-violet face.
NSColor(calibratedRed: 0.52, green: 0.10, blue: 0.95, alpha: 0.38).setStroke()
let glow = mark.copy() as! NSBezierPath
glow.lineWidth = 150
glow.stroke()

NSGraphicsContext.saveGraphicsState()
mark.addClip()
NSGradient(colors: [
    NSColor(calibratedRed: 0.98, green: 0.98, blue: 1.0, alpha: 1),
    NSColor(calibratedRed: 0.88, green: 0.80, blue: 1.0, alpha: 1),
    NSColor(calibratedRed: 0.56, green: 0.12, blue: 0.98, alpha: 1)
])!.draw(in: bounds, angle: 90)
NSGraphicsContext.restoreGraphicsState()

// Small speed cuts at the lower-left and upper-right of the mark.
streak([NSPoint(x: 300, y: 300), NSPoint(x: 455, y: 430)], width: 10, alpha: 0.95)
streak([NSPoint(x: 580, y: 600), NSPoint(x: 735, y: 735)], width: 8, alpha: 0.90)

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:]) else {
    fatalError("Could not render Stride Dynamic Speed app icon")
}

let output = URL(fileURLWithPath: "Assets.xcassets/AppIcon.appiconset/StrideAppIcon.png")
try FileManager.default.createDirectory(
    at: output.deletingLastPathComponent(),
    withIntermediateDirectories: true
)
try png.write(to: output, options: .atomic)
print("Generated selected Stride Dynamic Speed icon at \(output.path) (\(png.count) bytes)")
