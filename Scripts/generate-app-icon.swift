import AppKit
import Foundation

// Stride app icon: minimal, high-contrast gaming identity.
let size = 1024
let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocus()
let bounds = NSRect(x: 0, y: 0, width: size, height: size)

// Deep graphite background with a restrained green glow.
NSGradient(colors: [
    NSColor(calibratedRed: 0.035, green: 0.045, blue: 0.055, alpha: 1),
    NSColor(calibratedRed: 0.010, green: 0.018, blue: 0.022, alpha: 1)
])!.draw(in: bounds, angle: 45)
NSGradient(colors: [
    NSColor(calibratedRed: 0.55, green: 1.0, blue: 0.12, alpha: 0.20),
    NSColor(calibratedRed: 0.20, green: 0.45, blue: 0.06, alpha: 0)
])!.draw(in: NSRect(x: 180, y: 170, width: 680, height: 680),
         relativeCenterPosition: NSPoint(x: 0, y: 0))

// Subtle inset edge, not a separate badge or symbol container.
let edge = NSBezierPath(roundedRect: NSRect(x: 25, y: 25, width: 974, height: 974),
                        xRadius: 220, yRadius: 220)
edge.lineWidth = 3
NSColor(calibratedWhite: 1, alpha: 0.08).setStroke()
edge.stroke()

// A single bold, custom angular S.
let mark = NSBezierPath()
mark.move(to: NSPoint(x: 720, y: 700))
mark.line(to: NSPoint(x: 430, y: 700))
mark.curve(to: NSPoint(x: 340, y: 590),
           controlPoint1: NSPoint(x: 340, y: 700),
           controlPoint2: NSPoint(x: 305, y: 635))
mark.line(to: NSPoint(x: 625, y: 465))
mark.curve(to: NSPoint(x: 690, y: 350),
           controlPoint1: NSPoint(x: 715, y: 410),
           controlPoint2: NSPoint(x: 690, y: 350))
mark.line(to: NSPoint(x: 390, y: 350))
mark.lineCapStyle = .round
mark.lineJoinStyle = .round
mark.lineWidth = 118

let glow = mark.copy() as! NSBezierPath
glow.lineWidth = 174
NSColor(calibratedRed: 0.55, green: 1.0, blue: 0.10, alpha: 0.28).setStroke()
glow.stroke()

NSGraphicsContext.saveGraphicsState()
mark.addClip()
NSGradient(colors: [
    NSColor(calibratedWhite: 1.0, alpha: 1),
    NSColor(calibratedRed: 0.82, green: 1.0, blue: 0.55, alpha: 1),
    NSColor(calibratedRed: 0.52, green: 0.94, blue: 0.08, alpha: 1)
])!.draw(in: bounds, angle: 90)
NSGraphicsContext.restoreGraphicsState()

// Two short lime motion accents; keep the mark clean at small icon sizes.
func drawAccent(_ from: NSPoint, _ to: NSPoint, alpha: CGFloat) {
    let path = NSBezierPath()
    path.move(to: from)
    path.line(to: to)
    path.lineCapStyle = .round
    path.lineWidth = 13
    NSColor(calibratedRed: 0.68, green: 1.0, blue: 0.20, alpha: alpha).setStroke()
    path.stroke()
}
drawAccent(NSPoint(x: 235, y: 285), NSPoint(x: 325, y: 355), alpha: 0.85)
drawAccent(NSPoint(x: 700, y: 665), NSPoint(x: 790, y: 735), alpha: 0.85)

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
print("Generated minimal Stride gaming app icon at \(output.path) (\(png.count) bytes)")
