package lol.cutout

import android.content.Context
import android.location.Geocoder
import android.os.Handler
import android.os.Looper
import uniffi.cutout_mobile_ffi.MobileChargingConnectorDto
import uniffi.cutout_mobile_ffi.MobileMapCoordinateDto
import java.io.Closeable
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** Presentation context for the charging editor, resolved only from a fresh phone location. */
data class ChargingEditorRegionalContext(
    val countryCode: String?,
    val connectorChoices: List<MobileChargingConnectorDto>?,
)

/**
 * Resolves the native reverse-geocoding effect and then asks Rust for the nearby connector
 * ordering. It deliberately has no locale or language fallback when location is unavailable.
 */
class ChargingEditorContextResolver internal constructor(
    private val reverseGeocode: (Double, Double) -> String?,
    private val loadChoices: (
        MobileMapCoordinateDto,
        String?,
        (Result<List<MobileChargingConnectorDto>>) -> Unit,
    ) -> Unit,
) : Closeable {
    constructor(context: Context, client: AndroidChargingClient) : this(
        { latitude, longitude ->
            if (!Geocoder.isPresent()) null
            else Geocoder(context.applicationContext).getFromLocation(latitude, longitude, 1)
                ?.firstOrNull()?.countryCode
        },
        { location, countryCode, completed ->
            client.withDatabase({ database ->
                database.chargingConnectorChoicesNear(location, countryCode)
            }, completed)
        },
    )

    private val main = Handler(Looper.getMainLooper())
    private val worker = ThreadPoolExecutor(
        1, 1, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(4),
        { task -> Thread(task, "CutoutChargingEditorContext").apply { isDaemon = true } },
        ThreadPoolExecutor.AbortPolicy(),
    )
    private val closed = AtomicBoolean(false)
    private val generationLock = Any()
    private var generation = 0L

    /** Each request invalidates its predecessor; closing the returned handle invalidates it too. */
    fun resolve(
        location: MobileMapCoordinateDto?,
        completed: (ChargingEditorRegionalContext) -> Unit,
    ): Closeable {
        val request = synchronized(generationLock) { ++generation }
        val cancelled = AtomicBoolean(false)
        fun isCurrent() = !closed.get() && !cancelled.get() && synchronized(generationLock) { request == generation }
        if (location == null) {
            main.post { if (isCurrent()) completed(ChargingEditorRegionalContext(null, null)) }
            return Closeable { cancelled.set(true) }
        }
        try {
            worker.execute {
                val countryCode = runCatching {
                    reverseGeocode(location.latitudeDegrees, location.longitudeDegrees)
                }.getOrNull()
                if (!isCurrent()) return@execute
                try {
                    loadChoices(location, countryCode) { choices ->
                        main.post {
                            if (isCurrent()) completed(ChargingEditorRegionalContext(countryCode, choices.getOrNull()))
                        }
                    }
                } catch (_: Exception) {
                    main.post { if (isCurrent()) completed(ChargingEditorRegionalContext(countryCode, null)) }
                }
            }
        } catch (_: RejectedExecutionException) {
            main.post { if (isCurrent()) completed(ChargingEditorRegionalContext(null, null)) }
        }
        return Closeable { cancelled.set(true) }
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        synchronized(generationLock) { generation++ }
        worker.shutdownNow()
    }
}
