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
