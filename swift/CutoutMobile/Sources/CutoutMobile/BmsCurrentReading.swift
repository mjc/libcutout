import CutoutMobileFFI

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
        displayedPackCurrents(pack0: bmsPackCurrent0, pack1: bmsPackCurrent1, legacy: current).map { reading in
            BmsCurrentReading(packIndex: reading.packIndex.map(Int.init), current: reading.current)
        }
    }
}
