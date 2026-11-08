import AppKit
import Foundation

// Stride's selected "Core / Hex" icon concept (option B).
let canvas = 1024
let image = NSImage(size: NSSize(width: canvas, height: canvas))
image.lockFocus()

let bounds = NSRect(x: 0, y: 0, width: canvas, height: canvas)
let background = NSGradient(colors: [
    NSColor(calibratedRed: 0.105, green: 0.145, blue: 0.075, alpha: 1),
    NSColor(calibratedRed: 0.018, green: 0.027, blue: 0.020, alpha: 1)
])!
background.draw(in: bounds, angle: 45)

// Soft central glow keeps the mark legible at small home-screen sizes.
NSGradient(colors: [
    NSColor(calibratedRed: 0.48, green: 0.72, blue: 0.25, alpha: 0.20),
    NSColor(calibratedRed: 0.12, green: 0.22, blue: 0.10, alpha: 0.0)
])!.draw(in: NSRect(x: 170, y: 170, width: 684, height: 684), relativeCenterPosition: NSPoint(x: 0, y: 0))

// Subtle rounded inset border.
let inset = NSBezierPath(roundedRect: NSRect(x: 28, y: 28, width: 968, height: 968),
                         xRadius: 215, yRadius: 215)
NSColor(calibratedWhite: 1, alpha: 0.055).setStroke()
inset.lineWidth = 3
inset.stroke()

// Technical hexagon outline.
let center = NSPoint(x: 512, y: 512)
let radius: CGFloat = 355
let hexagon = NSBezierPath()
for index in 0..<6 {
    let angle = CGFloat(index) * .pi / 3 - .pi / 2
    let point = NSPoint(x: center.x + cos(angle) * radius,
                        y: center.y + sin(angle) * radius)
    if index == 0 { hexagon.move(to: point) } else { hexagon.line(to: point) }
}
hexagon.close()
NSColor(calibratedRed: 0.77, green: 0.96, blue: 0.48, alpha: 0.16).setStroke()
let glow = hexagon.copy() as! NSBezierPath
glow.lineWidth = 42
glow.stroke()
NSColor(calibratedRed: 0.77, green: 0.96, blue: 0.48, alpha: 1).setStroke()
hexagon.lineWidth = 25
hexagon.lineJoinStyle = .round
hexagon.stroke()

// Bold, angular Stride core mark based on the selected second concept.
let mark = NSBezierPath()
let points: [NSPoint] = [
    NSPoint(x: 315, y: 675),
    NSPoint(x: 710, y: 675),
    NSPoint(x: 405, y: 350),
    NSPoint(x: 315, y: 350),
    NSPoint(x: 410, y: 485),
    NSPoint(x: 275, y: 485),
    NSPoint(x: 355, y: 585),
    NSPoint(x: 510, y: 585),
    NSPoint(x: 555, y: 630),
    NSPoint(x: 315, y: 630)
]
mark.move(to: points[0])
for point in points.dropFirst() { mark.line(to: point) }
mark.close()
NSColor.white.setFill()
mark.fill()

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:]) else {
    fatalError("Could not render Stride Core / Hex app icon")
}

let output = URL(fileURLWithPath: "Assets.xcassets/AppIcon.appiconset/StrideAppIcon.png")
try FileManager.default.createDirectory(
    at: output.deletingLastPathComponent(),
    withIntermediateDirectories: true
)
try png.write(to: output, options: .atomic)
print("Generated selected Stride Core / Hex icon at \(output.path) (\(png.count) bytes)")
