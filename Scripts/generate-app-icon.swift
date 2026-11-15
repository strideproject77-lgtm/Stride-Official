import AppKit
import Foundation

// Stride identity: a polished silver S ribbon with electric violet and cyan motion.
let size = 1024
let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocus()
let bounds = NSRect(x: 0, y: 0, width: size, height: size)

// Midnight navy base with restrained blue-violet atmosphere.
NSGradient(colors: [
    NSColor(calibratedRed: 0.025, green: 0.035, blue: 0.115, alpha: 1),
    NSColor(calibratedRed: 0.004, green: 0.008, blue: 0.035, alpha: 1)
])!.draw(in: bounds, angle: 45)
NSGradient(colors: [
    NSColor(calibratedRed: 0.48, green: 0.12, blue: 0.98, alpha: 0.42),
    NSColor(calibratedRed: 0.12, green: 0.30, blue: 0.95, alpha: 0)
])!.draw(in: NSRect(x: 20, y: 500, width: 980, height: 560),
         relativeCenterPosition: NSPoint(x: -0.45, y: 0.35))
NSGradient(colors: [
    NSColor(calibratedRed: 0.02, green: 0.72, blue: 1, alpha: 0.34),
    NSColor(calibratedRed: 0.02, green: 0.22, blue: 0.85, alpha: 0)
])!.draw(in: NSRect(x: 120, y: 10, width: 900, height: 620),
         relativeCenterPosition: NSPoint(x: 0.55, y: -0.35))

let rim = NSBezierPath(roundedRect: NSRect(x: 24, y: 24, width: 976, height: 976),
                       xRadius: 220, yRadius: 220)
rim.lineWidth = 3
NSColor(calibratedRed: 0.36, green: 0.62, blue: 1, alpha: 0.24).setStroke()
rim.stroke()

// A single closed, smooth S ribbon. A filled contour avoids the clipped,
 // flat-looking joins produced by the former open stroked path.
let mark = NSBezierPath()
mark.move(to: NSPoint(x: 790, y: 790))
mark.line(to: NSPoint(x: 390, y: 790))
mark.curve(to: NSPoint(x: 300, y: 535),
           controlPoint1: NSPoint(x: 175, y: 790),
           controlPoint2: NSPoint(x: 185, y: 610))
mark.line(to: NSPoint(x: 655, y: 335))
mark.curve(to: NSPoint(x: 610, y: 245),
           controlPoint1: NSPoint(x: 735, y: 300),
           controlPoint2: NSPoint(x: 690, y: 245))
mark.line(to: NSPoint(x: 250, y: 245))
mark.line(to: NSPoint(x: 250, y: 115))
mark.line(to: NSPoint(x: 640, y: 115))
mark.curve(to: NSPoint(x: 735, y: 475),
           controlPoint1: NSPoint(x: 900, y: 115),
           controlPoint2: NSPoint(x: 920, y: 350))
mark.line(to: NSPoint(x: 385, y: 675))
mark.curve(to: NSPoint(x: 435, y: 660),
           controlPoint1: NSPoint(x: 330, y: 715),
           controlPoint2: NSPoint(x: 375, y: 660))
mark.line(to: NSPoint(x: 790, y: 660))
mark.close()

// Soft aura follows the complete silhouette.
let outerGlow = mark.copy() as! NSBezierPath
outerGlow.transform(using: AffineTransform(scaleBy: 1.035))
NSColor(calibratedRed: 0.30, green: 0.20, blue: 1, alpha: 0.30).setFill()
outerGlow.fill()
let innerGlow = mark.copy() as! NSBezierPath
NSColor(calibratedRed: 0.02, green: 0.72, blue: 1, alpha: 0.18).setFill()
innerGlow.fill()

// Silver metal face with a bright upper edge and cool shaded lower edge.
NSGraphicsContext.saveGraphicsState()
mark.addClip()
NSGradient(colors: [
    NSColor(calibratedRed: 1.0, green: 1.0, blue: 1.0, alpha: 1),
    NSColor(calibratedRed: 0.82, green: 0.88, blue: 0.98, alpha: 1),
    NSColor(calibratedRed: 0.56, green: 0.68, blue: 0.86, alpha: 1),
    NSColor(calibratedRed: 0.90, green: 0.95, blue: 1.0, alpha: 1)
])!.draw(in: bounds, angle: 90)
NSGraphicsContext.restoreGraphicsState()

// Fine metallic highlight along the upper contour.
let highlight = NSBezierPath()
highlight.move(to: NSPoint(x: 390, y: 765))
highlight.line(to: NSPoint(x: 770, y: 765))
highlight.lineCapStyle = .round
highlight.lineWidth = 9
NSColor(calibratedWhite: 1, alpha: 0.82).setStroke()
highlight.stroke()

func drawStroke(_ from: NSPoint, _ to: NSPoint, width: CGFloat, color: NSColor) {
    let path = NSBezierPath()
    path.move(to: from)
    path.line(to: to)
    path.lineCapStyle = .round
    path.lineWidth = width
    color.setStroke()
    path.stroke()
}

// Motion accents echo the violet-to-cyan sweep in the app mark.
drawStroke(NSPoint(x: 110, y: 850), NSPoint(x: 405, y: 985), width: 18,
           color: NSColor(calibratedRed: 0.66, green: 0.20, blue: 1, alpha: 0.78))
drawStroke(NSPoint(x: 700, y: 115), NSPoint(x: 930, y: 225), width: 15,
           color: NSColor(calibratedRed: 0.02, green: 0.76, blue: 1, alpha: 0.82))
drawStroke(NSPoint(x: 760, y: 80), NSPoint(x: 900, y: 145), width: 8,
           color: NSColor(calibratedRed: 0.20, green: 0.48, blue: 1, alpha: 0.55))

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
print("Generated Stride silver S icon with electric violet/cyan accents at \(output.path) (\(png.count) bytes)")
