package lol.cutout.local

import lol.cutout.CaptureHistorySendBadge
import lol.cutout.captureHistorySendBadge
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.cutout_mobile_ffi.MobileCaptureIntegrityDto
import uniffi.cutout_mobile_ffi.MobileCaptureSubmissionCategoriesDto
import uniffi.cutout_mobile_ffi.MobileCaptureSubmissionLocalPayloadStateDto
import uniffi.cutout_mobile_ffi.MobileCaptureSubmissionOriginDto
import uniffi.cutout_mobile_ffi.MobileCaptureSubmissionPhaseDto
import uniffi.cutout_mobile_ffi.MobileCaptureSubmissionReceiptDto
import uniffi.cutout_mobile_ffi.MobileCaptureSubmissionStatusDto

class LocalCaptureSubmissionGateTest {
    @Test
    fun historyBadgeRequiresDurableSendReceiptAndKeepsRemoteDeletionDistinct() {
        assertEquals(null, captureHistorySendBadge(status(MobileCaptureSubmissionPhaseDto.SENT, null)))
        val receipt = MobileCaptureSubmissionReceiptDto(
            operationId = "fixed-operation",
            sha256 = "a".repeat(64),
            bytes = 100uL,
            receivedAtMilliseconds = 1_700_000_000_000uL,
        )
        assertEquals(CaptureHistorySendBadge.SENT, captureHistorySendBadge(status(MobileCaptureSubmissionPhaseDto.SENT, receipt)))
        assertEquals(CaptureHistorySendBadge.REMOTE_DELETED, captureHistorySendBadge(status(MobileCaptureSubmissionPhaseDto.DELETED, receipt)))
        assertEquals(CaptureHistorySendBadge.SENT, captureHistorySendBadge(status(MobileCaptureSubmissionPhaseDto.DELETING, receipt)))
        assertEquals(null, captureHistorySendBadge(status(MobileCaptureSubmissionPhaseDto.PREPARED, receipt)))
    }

    private fun status(
        phase: MobileCaptureSubmissionPhaseDto,
        receipt: MobileCaptureSubmissionReceiptDto?,
    ) = MobileCaptureSubmissionStatusDto(
        operationId = "fixed-operation",
        phase = phase,
        origin = MobileCaptureSubmissionOriginDto.UNKNOWN,
        integrity = MobileCaptureIntegrityDto.Unknown,
        localPayloadState = MobileCaptureSubmissionLocalPayloadStateDto.RETAINED,
        categories = MobileCaptureSubmissionCategoriesDto(
            deviceIdentifiers = true, location = false, notes = false,
            annotations = false, settingsObservations = false,
        ),
        droppedMessages = 0uL,
        byteCount = 100uL,
        receipt = receipt,
        failure = null,
    )
}
