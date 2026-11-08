import AppKit
import Foundation

// Stride app icon — Option 2: Dynamic Speed.
// Black rounded-square field, a white-to-violet speed "S", and motion streaks.
let canvas = 1024
let image = NSImage(size: NSSize(width: canvas, height: canvas))
image.lockFocus()

let bounds = NSRect(x: 0, y: 0, width: canvas, height: canvas)
NSColor(calibratedRed: 0.008, green: 0.009, blue: 0.016, alpha: 1).setFill()
bounds.fill()

// Very subtle dark graphite vignette.
NSGradient(colors: [
    NSColor(calibratedWhite: 0.10, alpha: 0.42),
    NSColor(calibratedWhite: 0.005, alpha: 0.0)
])!.draw(in: NSRect(x: 70, y: 70, width: 884, height: 884), relativeCenterPosition: NSPoint(x: 0, y: 0))

// Main dynamic S mark. The broad, angular silhouette follows option 2.
let mark = NSBezierPath()
mark.move(to: NSPoint(x: 700, y: 735))
mark.line(to: NSPoint(x: 405, y: 735))
mark.curve(to: NSPoint(x: 282, y: 625),
           controlPoint1: NSPoint(x: 332, y: 735),
           controlPoint2: NSPoint(x: 250, y: 682))
mark.curve(to: NSPoint(x: 365, y: 500),
           controlPoint1: NSPoint(x: 285, y: 555),
           controlPoint2: NSPoint(x: 315, y: 510))
mark.line(to: NSPoint(x: 625, y: 500))
mark.curve(to: NSPoint(x: 688, y: 438),
           controlPoint1: NSPoint(x: 690, y: 500),
           controlPoint2: NSPoint(x: 708, y: 462))
mark.curve(to: NSPoint(x: 600, y: 365),
           controlPoint1: NSPoint(x: 670, y: 400),
           controlPoint2: NSPoint(x: 642, y: 365))
mark.line(to: NSPoint(x: 342, y: 365))
mark.line(to: NSPoint(x: 430, y: 275))
mark.line(to: NSPoint(x: 705, y: 275))
mark.curve(to: NSPoint(x: 825, y: 405),
           controlPoint1: NSPoint(x: 780, y: 275),
           controlPoint2: NSPoint(x: 850, y: 332))
mark.curve(to: NSPoint(x: 730, y: 545),
           controlPoint1: NSPoint(x: 825, y: 478),
           controlPoint2: NSPoint(x: 790, y: 530))
mark.line(to: NSPoint(x: 470, y: 545))
mark.curve(to: NSPoint(x: 405, y: 610),
           controlPoint1: NSPoint(x: 405, y: 545),
           controlPoint2: NSPoint(x: 385, y: 590))
mark.curve(to: NSPoint(x: 490, y: 650),
           controlPoint1: NSPoint(x: 425, y: 650),
           controlPoint2: NSPoint(x: 455, y: 650))
mark.line(to: NSPoint(x: 790, y: 650))
mark.close()

let markGradient = NSGradient(colors: [
    NSColor(calibratedRed: 0.99, green: 0.99, blue: 1.0, alpha: 1),
    NSColor(calibratedRed: 0.88, green: 0.83, blue: 1.0, alpha: 1),
    NSColor(calibratedRed: 0.67, green: 0.12, blue: 0.98, alpha: 1)
])!
markGradient.draw(in: mark, angle: -35)

// Fast violet streaks: upper-right and lower-left, matching the reference.
func streak(_ points: [NSPoint], width: CGFloat, alpha: CGFloat) {
    let path = NSBezierPath()
    path.move(to: points[0])
    for point in points.dropFirst() { path.line(to: point) }
    path.lineWidth = width
    path.lineCapStyle = .round
    path.lineJoinStyle = .round
    NSColor(calibratedRed: 0.69, green: 0.16, blue: 1.0, alpha: alpha).setStroke()
    path.stroke()
}

streak([NSPoint(x: 720, y: 790), NSPoint(x: 835, y: 900)], width: 17, alpha: 0.95)
streak([NSPoint(x: 755, y: 750), NSPoint(x: 900, y: 875)], width: 9, alpha: 0.8)
streak([NSPoint(x: 285, y: 245), NSPoint(x: 165, y: 135)], width: 17, alpha: 0.95)
streak([NSPoint(x: 330, y: 270), NSPoint(x: 215, y: 165)], width: 9, alpha: 0.8)

// Fine inner edge gives the black icon a restrained premium finish.
let inset = NSBezierPath(roundedRect: NSRect(x: 24, y: 24, width: 976, height: 976),
                         xRadius: 205, yRadius: 205)
NSColor(calibratedWhite: 1, alpha: 0.075).setStroke()
inset.lineWidth = 3
inset.stroke()

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
print("Generated Stride Option 2 — Dynamic Speed icon at \(output.path) (\(png.count) bytes)")
