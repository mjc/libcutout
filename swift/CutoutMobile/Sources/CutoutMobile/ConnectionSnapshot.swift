import CutoutMobileFFI

/// Immutable identity captured with work before crossing a callback or queue boundary.
public typealias ConnectionAttemptToken = CutoutMobileFFI.MobileConnectionAttemptTokenDto

/// Rust-owned connection identity and admission, read together under the session lock.
public typealias ConnectionSnapshot = CutoutMobileFFI.MobileConnectionAttemptSnapshotDto

/// Native phase and effect metadata captured with Rust identity before Main delivery.
public struct SessionConnectionPresentation: Equatable, Sendable {
    public let phase: SessionConnectionPhase
    public let isRecordOnly: Bool
    public let connection: ConnectionSnapshot

    public init(phase: SessionConnectionPhase, isRecordOnly: Bool, connection: ConnectionSnapshot) {
        self.phase = phase
        self.isRecordOnly = isRecordOnly
        self.connection = connection
    }
}
