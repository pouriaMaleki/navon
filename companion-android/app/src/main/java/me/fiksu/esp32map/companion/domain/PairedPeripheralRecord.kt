package me.fiksu.esp32map.companion.domain

/**
 * Locally-persisted record of the BLE peripheral the companion is
 * bonded to. Cross-platform JSON contract (see
 * `parity-fixtures/data/paired_peripheral.json`):
 *
 * ```
 * { "identifier": "<platform-native peripheral id>",
 *   "friendlyName": "ESP32 Bike Minimap",
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
 */
data class PairedPeripheralRecord(
    val identifier: String,
    val friendlyName: String,
    val pairedAt: String,
)
