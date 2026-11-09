import AppKit
import Foundation

// Stride — geometric gaming mark.
let canvas = 1024
let image = NSImage(size: NSSize(width: canvas, height: canvas))
image.lockFocus()

let bounds = NSRect(x: 0, y: 0, width: canvas, height: canvas)
NSGradient(colors: [
    NSColor(calibratedRed: 0.025, green: 0.045, blue: 0.085, alpha: 1),
    NSColor(calibratedRed: 0.055, green: 0.075, blue: 0.135, alpha: 1),
    NSColor(calibratedRed: 0.035, green: 0.025, blue: 0.095, alpha: 1)
])!.draw(in: bounds, angle: 35)

// Soft teal and violet atmosphere.
NSGradient(colors: [
    NSColor(calibratedRed: 0.02, green: 0.82, blue: 0.72, alpha: 0.22),
    NSColor(calibratedRed: 0.02, green: 0.30, blue: 0.42, alpha: 0.0)
])!.draw(in: NSRect(x: 80, y: 250, width: 650, height: 650),
         relativeCenterPosition: NSPoint(x: -0.25, y: 0.15))
NSGradient(colors: [
    NSColor(calibratedRed: 0.42, green: 0.16, blue: 0.98, alpha: 0.24),
    NSColor(calibratedRed: 0.18, green: 0.06, blue: 0.45, alpha: 0.0)
])!.draw(in: NSRect(x: 330, y: 100, width: 650, height: 650),
         relativeCenterPosition: NSPoint(x: 0.25, y: -0.15))

// Fine hexagonal frame.
let frame = NSBezierPath()
frame.move(to: NSPoint(x: 512, y: 880))
frame.line(to: NSPoint(x: 830, y: 696))
frame.line(to: NSPoint(x: 830, y: 328))
frame.line(to: NSPoint(x: 512, y: 144))
frame.line(to: NSPoint(x: 194, y: 328))
frame.line(to: NSPoint(x: 194, y: 696))
frame.close()
frame.lineJoinStyle = .round
frame.lineWidth = 24
NSColor(calibratedRed: 0.22, green: 0.88, blue: 0.80, alpha: 0.90).setStroke()
frame.stroke()

let innerFrame = NSBezierPath()
innerFrame.move(to: NSPoint(x: 512, y: 812))
innerFrame.line(to: NSPoint(x: 770, y: 664))
innerFrame.line(to: NSPoint(x: 770, y: 360))
innerFrame.line(to: NSPoint(x: 512, y: 212))
innerFrame.line(to: NSPoint(x: 254, y: 360))
innerFrame.line(to: NSPoint(x: 254, y: 664))
innerFrame.close()
innerFrame.lineWidth = 5
NSColor(calibratedWhite: 1, alpha: 0.16).setStroke()
innerFrame.stroke()

// Angular S ribbon.
let mark = NSBezierPath()
mark.move(to: NSPoint(x: 690, y: 680))
mark.line(to: NSPoint(x: 445, y: 680))
mark.curve(to: NSPoint(x: 355, y: 570),
           controlPoint1: NSPoint(x: 355, y: 680),
           controlPoint2: NSPoint(x: 320, y: 620))
mark.line(to: NSPoint(x: 625, y: 470))
mark.curve(to: NSPoint(x: 690, y: 360),
           controlPoint1: NSPoint(x: 715, y: 415),
           controlPoint2: NSPoint(x: 690, y: 360))
mark.line(to: NSPoint(x: 405, y: 360))
mark.lineCapStyle = .round
mark.lineJoinStyle = .round
mark.lineWidth = 112

let glow = mark.copy() as! NSBezierPath
glow.lineWidth = 172
NSColor(calibratedRed: 0.20, green: 0.85, blue: 0.78, alpha: 0.30).setStroke()
glow.stroke()

NSGraphicsContext.saveGraphicsState()
mark.addClip()
NSGradient(colors: [
    NSColor(calibratedRed: 0.98, green: 1.0, blue: 1.0, alpha: 1),
    NSColor(calibratedRed: 0.70, green: 0.98, blue: 0.94, alpha: 1),
    NSColor(calibratedRed: 0.42, green: 0.48, blue: 1.0, alpha: 1)
])!.draw(in: bounds, angle: 90)
NSGraphicsContext.restoreGraphicsState()

// Two small motion accents reinforce the gaming identity.
func accentLine(_ start: NSPoint, _ end: NSPoint, width: CGFloat, alpha: CGFloat) {
    let path = NSBezierPath()
    path.move(to: start)
    path.line(to: end)
    path.lineCapStyle = .round
    path.lineWidth = width
    NSColor(calibratedRed: 0.25, green: 0.94, blue: 0.82, alpha: alpha).setStroke()
    path.stroke()
}
accentLine(NSPoint(x: 285, y: 270), NSPoint(x: 390, y: 350), width: 13, alpha: 0.85)
accentLine(NSPoint(x: 635, y: 674), NSPoint(x: 740, y: 754), width: 13, alpha: 0.85)

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
print("Generated Stride geometric gaming app icon at \(output.path) (\(png.count) bytes)")
