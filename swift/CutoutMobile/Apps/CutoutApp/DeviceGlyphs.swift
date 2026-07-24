import CutoutMobile
import SwiftUI

struct DeviceGlyph: View {
    let row: DevicePickerRow

    var body: some View {
        GeometryReader { proxy in
            let size = proxy.size
            let side = min(proxy.size.width, proxy.size.height)
            let line = max(2, side * 0.08)

            ZStack {
                switch row.glyphKind {
                case .electricUnicycle:
                    EucGlyph(color: row.glyphColor, lineWidth: line, size: size)
                case .onewheel:
                    OnewheelGlyph(color: row.glyphColor, accent: PevColors.purple, lineWidth: line, size: size)
                case .scooter:
                    ScooterGlyph(color: row.glyphColor, lineWidth: line, size: size)
                case .hoverboard:
                    HoverboardGlyph(color: row.glyphColor, lineWidth: line, size: size)
                case .systemSymbol:
                    Circle()
                        .fill(row.glyphBackground)
                    Image(systemName: row.symbolName)
                        .font(.system(size: side * 0.42, weight: .bold))
                        .foregroundStyle(row.glyphColor)
                }
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
        }
    }
}

struct EucGlyph: View {
    let color: Color
    let lineWidth: CGFloat
    let size: CGSize

    var body: some View {
        let side = min(size.width, size.height)
        ZStack {
            Circle()
                .fill(PevColors.iconFill)
            Circle()
                .stroke(color, lineWidth: lineWidth)
            Circle()
                .fill(PevColors.cardFill)
                .frame(width: side * 0.42, height: side * 0.42)
            ForEach(0..<8, id: \.self) { index in
                Circle()
                    .fill(color)
                    .frame(width: side * 0.085, height: side * 0.085)
                    .offset(y: -side * 0.16)
                    .rotationEffect(.degrees(Double(index) * 45))
            }
            Circle()
                .fill(color)
                .frame(width: side * 0.10, height: side * 0.10)
        }
        .frame(width: size.width, height: size.height)
    }
}

struct OnewheelGlyph: View {
    let color: Color
    let accent: Color
    let lineWidth: CGFloat
    let size: CGSize

    var body: some View {
        let side = min(size.width, size.height)
        ZStack {
            Capsule()
                .stroke(accent, lineWidth: lineWidth * 0.8)
                .frame(width: side * 0.92, height: side * 0.34)
            Circle()
                .fill(PevColors.iconFill)
                .frame(width: side * 0.46, height: side * 0.46)
            Circle()
                .stroke(color, lineWidth: lineWidth)
                .frame(width: side * 0.46, height: side * 0.46)
        }
        .frame(width: size.width, height: size.height)
    }
}

struct ScooterGlyph: View {
    let color: Color
    let lineWidth: CGFloat
    let size: CGSize

    var body: some View {
        let side = min(size.width, size.height)
        ZStack {
            Circle()
                .stroke(color, lineWidth: lineWidth * 0.85)
                .frame(width: side * 0.23, height: side * 0.23)
                .offset(x: -side * 0.30, y: side * 0.22)
            Circle()
                .stroke(color, lineWidth: lineWidth * 0.85)
                .frame(width: side * 0.23, height: side * 0.23)
                .offset(x: side * 0.32, y: side * 0.22)
            Path { path in
                path.move(to: CGPoint(x: side * 0.20, y: side * 0.29))
                path.addLine(to: CGPoint(x: side * 0.48, y: side * 0.74))
                path.addLine(to: CGPoint(x: side * 0.75, y: side * 0.74))
                path.move(to: CGPoint(x: side * 0.48, y: side * 0.74))
                path.addLine(to: CGPoint(x: side * 0.26, y: side * 0.74))
                path.move(to: CGPoint(x: side * 0.20, y: side * 0.29))
                path.addLine(to: CGPoint(x: side * 0.34, y: side * 0.29))
            }
            .stroke(color, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round, lineJoin: .round))
            .frame(width: side, height: side)
        }
        .frame(width: size.width, height: size.height)
    }
}

struct HoverboardGlyph: View {
    let color: Color
    let lineWidth: CGFloat
    let size: CGSize

    var body: some View {
        let side = min(size.width, size.height)
        ZStack {
            Capsule()
                .stroke(color, lineWidth: lineWidth)
                .frame(width: side * 0.56, height: side * 0.26)
                .offset(x: -side * 0.18)
            Capsule()
                .stroke(color, lineWidth: lineWidth)
                .frame(width: side * 0.56, height: side * 0.26)
                .offset(x: side * 0.18)
        }
        .frame(width: size.width, height: size.height)
    }
}
