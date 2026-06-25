use std::pin::Pin;

use async_trait::async_trait;
use btleplug::api::{Characteristic, ValueNotification};
use bytes::Bytes;
use cutout_core::{NotificationByteLen, WriteMode};
use futures_util::{StreamExt, stream::Stream};
use uuid::Uuid;

use crate::{BtleError, CapturedBtlePacket, NegotiatedWriteLen};

/// Notification admitted at the BTLE adapter boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BtleNotification {
    /// Characteristic UUID that emitted the notification.
    pub characteristic: Uuid,

    /// Service UUID associated with the notification.
    pub service: Uuid,

    /// Captured and classified notification payload.
    pub bytes: CapturedBtlePacket,
}

impl BtleNotification {
    /// Creates a typed BTLE notification from raw adapter bytes.
    #[must_use]
    pub fn from_raw_bytes(characteristic: Uuid, service: Uuid, bytes: impl Into<Bytes>) -> Self {
        Self {
            characteristic,
            service,
            bytes: CapturedBtlePacket::from_raw_bytes(bytes.into()),
        }
    }

    pub(crate) fn from_backend(notification: ValueNotification) -> Self {
        Self::from_raw_bytes(
            notification.uuid,
            notification.service_uuid,
            notification.value,
        )
    }

    /// Returns the original notification bytes for parser/capture boundaries.
    #[must_use]
    pub fn as_raw_bytes(&self) -> &[u8] {
        self.bytes.as_raw_bytes()
    }

    /// Returns the typed notification payload length.
    #[must_use]
    pub fn len(&self) -> NotificationByteLen {
        self.bytes.len()
    }

    /// Returns true when the notification payload is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Typed outbound BTLE write chunk admitted by the bridge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BtleWriteChunk<'a> {
    bytes: &'a [u8],
    negotiated_limit: NegotiatedWriteLen,
}

impl<'a> BtleWriteChunk<'a> {
    /// Creates a write chunk after negotiated MTU splitting.
    ///
    /// # Panics
    ///
    /// Returns `None` when `bytes` exceeds `negotiated_limit`.
    #[must_use]
    pub fn new(bytes: &'a [u8], negotiated_limit: NegotiatedWriteLen) -> Option<Self> {
        (bytes.len() <= negotiated_limit.chunk_len()).then_some(Self {
            bytes,
            negotiated_limit,
        })
    }

    /// Returns the write bytes for the backend adapter edge.
    #[must_use]
    pub const fn as_slice(self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the negotiated write limit that admitted this chunk.
    #[must_use]
    pub const fn negotiated_limit(self) -> NegotiatedWriteLen {
        self.negotiated_limit
    }
}

/// Minimal BTLE operations required by the protocol bridge.
#[async_trait]
pub trait SessionPeripheral: Send + Sync {
    /// Returns the negotiated MTU for the connected peripheral.
    fn mtu(&self) -> u16;

    /// Subscribes to notifications on the selected endpoint.
    async fn subscribe(&self, characteristic: &Characteristic) -> Result<(), BtleError>;

    /// Writes a payload to the selected endpoint.
    async fn write(
        &self,
        characteristic: &Characteristic,
        chunk: BtleWriteChunk<'_>,
        mode: WriteMode,
    ) -> Result<(), BtleError>;

    /// Returns the notification stream for the connected peripheral.
    async fn notifications(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = BtleNotification> + Send>>, BtleError>;

    /// Disconnects the peripheral.
    async fn disconnect(&self) -> Result<(), BtleError>;
}

#[async_trait]
impl SessionPeripheral for btleplug::platform::Peripheral {
    fn mtu(&self) -> u16 {
        btleplug::api::Peripheral::mtu(self)
    }

    async fn subscribe(&self, characteristic: &Characteristic) -> Result<(), BtleError> {
        btleplug::api::Peripheral::subscribe(self, characteristic)
            .await
            .map_err(BtleError::from)
    }

    async fn write(
        &self,
        characteristic: &Characteristic,
        chunk: BtleWriteChunk<'_>,
        mode: WriteMode,
    ) -> Result<(), BtleError> {
        btleplug::api::Peripheral::write(
            self,
            characteristic,
            chunk.as_slice(),
            match mode {
                WriteMode::WithResponse => btleplug::api::WriteType::WithResponse,
                WriteMode::WithoutResponse => btleplug::api::WriteType::WithoutResponse,
            },
        )
        .await
        .map_err(BtleError::from)
    }

    async fn notifications(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = BtleNotification> + Send>>, BtleError> {
        let notifications = btleplug::api::Peripheral::notifications(self)
            .await
            .map_err(BtleError::from)?;
        Ok(Box::pin(notifications.map(BtleNotification::from_backend)))
    }

    async fn disconnect(&self) -> Result<(), BtleError> {
        btleplug::api::Peripheral::disconnect(self)
            .await
            .map_err(BtleError::from)
    }
}

#[cfg(test)]
mod tests {
    use btleplug::api::ValueNotification;
    use uuid::Uuid;

    use super::{BtleNotification, BtleWriteChunk};
    use crate::{MalformedBtlePacketReason, NegotiatedWriteLen};

    #[test]
    fn btle_notification_consumes_backend_vec_into_typed_bytes() {
        let notification = BtleNotification::from_backend(ValueNotification {
            uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            value: vec![0xde, 0xad],
        });

        assert_eq!(notification.as_raw_bytes(), [0xde, 0xad]);
        assert_eq!(notification.len().get(), 2);
    }

    #[test]
    fn btle_notification_tags_oversized_backend_values_as_malformed() {
        let notification = BtleNotification::from_backend(ValueNotification {
            uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            value: vec![0; 513],
        });

        let crate::CapturedBtlePacket::Malformed(malformed) = notification.bytes else {
            panic!("oversized backend value should be malformed");
        };
        assert_eq!(
            malformed.reason(),
            MalformedBtlePacketReason::OversizedAttributeValue {
                max: cutout_core::NotificationByteLen::from_bytes(512),
            }
        );
    }

    #[test]
    fn btle_write_chunk_requires_negotiated_limit() {
        let limit = NegotiatedWriteLen::from_mtu(7);
        let accepted = vec![0; limit.chunk_len()];
        let rejected = vec![0; limit.chunk_len() + 1];

        assert!(BtleWriteChunk::new(&accepted, limit).is_some());
        assert!(BtleWriteChunk::new(&rejected, limit).is_none());
    }
}
