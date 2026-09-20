//! Shared source of truth for control destinations and their serialization.
//! Schemas are constructed in const contexts by `control_wire_schema!`.

use arrayvec::ArrayVec;
use cutout_core::{WriteMode, WritePayload};

use crate::request_encoder::EncodedControlStep;
use crate::request_encoder::request_payload as payload;

mod schema;
use schema::BinaryField;
pub(crate) use schema::{Layout, Schema};

impl Schema {
    pub(crate) fn payload(&self, index: usize, value: u8) -> Option<WritePayload> {
        match self.layout(index) {
            Layout::Literal(bytes) => Some(payload(bytes)),
            Layout::DecimalMenu { .. } => None,
            Layout::Field {
                magic,
                bank,
                offset,
            } => Some(field_payload(
                BinaryField {
                    magic,
                    bank,
                    offset,
                },
                value,
            )),
        }
    }

    pub(crate) fn steps(&self, index: usize, value: u8) -> Option<ArrayVec<EncodedControlStep, 5>> {
        let mut steps = ArrayVec::new();
        let Layout::DecimalMenu { selector, digits } = self.layout(index) else {
            return None;
        };
        if (digits == 1 && value > 9) || value > 99 {
            return None;
        }
        let mut push = |delay_ms, byte| {
            steps.push(EncodedControlStep {
                delay_ms,
                payload: payload(&[byte]),
                mode: WriteMode::WithoutResponse,
            });
        };
        push(0, b'W');
        push(100, selector);
        if digits == 2 {
            push(200, b'0' + value / 10);
        }
        push(200, b'0' + value % 10);
        push(200, b'b');
        Some(steps)
    }
}

fn field_payload(field: BinaryField, value: u8) -> WritePayload {
    let mut frame = ArrayVec::<u8, 33>::new();
    frame
        .try_extend_from_slice(&field.magic)
        .expect("binary field magic fits");
    frame.push(field.offset + 5);
    frame
        .try_extend_from_slice(field.bank)
        .expect("binary field bank fits");
    while frame.len() < usize::from(field.offset) {
        frame.push(0x80);
    }
    frame.push(value);
    let crc = crc32fast::hash(&frame).to_be_bytes();
    frame
        .try_extend_from_slice(&crc)
        .expect("binary field CRC fits");
    payload(&frame)
}

pub(crate) trait WireCommand: Copy {
    type Protocol;
    const SCHEMA: Schema;
    fn index(self) -> usize;

    fn select(self, value: u8) -> Selection<Self::Protocol> {
        Selection {
            schema: &Self::SCHEMA,
            index: self.index(),
            value,
            protocol: core::marker::PhantomData,
        }
    }
}

/// Opaque selection from a checked schema, never an arbitrary byte buffer.
#[derive(Clone, Copy, Debug)]
pub struct Selection<P> {
    schema: &'static Schema,
    index: usize,
    value: u8,
    protocol: core::marker::PhantomData<fn() -> P>,
}

impl<P> Selection<P> {
    pub(crate) fn single(
        self,
        command: cutout_core::DeviceCommand,
    ) -> Option<crate::EncodedControl> {
        Some(crate::EncodedControl {
            command: command.kind(),
            payload: self.schema.payload(self.index, self.value)?,
            mode: WriteMode::WithoutResponse,
        })
    }

    pub(crate) fn sequence(
        self,
        command: cutout_core::DeviceCommand,
    ) -> Option<crate::EncodedControlSequence> {
        Some(crate::EncodedControlSequence {
            command: command.kind(),
            steps: self.schema.steps(self.index, self.value)?,
        })
    }
}

macro_rules! control_wire_schema {
    ($protocol:ty => $name:ident { $($variant:ident => $layout:expr),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug)]
        pub(crate) enum $name { $($variant),+ }
        impl $crate::control_wire::WireCommand for $name {
            type Protocol = $protocol;
            const SCHEMA: $crate::control_wire::Schema = {
                const LAYOUTS: &[$crate::control_wire::Layout] = &[$($layout),+];
                $crate::control_wire::Schema::new(LAYOUTS)
            };
            fn index(self) -> usize { self as usize }
        }
        // Evaluate even when an adapter has no callers in this build.
        const _: &$crate::control_wire::Schema =
            &<$name as $crate::control_wire::WireCommand>::SCHEMA;
    };
}

macro_rules! control_wire_dialect {
    ($dialect:ty : $parent:ty) => {
        const _: &$crate::control_wire::Schema =
            &<$parent as $crate::control_wire::WireCommand>::SCHEMA;
        impl $crate::control_wire::CheckedDialect for $dialect {}
    };
    ($dialect:ty => $name:ident : $parent:ty { $($variant:ident => $layout:expr),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug)]
        pub(crate) enum $name { $($variant),+ }
        impl $crate::control_wire::WireCommand for $name {
            type Protocol = <$parent as $crate::control_wire::WireCommand>::Protocol;
            const SCHEMA: $crate::control_wire::Schema = {
                const LAYOUTS: &[$crate::control_wire::Layout] = &[$($layout),+];
                $crate::control_wire::Schema::extend(
                    &<$parent as $crate::control_wire::WireCommand>::SCHEMA, LAYOUTS,
                )
            };
            fn index(self) -> usize {
                <$parent as $crate::control_wire::WireCommand>::SCHEMA.len() + self as usize
            }
        }
        control_wire_dialect!($dialect: $name);
    };
}

/// Implemented only by dialect encoders with a compile-time-checked schema.
pub trait CheckedDialect {}

/// Protocol-level encoding, separate from model capabilities and safety policy.
pub trait Dialect: CheckedDialect {
    /// Base wire protocol to which this dialect belongs.
    type Protocol;
    /// Selects a checked command using the connection's protocol dialect.
    fn select(
        command: cutout_core::DeviceCommand,
        context: crate::session::ControlEncodingContext,
    ) -> Option<Selection<Self::Protocol>>;
}

/// Models select a checked protocol; they cannot provide independent serializers.
pub trait Model {
    /// Protocol implementation used for all of this model's control writes.
    type Protocol;
    /// Selected dialect must belong to the model's base protocol.
    type WireDialect: Dialect<Protocol = Self::Protocol>;
}

/// Community-named Veteran wire protocol, shared by Leaperkim and NOSFET.
#[derive(Clone, Copy, Debug)]
pub enum VeteranProtocol {}

/// Begode wire protocol; model-specific commands belong to its dialects.
#[derive(Clone, Copy, Debug)]
pub enum BegodeProtocol {}

/// VESC firmware's wire protocol; packages such as Refloat layer dialects on it.
#[derive(Clone, Copy, Debug)]
pub enum VescProtocol {}

mod begode;
mod veteran;
pub(crate) use begode::BegodeWire;
pub(crate) use veteran::VeteranWire;
pub(crate) use veteran::nosfet::NosfetWire;

#[cfg(test)]
mod tests;
