public struct BmsCurrentReading: Equatable, Hashable, Sendable {
    public let packIndex: Int?
    public let current: BatteryCurrent

    public init(packIndex: Int?, current: BatteryCurrent) {
        self.packIndex = packIndex
        self.current = current
    }
}

extension BmsSnapshot {
    /// Prefer the source's paired pack-specific values over the ambiguous legacy pack current.
    public var currentReadings: [BmsCurrentReading] {
        let paired = [bmsPackCurrent0, bmsPackCurrent1].enumerated().compactMap { index, current in
            current.map { BmsCurrentReading(packIndex: index + 1, current: $0) }
        }
        if !paired.isEmpty { return paired }
        return current.map { [BmsCurrentReading(packIndex: nil, current: $0)] } ?? []
    }
}
