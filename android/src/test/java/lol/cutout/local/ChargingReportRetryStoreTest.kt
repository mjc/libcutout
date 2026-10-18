package lol.cutout.local

import lol.cutout.ChargingReportOperationStore
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

class ChargingReportRetryStoreTest {
    private data class Report(val spotId: String, val message: String)

    @Test
    fun retryAfterUnknownTransportOutcomeReusesOneImmutableOperationUntilReceipt() {
        var created = 0
        val closed = mutableListOf<Report>()
        val store = ChargingReportOperationStore(
            create = { spotId, message -> created++; Report(spotId, message) },
            close = { closed.add(it); Unit },
        )
        try {
            val firstAttempt = store.getOrCreate("spot-17", "Access changed")
            val retry = store.getOrCreate("spot-17", "edited after timeout")
            assertSame(firstAttempt, retry)
            assertEquals("Access changed", retry.message)
            assertEquals(1, created)
            assertEquals(emptyList<Report>(), closed)

            // A validated receipt is the only event that retires the operation.
            store.received("spot-17", retry)
            assertEquals(listOf(firstAttempt), closed)
            val nextReport = store.getOrCreate("spot-17", "New report")
            assertEquals("New report", nextReport.message)
            assertEquals(2, created)
        } finally {
            store.close()
        }
    }
}
