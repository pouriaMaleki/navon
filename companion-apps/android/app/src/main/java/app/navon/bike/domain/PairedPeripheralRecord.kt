package app.navon.bike.domain

/**
 * Locally-persisted record of the BLE peripheral the companion is
 * bonded to. Cross-platform JSON contract (see
 * `parity-fixtures/data/paired_peripheral.json`):
 *
 * ```
 * { "identifier": "<platform-native peripheral id>",
 *   "friendlyName": "Navon",
 *   "pairedAt": "<ISO-8601 timestamp>" }
 * ```
 *
 * On Android, [identifier] is the Bluetooth MAC address (`AA:BB:CC:DD:EE:FF`);
 * on iOS, it's the `peripheral.identifier.uuidString`. We persist the
 * platform-native form so each side can reuse its OS's stable handle for
 * fast-path reconnects without scanning.
 *
 * [pairedAt] is an ISO-8601 string (with milliseconds and the `Z` zone
 * suffix) so the format survives a round-trip through both iOS's
 * `JSONEncoder.dateEncodingStrategy = .iso8601` and Android's `Gson`
 * default `Date` serializer without drift.
 *
 * [deviceType] distinguishes the first-party Navon ESP32 from a third-party
 * Beeline (see [PairedDeviceType]). It is nullable so records written before
 * Beeline support existed — which have no `deviceType` key — still
 * deserialize cleanly; [effectiveDeviceType] resolves the absent/`null` case
 * to [PairedDeviceType.NAVON_ESP32], the only device the old schema could
 * represent. iOS reads the same key and applies the same default.
 */
data class PairedPeripheralRecord(
    val identifier: String,
    val friendlyName: String,
    val pairedAt: String,
    val deviceType: PairedDeviceType? = null,
) {
    /** [deviceType] with the legacy/absent case resolved to the ESP32. */
    val effectiveDeviceType: PairedDeviceType
        get() = deviceType ?: PairedDeviceType.NAVON_ESP32
}
