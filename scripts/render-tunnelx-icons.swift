import AppKit
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let assetsDir = root.appendingPathComponent("src/assets")
let iconsDir = root.appendingPathComponent("src-tauri/icons")

func color(_ hex: UInt32, alpha: CGFloat = 1) -> CGColor {
    CGColor(
        red: CGFloat((hex >> 16) & 0xff) / 255,
        green: CGFloat((hex >> 8) & 0xff) / 255,
        blue: CGFloat(hex & 0xff) / 255,
        alpha: alpha
    )
}

func gradient(_ stops: [(CGFloat, CGColor)]) -> CGGradient {
    CGGradient(
        colorsSpace: CGColorSpaceCreateDeviceRGB(),
        colors: stops.map(\.1) as CFArray,
        locations: stops.map(\.0)
    )!
}

func pngData(from image: CGImage) -> Data {
    let data = NSMutableData()
    let dest = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(dest, image, nil)
    CGImageDestinationFinalize(dest)
    return data as Data
}

func writePNG(_ image: CGImage, to url: URL) throws {
    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try pngData(from: image).write(to: url)
}

func loadPNG(_ url: URL) -> CGImage? {
    guard
        let source = CGImageSourceCreateWithURL(url as CFURL, nil),
        let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
    else {
        return nil
    }
    return image
}

func drawSolidStrokePath(_ ctx: CGContext, path: CGPath, width: CGFloat, color: CGColor) {
    ctx.saveGState()
    ctx.addPath(path)
    ctx.setStrokeColor(color)
    ctx.setLineWidth(width)
    ctx.setLineCap(.round)
    ctx.strokePath()
    ctx.restoreGState()
}

func drawSolidStroke(_ ctx: CGContext, from: CGPoint, to: CGPoint, width: CGFloat, color: CGColor) {
    let path = CGMutablePath()
    path.move(to: from)
    path.addLine(to: to)
    drawSolidStrokePath(ctx, path: path, width: width, color: color)
}

func drawGradientStrokePath(
    _ ctx: CGContext,
    path: CGPath,
    width: CGFloat,
    gradient: CGGradient,
    start: CGPoint,
    end: CGPoint,
    alpha: CGFloat = 1
) {
    ctx.saveGState()
    ctx.setAlpha(alpha)
    ctx.addPath(path)
    ctx.setLineWidth(width)
    ctx.setLineCap(.round)
    ctx.setLineJoin(.round)
    ctx.replacePathWithStrokedPath()
    ctx.clip()
    ctx.drawLinearGradient(gradient, start: start, end: end, options: [])
    ctx.restoreGState()
}

func drawCircle(
    _ ctx: CGContext,
    center: CGPoint,
    radius: CGFloat,
    fill: CGColor,
    shadow: (CGSize, CGFloat, CGColor)? = nil
) {
    ctx.saveGState()
    if let shadow {
        ctx.setShadow(offset: shadow.0, blur: shadow.1, color: shadow.2)
    }
    ctx.addEllipse(in: CGRect(
        x: center.x - radius,
        y: center.y - radius,
        width: radius * 2,
        height: radius * 2
    ))
    ctx.setFillColor(fill)
    ctx.fillPath()
    ctx.restoreGState()
}

func fillPaths(_ ctx: CGContext, paths: [CGPath], color: CGColor) {
    ctx.saveGState()
    for path in paths {
        ctx.addPath(path)
    }
    ctx.setFillColor(color)
    ctx.fillPath()
    ctx.restoreGState()
}

func fillGradientShapes(
    _ ctx: CGContext,
    paths: [CGPath],
    gradient: CGGradient,
    start: CGPoint,
    end: CGPoint
) {
    ctx.saveGState()
    for path in paths {
        ctx.addPath(path)
    }
    ctx.clip()
    ctx.drawLinearGradient(gradient, start: start, end: end, options: [])
    ctx.restoreGState()
}

func fillGradientPath(
    _ ctx: CGContext,
    path: CGPath,
    gradient: CGGradient,
    start: CGPoint,
    end: CGPoint,
    alpha: CGFloat = 1
) {
    ctx.saveGState()
    ctx.setAlpha(alpha)
    ctx.addPath(path)
    ctx.clip()
    ctx.drawLinearGradient(gradient, start: start, end: end, options: [])
    ctx.restoreGState()
}

func fillEvenOdd(_ ctx: CGContext, paths: [CGPath], color: CGColor) {
    ctx.saveGState()
    for path in paths {
        ctx.addPath(path)
    }
    ctx.setFillColor(color)
    ctx.fillPath(using: .evenOdd)
    ctx.restoreGState()
}

func fillEvenOddGradient(
    _ ctx: CGContext,
    paths: [CGPath],
    gradient: CGGradient,
    start: CGPoint,
    end: CGPoint
) {
    ctx.saveGState()
    for path in paths {
        ctx.addPath(path)
    }
    ctx.clip(using: .evenOdd)
    ctx.drawLinearGradient(gradient, start: start, end: end, options: [])
    ctx.restoreGState()
}

func softTrianglePath(top: CGPoint, right: CGPoint, left: CGPoint, curve: CGFloat) -> CGPath {
    let path = CGMutablePath()
    path.move(to: top)
    path.addCurve(
        to: right,
        control1: CGPoint(x: top.x + curve, y: top.y),
        control2: CGPoint(x: right.x, y: right.y + curve)
    )
    path.addCurve(
        to: left,
        control1: CGPoint(x: right.x, y: right.y - curve),
        control2: CGPoint(x: left.x + curve, y: left.y - curve * 0.5)
    )
    path.addCurve(
        to: top,
        control1: CGPoint(x: left.x - curve, y: left.y + curve * 0.7),
        control2: CGPoint(x: top.x - curve, y: top.y)
    )
    path.closeSubpath()
    return path
}

func brandFoldPath() -> CGPath {
    let path = CGMutablePath()
    path.move(to: CGPoint(x: 332, y: 374))
    path.addCurve(
        to: CGPoint(x: 534, y: 356),
        control1: CGPoint(x: 402, y: 320),
        control2: CGPoint(x: 478, y: 320)
    )
    path.addCurve(
        to: CGPoint(x: 420, y: 458),
        control1: CGPoint(x: 492, y: 384),
        control2: CGPoint(x: 454, y: 418)
    )
    path.addCurve(
        to: CGPoint(x: 332, y: 374),
        control1: CGPoint(x: 390, y: 422),
        control2: CGPoint(x: 362, y: 394)
    )
    path.closeSubpath()
    return path
}

func smoothTriangleRingOuterPath() -> CGPath {
    let path = CGMutablePath()
    path.move(to: CGPoint(x: 512, y: 744))
    path.addCurve(
        to: CGPoint(x: 742, y: 354),
        control1: CGPoint(x: 664, y: 744),
        control2: CGPoint(x: 764, y: 520)
    )
    path.addCurve(
        to: CGPoint(x: 282, y: 354),
        control1: CGPoint(x: 704, y: 278),
        control2: CGPoint(x: 320, y: 278)
    )
    path.addCurve(
        to: CGPoint(x: 512, y: 744),
        control1: CGPoint(x: 260, y: 520),
        control2: CGPoint(x: 360, y: 744)
    )
    path.closeSubpath()
    return path
}

func smoothTriangleRingInnerPath() -> CGPath {
    let path = CGMutablePath()
    path.move(to: CGPoint(x: 512, y: 598))
    path.addCurve(
        to: CGPoint(x: 612, y: 430),
        control1: CGPoint(x: 574, y: 598),
        control2: CGPoint(x: 626, y: 500)
    )
    path.addCurve(
        to: CGPoint(x: 412, y: 430),
        control1: CGPoint(x: 586, y: 396),
        control2: CGPoint(x: 438, y: 396)
    )
    path.addCurve(
        to: CGPoint(x: 512, y: 598),
        control1: CGPoint(x: 398, y: 500),
        control2: CGPoint(x: 450, y: 598)
    )
    path.closeSubpath()
    return path
}

func ribbonMarkPath() -> CGPath {
    let path = CGMutablePath()
    path.move(to: CGPoint(x: 316, y: 512))
    path.addCurve(
        to: CGPoint(x: 548, y: 704),
        control1: CGPoint(x: 318, y: 650),
        control2: CGPoint(x: 424, y: 732)
    )
    path.addCurve(
        to: CGPoint(x: 704, y: 456),
        control1: CGPoint(x: 664, y: 678),
        control2: CGPoint(x: 742, y: 568)
    )
    path.addCurve(
        to: CGPoint(x: 444, y: 350),
        control1: CGPoint(x: 668, y: 350),
        control2: CGPoint(x: 548, y: 304)
    )
    path.addCurve(
        to: CGPoint(x: 316, y: 512),
        control1: CGPoint(x: 366, y: 384),
        control2: CGPoint(x: 318, y: 444)
    )
    path.closeSubpath()
    return path
}

func ribbonFoldPath() -> CGPath {
    let path = CGMutablePath()
    path.move(to: CGPoint(x: 456, y: 410))
    path.addCurve(
        to: CGPoint(x: 666, y: 512),
        control1: CGPoint(x: 536, y: 438),
        control2: CGPoint(x: 610, y: 454)
    )
    path.addCurve(
        to: CGPoint(x: 492, y: 622),
        control1: CGPoint(x: 626, y: 588),
        control2: CGPoint(x: 560, y: 628)
    )
    path.addCurve(
        to: CGPoint(x: 456, y: 410),
        control1: CGPoint(x: 456, y: 562),
        control2: CGPoint(x: 438, y: 488)
    )
    path.closeSubpath()
    return path
}

func ribbonHighlightPath() -> CGPath {
    let path = CGMutablePath()
    path.move(to: CGPoint(x: 390, y: 548))
    path.addCurve(
        to: CGPoint(x: 506, y: 654),
        control1: CGPoint(x: 410, y: 612),
        control2: CGPoint(x: 456, y: 650)
    )
    path.addCurve(
        to: CGPoint(x: 610, y: 618),
        control1: CGPoint(x: 548, y: 656),
        control2: CGPoint(x: 584, y: 642)
    )
    return path
}

func auroraRibbonPath() -> CGPath {
    let path = CGMutablePath()
    path.move(to: CGPoint(x: 298, y: 552))
    path.addCurve(
        to: CGPoint(x: 560, y: 306),
        control1: CGPoint(x: 334, y: 404),
        control2: CGPoint(x: 436, y: 302)
    )
    path.addCurve(
        to: CGPoint(x: 754, y: 488),
        control1: CGPoint(x: 692, y: 310),
        control2: CGPoint(x: 780, y: 404)
    )
    path.addCurve(
        to: CGPoint(x: 552, y: 724),
        control1: CGPoint(x: 720, y: 608),
        control2: CGPoint(x: 646, y: 700)
    )
    path.addCurve(
        to: CGPoint(x: 364, y: 690),
        control1: CGPoint(x: 486, y: 744),
        control2: CGPoint(x: 408, y: 738)
    )
    path.addCurve(
        to: CGPoint(x: 298, y: 552),
        control1: CGPoint(x: 324, y: 646),
        control2: CGPoint(x: 288, y: 606)
    )
    path.closeSubpath()
    return path
}

func auroraFoldPath() -> CGPath {
    let path = CGMutablePath()
    path.move(to: CGPoint(x: 514, y: 382))
    path.addCurve(
        to: CGPoint(x: 666, y: 502),
        control1: CGPoint(x: 606, y: 372),
        control2: CGPoint(x: 672, y: 432)
    )
    path.addCurve(
        to: CGPoint(x: 488, y: 690),
        control1: CGPoint(x: 658, y: 594),
        control2: CGPoint(x: 590, y: 666)
    )
    path.addCurve(
        to: CGPoint(x: 568, y: 532),
        control1: CGPoint(x: 546, y: 628),
        control2: CGPoint(x: 582, y: 584)
    )
    path.addCurve(
        to: CGPoint(x: 514, y: 382),
        control1: CGPoint(x: 556, y: 480),
        control2: CGPoint(x: 542, y: 424)
    )
    path.closeSubpath()
    return path
}

func auroraHighlightPath() -> CGPath {
    let path = CGMutablePath()
    path.move(to: CGPoint(x: 390, y: 564))
    path.addCurve(
        to: CGPoint(x: 548, y: 390),
        control1: CGPoint(x: 412, y: 474),
        control2: CGPoint(x: 472, y: 408)
    )
    path.addCurve(
        to: CGPoint(x: 644, y: 420),
        control1: CGPoint(x: 588, y: 380),
        control2: CGPoint(x: 622, y: 390)
    )
    return path
}

struct OrganicPetal {
    let root: CGPoint
    let angle: CGFloat
    let length: CGFloat
    let width: CGFloat
    let bend: CGFloat
    let start: UInt32
    let end: UInt32
}

let tunnelxOrganicPetals: [OrganicPetal] = [
    OrganicPetal(root: CGPoint(x: 512, y: 528), angle: 90, length: 188, width: 58, bend: -8, start: 0xfacc15, end: 0xf59e0b),
    OrganicPetal(root: CGPoint(x: 522, y: 522), angle: 44, length: 194, width: 56, bend: 10, start: 0xfb923c, end: 0xef4444),
    OrganicPetal(root: CGPoint(x: 526, y: 512), angle: 3, length: 154, width: 50, bend: 10, start: 0xec4899, end: 0xdb2777),
    OrganicPetal(root: CGPoint(x: 522, y: 502), angle: -42, length: 186, width: 58, bend: -10, start: 0x8b5cf6, end: 0x6d5dfc),
    OrganicPetal(root: CGPoint(x: 512, y: 496), angle: -90, length: 186, width: 58, bend: 8, start: 0x3b82f6, end: 0x1d9bf0),
    OrganicPetal(root: CGPoint(x: 502, y: 502), angle: -138, length: 190, width: 58, bend: -10, start: 0x06b6d4, end: 0x0891b2),
    OrganicPetal(root: CGPoint(x: 498, y: 512), angle: 177, length: 154, width: 50, bend: -10, start: 0x22c55e, end: 0x16a34a),
    OrganicPetal(root: CGPoint(x: 502, y: 522), angle: 136, length: 162, width: 54, bend: 8, start: 0x84cc16, end: 0x65a30d),
]

let tunnelxOrganicPetalDrawOrder = [6, 2, 7, 0, 1, 3, 5, 4]

func organicPetalPath(_ petal: OrganicPetal, scale: CGFloat = 1) -> CGPath {
    let radians = petal.angle * .pi / 180
    let unit = CGPoint(x: cos(radians), y: sin(radians))
    let perp = CGPoint(x: -unit.y, y: unit.x)
    let length = petal.length * scale
    let width = petal.width * scale
    let bend = petal.bend * scale
    let root = CGPoint(
        x: 512 + (petal.root.x - 512) * scale,
        y: 512 + (petal.root.y - 512) * scale
    )

    func point(_ along: CGFloat, _ side: CGFloat) -> CGPoint {
        let progress = max(0, min(1, along / max(1, length)))
        let curve = sin(progress * .pi) * bend
        return CGPoint(
            x: root.x + unit.x * along + perp.x * (side + curve),
            y: root.y + unit.y * along + perp.y * (side + curve)
        )
    }

    let path = CGMutablePath()
    let baseLeft = point(0, width * 0.12)
    let baseRight = point(0, -width * 0.12)
    let tipLeft = point(length - width * 0.26, width * 0.22)
    let tipRight = point(length - width * 0.26, -width * 0.22)
    path.move(to: baseLeft)
    path.addCurve(
        to: tipLeft,
        control1: point(length * 0.2, width * 0.86),
        control2: point(length * 0.72, width * 0.96)
    )
    path.addCurve(
        to: tipRight,
        control1: point(length + width * 0.2, width * 0.1),
        control2: point(length + width * 0.2, -width * 0.1)
    )
    path.addCurve(
        to: baseRight,
        control1: point(length * 0.72, -width * 0.96),
        control2: point(length * 0.2, -width * 0.86)
    )
    path.addCurve(
        to: baseLeft,
        control1: point(-width * 0.16, -width * 0.06),
        control2: point(-width * 0.16, width * 0.06)
    )
    path.closeSubpath()
    return path
}

func organicPetalTip(_ petal: OrganicPetal, scale: CGFloat = 1) -> CGPoint {
    let radians = petal.angle * .pi / 180
    let root = CGPoint(
        x: 512 + (petal.root.x - 512) * scale,
        y: 512 + (petal.root.y - 512) * scale
    )
    return CGPoint(
        x: root.x + cos(radians) * petal.length * scale,
        y: root.y + sin(radians) * petal.length * scale
    )
}

struct SpiralPetal {
    let angle: CGFloat
    let length: CGFloat
    let width: CGFloat
    let curve: CGFloat
    let start: UInt32
    let end: UInt32
}

let tunnelxSpiralPetals: [SpiralPetal] = [
    SpiralPetal(angle: 96, length: 184, width: 56, curve: -10, start: 0xfacc15, end: 0xf59e0b),
    SpiralPetal(angle: 50, length: 198, width: 58, curve: 16, start: 0xfb923c, end: 0xef4444),
    SpiralPetal(angle: 8, length: 192, width: 56, curve: 14, start: 0xec4899, end: 0xdb2777),
    SpiralPetal(angle: -38, length: 198, width: 58, curve: -12, start: 0x8b5cf6, end: 0x6d5dfc),
    SpiralPetal(angle: -86, length: 188, width: 56, curve: -14, start: 0x38bdf8, end: 0x0ea5e9),
    SpiralPetal(angle: -134, length: 200, width: 58, curve: -14, start: 0x06b6d4, end: 0x0891b2),
    SpiralPetal(angle: -178, length: 190, width: 56, curve: -12, start: 0x10b981, end: 0x059669),
    SpiralPetal(angle: 140, length: 174, width: 52, curve: 12, start: 0x84cc16, end: 0x65a30d),
]

let tunnelxSpiralDrawOrder = [7, 0, 1, 2, 3, 4, 5, 6]

func spiralPetalPath(center: CGPoint, petal: SpiralPetal, scale: CGFloat = 1) -> CGPath {
    let radians = petal.angle * .pi / 180
    let unit = CGPoint(x: cos(radians), y: sin(radians))
    let perp = CGPoint(x: -unit.y, y: unit.x)
    let length = petal.length * scale
    let width = petal.width * scale
    let curve = petal.curve * scale

    func point(_ along: CGFloat, _ side: CGFloat) -> CGPoint {
        let progress = max(0, min(1, along / max(1, length)))
        let bend = sin(progress * .pi) * curve
        return CGPoint(
            x: center.x + unit.x * along + perp.x * (side + bend),
            y: center.y + unit.y * along + perp.y * (side + bend)
        )
    }

    let path = CGMutablePath()
    let baseOuter = point(16 * scale, width * 0.1)
    let baseInner = point(34 * scale, -width * 0.04)
    let tip = point(length + width * 0.1, 0)
    let innerCut = point(length * 0.54, -width * 0.34)
    path.move(to: baseOuter)
    path.addCurve(
        to: tip,
        control1: point(length * 0.24, width * 0.98),
        control2: point(length * 0.76, width * 0.62)
    )
    path.addCurve(
        to: innerCut,
        control1: point(length * 0.9, -width * 0.08),
        control2: point(length * 0.72, -width * 0.26)
    )
    path.addCurve(
        to: baseInner,
        control1: point(length * 0.3, -width * 0.42),
        control2: point(length * 0.1, -width * 0.14)
    )
    path.addCurve(
        to: baseOuter,
        control1: point(10 * scale, -width * 0.02),
        control2: point(4 * scale, width * 0.08)
    )
    path.closeSubpath()
    return path
}

func spiralPetalTip(center: CGPoint, petal: SpiralPetal, scale: CGFloat = 1) -> CGPoint {
    let radians = petal.angle * .pi / 180
    return CGPoint(
        x: center.x + cos(radians) * petal.length * scale,
        y: center.y + sin(radians) * petal.length * scale
    )
}

typealias LooseLogoPetal = (path: CGPath, start: UInt32, end: UInt32, gradientStart: CGPoint, gradientEnd: CGPoint)

func looseFlowerPetals() -> [LooseLogoPetal] {
    var petals: [LooseLogoPetal] = []

    var p = CGMutablePath()
    p.move(to: CGPoint(x: 506, y: 528))
    p.addCurve(to: CGPoint(x: 538, y: 790), control1: CGPoint(x: 456, y: 608), control2: CGPoint(x: 468, y: 742))
    p.addCurve(to: CGPoint(x: 610, y: 635), control1: CGPoint(x: 622, y: 742), control2: CGPoint(x: 628, y: 676))
    p.addCurve(to: CGPoint(x: 538, y: 520), control1: CGPoint(x: 588, y: 580), control2: CGPoint(x: 558, y: 540))
    p.addCurve(to: CGPoint(x: 506, y: 528), control1: CGPoint(x: 526, y: 512), control2: CGPoint(x: 514, y: 516))
    p.closeSubpath()
    petals.append((p, 0x8bd12f, 0x52b82e, CGPoint(x: 500, y: 540), CGPoint(x: 525, y: 780)))

    p = CGMutablePath()
    p.move(to: CGPoint(x: 532, y: 530))
    p.addCurve(to: CGPoint(x: 766, y: 640), control1: CGPoint(x: 608, y: 608), control2: CGPoint(x: 706, y: 688))
    p.addCurve(to: CGPoint(x: 716, y: 506), control1: CGPoint(x: 780, y: 572), control2: CGPoint(x: 758, y: 530))
    p.addCurve(to: CGPoint(x: 558, y: 500), control1: CGPoint(x: 646, y: 486), control2: CGPoint(x: 590, y: 490))
    p.addCurve(to: CGPoint(x: 532, y: 530), control1: CGPoint(x: 542, y: 504), control2: CGPoint(x: 532, y: 516))
    p.closeSubpath()
    petals.append((p, 0xff8a24, 0xef3f22, CGPoint(x: 532, y: 535), CGPoint(x: 742, y: 646)))

    p = CGMutablePath()
    p.move(to: CGPoint(x: 534, y: 502))
    p.addCurve(to: CGPoint(x: 800, y: 488), control1: CGPoint(x: 622, y: 548), control2: CGPoint(x: 724, y: 546))
    p.addCurve(to: CGPoint(x: 692, y: 414), control1: CGPoint(x: 770, y: 438), control2: CGPoint(x: 732, y: 414))
    p.addCurve(to: CGPoint(x: 552, y: 474), control1: CGPoint(x: 626, y: 414), control2: CGPoint(x: 576, y: 446))
    p.addCurve(to: CGPoint(x: 534, y: 502), control1: CGPoint(x: 540, y: 486), control2: CGPoint(x: 534, y: 494))
    p.closeSubpath()
    petals.append((p, 0xec4899, 0xdb2777, CGPoint(x: 535, y: 506), CGPoint(x: 790, y: 500)))

    p = CGMutablePath()
    p.move(to: CGPoint(x: 522, y: 482))
    p.addCurve(to: CGPoint(x: 654, y: 272), control1: CGPoint(x: 584, y: 420), control2: CGPoint(x: 638, y: 344))
    p.addCurve(to: CGPoint(x: 530, y: 336), control1: CGPoint(x: 600, y: 278), control2: CGPoint(x: 560, y: 306))
    p.addCurve(to: CGPoint(x: 500, y: 472), control1: CGPoint(x: 498, y: 376), control2: CGPoint(x: 492, y: 434))
    p.addCurve(to: CGPoint(x: 522, y: 482), control1: CGPoint(x: 506, y: 486), control2: CGPoint(x: 514, y: 490))
    p.closeSubpath()
    petals.append((p, 0x7c3aed, 0x5b5df5, CGPoint(x: 522, y: 482), CGPoint(x: 640, y: 290)))

    p = CGMutablePath()
    p.move(to: CGPoint(x: 494, y: 478))
    p.addCurve(to: CGPoint(x: 314, y: 292), control1: CGPoint(x: 432, y: 420), control2: CGPoint(x: 374, y: 340))
    p.addCurve(to: CGPoint(x: 346, y: 438), control1: CGPoint(x: 304, y: 356), control2: CGPoint(x: 320, y: 404))
    p.addCurve(to: CGPoint(x: 480, y: 512), control1: CGPoint(x: 390, y: 488), control2: CGPoint(x: 440, y: 518))
    p.addCurve(to: CGPoint(x: 494, y: 478), control1: CGPoint(x: 492, y: 504), control2: CGPoint(x: 498, y: 492))
    p.closeSubpath()
    petals.append((p, 0x0ea5e9, 0x0284c7, CGPoint(x: 494, y: 478), CGPoint(x: 330, y: 286)))

    p = CGMutablePath()
    p.move(to: CGPoint(x: 480, y: 512))
    p.addCurve(to: CGPoint(x: 222, y: 520), control1: CGPoint(x: 392, y: 558), control2: CGPoint(x: 306, y: 568))
    p.addCurve(to: CGPoint(x: 340, y: 438), control1: CGPoint(x: 260, y: 476), control2: CGPoint(x: 300, y: 452))
    p.addCurve(to: CGPoint(x: 486, y: 486), control1: CGPoint(x: 402, y: 414), control2: CGPoint(x: 458, y: 448))
    p.addCurve(to: CGPoint(x: 480, y: 512), control1: CGPoint(x: 492, y: 498), control2: CGPoint(x: 490, y: 508))
    p.closeSubpath()
    petals.append((p, 0x10b981, 0x059669, CGPoint(x: 480, y: 510), CGPoint(x: 240, y: 520)))

    p = CGMutablePath()
    p.move(to: CGPoint(x: 494, y: 538))
    p.addCurve(to: CGPoint(x: 340, y: 710), control1: CGPoint(x: 420, y: 568), control2: CGPoint(x: 368, y: 634))
    p.addCurve(to: CGPoint(x: 474, y: 662), control1: CGPoint(x: 402, y: 708), control2: CGPoint(x: 452, y: 686))
    p.addCurve(to: CGPoint(x: 522, y: 546), control1: CGPoint(x: 504, y: 622), control2: CGPoint(x: 526, y: 578))
    p.addCurve(to: CGPoint(x: 494, y: 538), control1: CGPoint(x: 514, y: 532), control2: CGPoint(x: 504, y: 530))
    p.closeSubpath()
    petals.append((p, 0xfacc15, 0xf59e0b, CGPoint(x: 494, y: 538), CGPoint(x: 355, y: 695)))

    return petals
}

// 应用图标：从视觉稿源图导出各尺寸，并裁成圆角 app icon。
func renderLogo(size: Int) -> CGImage {
    let ctx = CGContext(
        data: nil,
        width: size,
        height: size,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!
    ctx.setAllowsAntialiasing(true)
    ctx.setShouldAntialias(true)
    ctx.scaleBy(x: CGFloat(size) / 1024, y: CGFloat(size) / 1024)

    guard let source = loadPNG(assetsDir.appendingPathComponent("tunnelx-logo-flower.png")) else {
        fatalError("Missing src/assets/tunnelx-logo-flower.png")
    }

    let baseRect = CGRect(x: 90, y: 90, width: 844, height: 844)
    let clip = CGPath(
        roundedRect: baseRect,
        cornerWidth: 196,
        cornerHeight: 196,
        transform: nil
    )

    ctx.saveGState()
    ctx.addPath(clip)
    ctx.setFillColor(color(0xffffff))
    ctx.fillPath()
    ctx.restoreGState()

    let flowerRect = CGRect(x: 192, y: 201, width: 640, height: 622)

    ctx.saveGState()
    ctx.addPath(clip)
    ctx.clip()
    ctx.interpolationQuality = .high
    ctx.draw(source, in: flowerRect)
    ctx.restoreGState()

    return ctx.makeImage()!
}

// 托盘图标：只使用透明花瓣，不带 app icon 白色底板；未连接时降低颜色强度。
func renderTray(size: Int, connected: Bool) -> CGImage {
    let ctx = CGContext(
        data: nil,
        width: size,
        height: size,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!
    ctx.setAllowsAntialiasing(true)
    ctx.setShouldAntialias(true)

    guard let source = loadPNG(assetsDir.appendingPathComponent("tunnelx-logo-flower.png")) else {
        fatalError("Missing src/assets/tunnelx-logo-flower.png")
    }

    let pad = CGFloat(size) * 0.06
    let drawRect = CGRect(x: pad, y: pad, width: CGFloat(size) - pad * 2, height: CGFloat(size) - pad * 2)

    ctx.saveGState()
    ctx.setAlpha(connected ? 1 : 0.38)
    ctx.interpolationQuality = .high
    ctx.draw(source, in: drawRect)
    ctx.restoreGState()

    return ctx.makeImage()!
}

func writeICO(images: [(Int, CGImage)], to url: URL) throws {
    var data = Data()
    func u16(_ value: UInt16) {
        data.append(UInt8(value & 0xff))
        data.append(UInt8((value >> 8) & 0xff))
    }
    func u32(_ value: UInt32) {
        data.append(UInt8(value & 0xff))
        data.append(UInt8((value >> 8) & 0xff))
        data.append(UInt8((value >> 16) & 0xff))
        data.append(UInt8((value >> 24) & 0xff))
    }

    let pngs = images.map { ($0.0, pngData(from: $0.1)) }
    u16(0)
    u16(1)
    u16(UInt16(pngs.count))

    var offset = 6 + pngs.count * 16
    for (size, png) in pngs {
        data.append(size >= 256 ? 0 : UInt8(size))
        data.append(size >= 256 ? 0 : UInt8(size))
        data.append(0)
        data.append(0)
        u16(1)
        u16(32)
        u32(UInt32(png.count))
        u32(UInt32(offset))
        offset += png.count
    }
    for (_, png) in pngs {
        data.append(png)
    }
    try data.write(to: url)
}

func writeICNS(images: [(String, CGImage)], to url: URL) throws {
    var chunks = Data()
    func appendFourCC(_ value: String, to data: inout Data) {
        data.append(contentsOf: value.utf8)
    }
    func appendBE32(_ value: UInt32, to data: inout Data) {
        data.append(UInt8((value >> 24) & 0xff))
        data.append(UInt8((value >> 16) & 0xff))
        data.append(UInt8((value >> 8) & 0xff))
        data.append(UInt8(value & 0xff))
    }

    for (type, image) in images {
        let png = pngData(from: image)
        appendFourCC(type, to: &chunks)
        appendBE32(UInt32(png.count + 8), to: &chunks)
        chunks.append(png)
    }

    var data = Data()
    appendFourCC("icns", to: &data)
    appendBE32(UInt32(chunks.count + 8), to: &data)
    data.append(chunks)
    try data.write(to: url)
}

try FileManager.default.createDirectory(at: assetsDir, withIntermediateDirectories: true)
try FileManager.default.createDirectory(at: iconsDir, withIntermediateDirectories: true)

try writePNG(renderLogo(size: 256), to: assetsDir.appendingPathComponent("tunnelx-logo.png"))
try writePNG(renderLogo(size: 32), to: iconsDir.appendingPathComponent("32x32.png"))
try writePNG(renderLogo(size: 128), to: iconsDir.appendingPathComponent("128x128.png"))
try writePNG(renderLogo(size: 256), to: iconsDir.appendingPathComponent("128x128@2x.png"))
try writePNG(renderTray(size: 32, connected: true), to: iconsDir.appendingPathComponent("tray-connected.png"))
try writePNG(renderTray(size: 32, connected: false), to: iconsDir.appendingPathComponent("tray-disconnected.png"))

try writeICNS(
    images: [
        ("icp4", renderLogo(size: 16)),
        ("icp5", renderLogo(size: 32)),
        ("icp6", renderLogo(size: 64)),
        ("ic07", renderLogo(size: 128)),
        ("ic08", renderLogo(size: 256)),
        ("ic09", renderLogo(size: 512)),
        ("ic10", renderLogo(size: 1024)),
    ],
    to: iconsDir.appendingPathComponent("icon.icns")
)
try writeICO(
    images: [16, 32, 48, 64, 128, 256].map { ($0, renderLogo(size: $0)) },
    to: iconsDir.appendingPathComponent("icon.ico")
)

print("[render-tunnelx-icons] Wrote TunnelX app, tray, ICO, and ICNS icons.")
