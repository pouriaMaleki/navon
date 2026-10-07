package app.navon.bike.domain

/**
 * The kind of handlebar device a [PairedPeripheralRecord] is bonded to.
 *
 * navon supports one *active* device at a time, but it can be either the
 * first-party Navon ESP32 firmware (route-sync over the custom GATT service
 * in `integration/ble/`) or a third-party Beeline Velo2/Moto2 (turn-by-turn
 * over the Beeline protocol in `integration/ble/navdevice/beeline/`). The
 * device type on the persisted record decides which transport drives the
 * route + GPS once connected.
 */
enum class PairedDeviceType(val displayName: String) {
    /** First-party Navon ESP32 handlebar display (QR-OOB pairing, route-sync protocol). */
    NAVON_ESP32("Navon"),

    /** Beeline Velo2 / Moto2 handlebar navigation device (BLE-scan pairing, Beeline protocol). */
    BEELINE("Beeline"),
}
