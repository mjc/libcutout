package lol.cutout

import android.bluetooth.BluetoothDevice
import android.bluetooth.le.ScanResult
import uniffi.cutout_mobile_ffi.DiscoveryObservation
import uniffi.cutout_mobile_ffi.DiscoveryServiceUuid

/** Native scan data forwarded to the Rust discovery owner without retaining advertisement payloads. */
internal data class AndroidBleScanObservation(
    val device: BluetoothDevice,
    val name: String?,
    val rssi: Int,
    val services: List<DiscoveryServiceUuid>,
) {
    val platformIdentifier: String get() = device.address

    fun toRustObservation() = DiscoveryObservation(
        platformIdentifier = platformIdentifier,
        advertisedName = name?.encodeToByteArray(),
        advertisedServiceUuids = services,
        manufacturerData = emptyList(),
        rssiDbm = rssi.toShort(),
    )

    fun toNearbyPev(fallbackName: String) = NearbyPev(
        id = platformIdentifier,
        name = name ?: fallbackName,
        rssi = rssi,
        suggested = false,
        advertisedName = name,
        advertisedServices = services.map { it.bytes },
    )
}

internal fun androidBleScanObservation(result: ScanResult): AndroidBleScanObservation {
    val record = result.scanRecord
    return AndroidBleScanObservation(
        device = result.device,
        name = record?.deviceName,
        rssi = result.rssi,
        services = record?.serviceUuids?.map { DiscoveryServiceUuid(it.uuid.wireBytes()) }.orEmpty(),
    )
}
