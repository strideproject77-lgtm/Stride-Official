import AppKit
import Foundation

// Stride identity: a dimensional, forward-moving S mark on a midnight graphite field.
let size = 1024
let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocus()
let bounds = NSRect(x: 0, y: 0, width: size, height: size)

// Deep midnight blue with a subtle warm lower glow.
NSGradient(colors: [
    NSColor(calibratedRed: 0.035, green: 0.055, blue: 0.105, alpha: 1),
    NSColor(calibratedRed: 0.012, green: 0.020, blue: 0.045, alpha: 1)
])!.draw(in: bounds, angle: 45)
NSGradient(colors: [
    NSColor(calibratedRed: 0.10, green: 0.62, blue: 0.82, alpha: 0.26),
    NSColor(calibratedRed: 0.10, green: 0.28, blue: 0.52, alpha: 0)
])!.draw(in: NSRect(x: 120, y: 130, width: 790, height: 790),
         relativeCenterPosition: NSPoint(x: -0.25, y: -0.25))

// Fine inset rim gives the icon definition without adding a badge.
let rim = NSBezierPath(roundedRect: NSRect(x: 24, y: 24, width: 976, height: 976),
                       xRadius: 220, yRadius: 220)
rim.lineWidth = 3
NSColor(calibratedRed: 0.55, green: 0.82, blue: 1, alpha: 0.16).setStroke()
rim.stroke()

// Custom continuous S ribbon with a forward-pointing terminal.
let mark = NSBezierPath()
mark.move(to: NSPoint(x: 730, y: 715))
mark.line(to: NSPoint(x: 420, y: 715))
mark.curve(to: NSPoint(x: 345, y: 590),
           controlPoint1: NSPoint(x: 345, y: 715),
           controlPoint2: NSPoint(x: 300, y: 650))
mark.line(to: NSPoint(x: 650, y: 455))
mark.curve(to: NSPoint(x: 580, y: 310),
           controlPoint1: NSPoint(x: 735, y: 375),
           controlPoint2: NSPoint(x: 690, y: 310))
mark.line(to: NSPoint(x: 315, y: 310))
mark.lineCapStyle = .round
mark.lineJoinStyle = .round
mark.lineWidth = 112

let shadow = mark.copy() as! NSBezierPath
shadow.transform(using: AffineTransform(translationBy: NSPoint(x: 0, y: -15)))
shadow.lineWidth = 174
NSColor(calibratedRed: 0.02, green: 0.55, blue: 0.92, alpha: 0.30).setStroke()
shadow.stroke()

NSGraphicsContext.saveGraphicsState()
mark.addClip()
NSGradient(colors: [
    NSColor(calibratedRed: 0.78, green: 0.96, blue: 1, alpha: 1),
    NSColor(calibratedRed: 0.24, green: 0.80, blue: 0.96, alpha: 1),
    NSColor(calibratedRed: 0.22, green: 0.48, blue: 0.98, alpha: 1)
])!.draw(in: bounds, angle: 90)
NSGraphicsContext.restoreGraphicsState()

// Small speed strokes reinforce the forward-motion identity.
func drawStroke(_ from: NSPoint, _ to: NSPoint, width: CGFloat, alpha: CGFloat) {
    let path = NSBezierPath()
    path.move(to: from)
    path.line(to: to)
    path.lineCapStyle = .round
    path.lineWidth = width
    NSColor(calibratedRed: 0.35, green: 0.83, blue: 1, alpha: alpha).setStroke()
    path.stroke()
}
drawStroke(NSPoint(x: 700, y: 245), NSPoint(x: 820, y: 245), width: 12, alpha: 0.80)
drawStroke(NSPoint(x: 740, y: 205), NSPoint(x: 855, y: 205), width: 8, alpha: 0.48)

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:]) else {
    fatalError("Could not render Stride app icon")
}
let output = URL(fileURLWithPath: "Assets.xcassets/AppIcon.appiconset/StrideAppIcon.png")
try FileManager.default.createDirectory(at: output.deletingLastPathComponent(),
                                        withIntermediateDirectories: true)
try png.write(to: output, options: .atomic)
print("Generated Stride dimensional blue motion icon at \(output.path) (\(png.count) bytes)")
