//! Recorded phone-drive facts. The phase marker selects which fact this is.

use std::marker::PhantomData;

use crate::MonotonicTimestamp;

use super::{PhoneDriveAction, PhoneDriveContactId};

mod sealed {
    pub trait Sealed {}
}

/// Selects one recorded phone-drive fact.
///
/// Only [`Intent`], [`Admitted`], [`NativeReceipt`], and [`ObservedEffect`] implement this.
pub trait PhoneDriveFactPhase: sealed::Sealed + Copy + Send + Sync + 'static {}

/// Rider intent, before any admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Intent;

/// Recorded admission of rider intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Admitted;

/// Receipt returned by native submission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReceipt;

/// Controller effect observed apart from admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservedEffect;

impl sealed::Sealed for Intent {}
impl sealed::Sealed for Admitted {}
impl sealed::Sealed for NativeReceipt {}
impl sealed::Sealed for ObservedEffect {}

impl PhoneDriveFactPhase for Intent {}
impl PhoneDriveFactPhase for Admitted {}
impl PhoneDriveFactPhase for NativeReceipt {}
impl PhoneDriveFactPhase for ObservedEffect {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Shared<Phase> {
    action: PhoneDriveAction,
    contact_id: PhoneDriveContactId,
    observed_at: MonotonicTimestamp,
    phase: PhantomData<fn() -> Phase>,
}

/// One recorded phone-drive fact.
///
/// The action, contact, and monotonic observation time are stored once.
/// `Phase` is the marker that selects rider intent, admission, a native receipt,
/// or an independently observed controller effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneDriveFact<Phase>(Shared<Phase>);

impl<Phase: PhoneDriveFactPhase> PhoneDriveFact<Phase> {
    /// Records one fact at the host monotonic time it was observed.
    #[must_use]
    pub const fn new(
        action: PhoneDriveAction,
        contact_id: PhoneDriveContactId,
        observed_at: MonotonicTimestamp,
    ) -> Self {
        Self(Shared {
            action,
            contact_id,
            observed_at,
            phase: PhantomData,
        })
    }

    /// Semantic throttle or brake the fact is about.
    #[must_use]
    pub const fn action(self) -> PhoneDriveAction {
        self.0.action
    }

    /// Native contact the fact is about.
    #[must_use]
    pub const fn contact_id(self) -> PhoneDriveContactId {
        self.0.contact_id
    }

    /// Host monotonic time of this observation.
    #[must_use]
    pub const fn observed_at(self) -> MonotonicTimestamp {
        self.0.observed_at
    }
}

/// A recorded fact selected by its phase marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneDriveEvidence {
    /// Rider intent.
    Intent(PhoneDriveFact<Intent>),
    /// Admission of that intent.
    Admitted(PhoneDriveFact<Admitted>),
    /// Native submission receipt.
    NativeReceipt(PhoneDriveFact<NativeReceipt>),
    /// Controller effect observed apart from admission.
    ObservedEffect(PhoneDriveFact<ObservedEffect>),
}

impl PhoneDriveEvidence {
    /// Records rider intent.
    #[must_use]
    pub const fn intent(
        action: PhoneDriveAction,
        contact_id: PhoneDriveContactId,
        observed_at: MonotonicTimestamp,
    ) -> Self {
        Self::Intent(PhoneDriveFact::new(action, contact_id, observed_at))
    }

    /// Records admission of rider intent.
    #[must_use]
    pub const fn admitted(
        action: PhoneDriveAction,
        contact_id: PhoneDriveContactId,
        observed_at: MonotonicTimestamp,
    ) -> Self {
        Self::Admitted(PhoneDriveFact::new(action, contact_id, observed_at))
    }

    /// Records a native submission receipt.
    #[must_use]
    pub const fn native_receipt(
        action: PhoneDriveAction,
        contact_id: PhoneDriveContactId,
        observed_at: MonotonicTimestamp,
    ) -> Self {
        Self::NativeReceipt(PhoneDriveFact::new(action, contact_id, observed_at))
    }

    /// Records a controller effect observed apart from admission.
    #[must_use]
    pub const fn observed_effect(
        action: PhoneDriveAction,
        contact_id: PhoneDriveContactId,
        observed_at: MonotonicTimestamp,
    ) -> Self {
        Self::ObservedEffect(PhoneDriveFact::new(action, contact_id, observed_at))
    }

    /// Semantic throttle or brake this fact records.
    #[must_use]
    pub const fn action(self) -> PhoneDriveAction {
        match self {
            Self::Intent(fact) => fact.action(),
            Self::Admitted(fact) => fact.action(),
            Self::NativeReceipt(fact) => fact.action(),
            Self::ObservedEffect(fact) => fact.action(),
        }
    }

    /// Native contact this fact records.
    #[must_use]
    pub const fn contact_id(self) -> PhoneDriveContactId {
        match self {
            Self::Intent(fact) => fact.contact_id(),
            Self::Admitted(fact) => fact.contact_id(),
            Self::NativeReceipt(fact) => fact.contact_id(),
            Self::ObservedEffect(fact) => fact.contact_id(),
        }
    }

    /// Host monotonic time of this observation.
    #[must_use]
    pub const fn observed_at(self) -> MonotonicTimestamp {
        match self {
            Self::Intent(fact) => fact.observed_at(),
            Self::Admitted(fact) => fact.observed_at(),
            Self::NativeReceipt(fact) => fact.observed_at(),
            Self::ObservedEffect(fact) => fact.observed_at(),
        }
    }
}
