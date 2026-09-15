import CutoutMobileFFI

/// Immutable identity captured with work before crossing a callback or queue boundary.
public typealias ConnectionAttemptToken = CutoutMobileFFI.MobileConnectionAttemptTokenDto

/// Rust-owned connection identity and admission, read together under the session lock.
public typealias ConnectionSnapshot = CutoutMobileFFI.MobileConnectionAttemptSnapshotDto
