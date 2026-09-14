import Foundation

enum CameraH264AccessUnitError: Error, Equatable {
    case empty
    case truncatedNAL
    case zeroLengthNAL
}

/// One AVCC-style H.264 access unit emitted by the Rust RTSP transport.
struct CameraH264AccessUnit: Equatable {
    let data: Data
    let parameterSets: [Data]
    let isRandomAccessPoint: Bool

    /// Extracts SPS/PPS NAL units from an H.264 `avcC` decoder
    /// configuration record advertised by RTSP SDP.
    static func parameterSets(fromAVCC data: Data) -> [Data]? {
        guard data.count >= 7, data[data.startIndex] == 1 else { return nil }
        var offset = data.startIndex + 5
        let spsCount = Int(data[offset] & 0x1f)
        offset += 1
        var sps: Data?
        for _ in 0..<spsCount {
            guard let candidate = readLengthPrefixedNAL(from: data, offset: &offset) else {
                return nil
            }
            sps = sps ?? candidate
        }
        guard offset < data.endIndex else { return nil }
        let ppsCount = Int(data[offset])
        offset += 1
        var pps: Data?
        for _ in 0..<ppsCount {
            guard let candidate = readLengthPrefixedNAL(from: data, offset: &offset) else {
                return nil
            }
            pps = pps ?? candidate
        }
        guard let sps, let pps else { return nil }
        return [sps, pps]
    }

    private static func readLengthPrefixedNAL(
        from data: Data,
        offset: inout Data.Index
    ) -> Data? {
        guard data.endIndex - offset >= 2 else { return nil }
        let length = Int(data[offset]) << 8 | Int(data[offset + 1])
        offset += 2
        guard length > 0, data.endIndex - offset >= length else { return nil }
        defer { offset += length }
        return Data(data[offset..<(offset + length)])
    }

    init(data: Data) throws {
        guard !data.isEmpty else { throw CameraH264AccessUnitError.empty }

        var offset = 0
        var sps: Data?
        var pps: Data?
        var isRandomAccessPoint = false

        while offset < data.count {
            guard data.count - offset >= 4 else {
                throw CameraH264AccessUnitError.truncatedNAL
            }

            let length = data[offset..<(offset + 4)].reduce(UInt32(0)) { value, byte in
                (value << 8) | UInt32(byte)
            }
            guard length > 0 else { throw CameraH264AccessUnitError.zeroLengthNAL }

            let payloadStart = offset + 4
            let payloadEnd = payloadStart + Int(length)
            guard payloadEnd <= data.count else {
                throw CameraH264AccessUnitError.truncatedNAL
            }

            let nal = Data(data[payloadStart..<payloadEnd])
            if let header = nal.first {
                switch header & 0x1f {
                case 5:
                    isRandomAccessPoint = true
                case 7:
                    sps = nal
                case 8:
                    pps = nal
                default:
                    break
                }
            }
            offset = payloadEnd
        }

        self.data = data
        self.parameterSets = [sps, pps].compactMap { $0 }
        self.isRandomAccessPoint = isRandomAccessPoint
    }
}
