package lol.cutout

import uniffi.cutout_mobile_ffi.MobileChargingConnectorDto

/** Resource mapping follows ChargingSpotPresentation.connectorAssetIdentifier in Swift. */
internal object ChargingConnectorArtwork {
    fun drawable(connector: MobileChargingConnectorDto): Int = when (connector) {
        MobileChargingConnectorDto.TypeA -> R.drawable.charging_connector_type_a
        MobileChargingConnectorDto.TypeC -> R.drawable.charging_connector_type_c
        MobileChargingConnectorDto.TypeD -> R.drawable.charging_connector_type_d
        MobileChargingConnectorDto.TypeE -> R.drawable.charging_connector_type_e
        MobileChargingConnectorDto.TypeF -> R.drawable.charging_connector_type_f
        MobileChargingConnectorDto.TypeG -> R.drawable.charging_connector_type_g
        MobileChargingConnectorDto.TypeH -> R.drawable.charging_connector_type_h
        MobileChargingConnectorDto.TypeI -> R.drawable.charging_connector_type_i
        MobileChargingConnectorDto.TypeJ -> R.drawable.charging_connector_type_j
        MobileChargingConnectorDto.TypeK -> R.drawable.charging_connector_type_k
        MobileChargingConnectorDto.TypeL -> R.drawable.charging_connector_type_l
        MobileChargingConnectorDto.TypeM -> R.drawable.charging_connector_type_m
        MobileChargingConnectorDto.TypeN -> R.drawable.charging_connector_type_n
        MobileChargingConnectorDto.TypeO -> R.drawable.charging_connector_type_o
        MobileChargingConnectorDto.Nema515 -> R.drawable.charging_connector_nema_5_15
        MobileChargingConnectorDto.Nema520 -> R.drawable.charging_connector_nema_5_20
        MobileChargingConnectorDto.Nema1430 -> R.drawable.charging_connector_nema_14_30
        MobileChargingConnectorDto.Nema1450 -> R.drawable.charging_connector_nema_14_50
        MobileChargingConnectorDto.Nema1030 -> R.drawable.charging_connector_nema_10_30
        MobileChargingConnectorDto.Nema650 -> R.drawable.charging_connector_nema_6_50
        MobileChargingConnectorDto.Nema1050 -> R.drawable.charging_connector_nema_10_50
        MobileChargingConnectorDto.J1772 -> R.drawable.charging_connector_j1772
        MobileChargingConnectorDto.Type2 -> R.drawable.charging_connector_type_2
        MobileChargingConnectorDto.Ccs1 -> R.drawable.charging_connector_ccs_1
        MobileChargingConnectorDto.Ccs2 -> R.drawable.charging_connector_ccs_2
        MobileChargingConnectorDto.Chademo -> R.drawable.charging_connector_chademo
        MobileChargingConnectorDto.Nacs -> R.drawable.charging_connector_nacs
        MobileChargingConnectorDto.Unknown, is MobileChargingConnectorDto.Other -> R.drawable.charging_connector_unknown
    }

    fun label(ui: CutoutUi, connector: MobileChargingConnectorDto): String = when (connector) {
        MobileChargingConnectorDto.TypeA -> ui.label("charging.connector.type_a")
        MobileChargingConnectorDto.TypeC -> ui.label("charging.connector.type_c")
        MobileChargingConnectorDto.TypeD -> ui.label("charging.connector.type_d")
        MobileChargingConnectorDto.TypeE -> ui.label("charging.connector.type_e")
        MobileChargingConnectorDto.TypeF -> ui.label("charging.connector.type_f")
        MobileChargingConnectorDto.TypeG -> ui.label("charging.connector.type_g")
        MobileChargingConnectorDto.TypeH -> ui.label("charging.connector.type_h")
        MobileChargingConnectorDto.TypeI -> ui.label("charging.connector.type_i")
        MobileChargingConnectorDto.TypeJ -> ui.label("charging.connector.type_j")
        MobileChargingConnectorDto.TypeK -> ui.label("charging.connector.type_k")
        MobileChargingConnectorDto.TypeL -> ui.label("charging.connector.type_l")
        MobileChargingConnectorDto.TypeM -> ui.label("charging.connector.type_m")
        MobileChargingConnectorDto.TypeN -> ui.label("charging.connector.type_n")
        MobileChargingConnectorDto.TypeO -> ui.label("charging.connector.type_o")
        MobileChargingConnectorDto.Nema515 -> ui.label("charging.connector.nema_5_15")
        MobileChargingConnectorDto.Nema520 -> ui.label("charging.connector.nema_5_20")
        MobileChargingConnectorDto.Nema1430 -> ui.label("charging.connector.nema_14_30")
        MobileChargingConnectorDto.Nema1450 -> ui.label("charging.connector.nema_14_50")
        MobileChargingConnectorDto.Nema1030 -> ui.label("charging.connector.nema_10_30")
        MobileChargingConnectorDto.Nema650 -> ui.label("charging.connector.nema_6_50")
        MobileChargingConnectorDto.Nema1050 -> ui.label("charging.connector.nema_10_50")
        MobileChargingConnectorDto.J1772 -> ui.label("charging.connector.j1772")
        MobileChargingConnectorDto.Type2 -> ui.label("charging.connector.type2")
        MobileChargingConnectorDto.Ccs1 -> ui.label("charging.connector.ccs1")
        MobileChargingConnectorDto.Ccs2 -> ui.label("charging.connector.ccs2")
        MobileChargingConnectorDto.Chademo -> ui.label("charging.connector.chademo")
        MobileChargingConnectorDto.Nacs -> ui.label("charging.connector.nacs")
        MobileChargingConnectorDto.Unknown -> ui.label("charging.connector.unknown")
        is MobileChargingConnectorDto.Other -> connector.label
    }
}
