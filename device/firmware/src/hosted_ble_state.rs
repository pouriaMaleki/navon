//! Pure-Rust mirror of the GATT-server connection-tracking state machine
//! that lives in `firmware/components/hosted_ble/hosted_ble_route_sync.c`.
//!
//! The C handler can't be unit-tested directly without bringing up a
//! full Bluedroid stack. Mirroring the rule here lets us assert it
//! deterministically; the C handler is then a near-mechanical port of
//! `BleConnectionState::on_connect` / `on_disconnect`.
//!
//! The rule we're locking in: **at most one concurrent connection**.
//! A second connection arriving while the first is still alive is
//! rejected (the C handler calls `esp_ble_gap_disconnect` on the
//! incoming peer rather than overwriting `s_conn_id`); the existing
//! connection keeps running. This avoids notifications going to the
//! wrong peer when two phones race to connect.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BleConnectionState {
    Idle,
    Connected(u16),
}

impl Default for BleConnectionState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectAction {
    /// Accept this connection; transition to `Connected(conn_id)`.
    Accept,
    /// A connection is already active; reject the new one. The C
    /// handler should call `esp_ble_gap_disconnect` on the peer that
    /// produced this `conn_id` and leave the existing state untouched.
    RejectDuplicate(u16),
}

impl BleConnectionState {
    pub fn on_connect(&mut self, conn_id: u16) -> ConnectAction {
        match *self {
            Self::Idle => {
                *self = Self::Connected(conn_id);
                ConnectAction::Accept
            }
            Self::Connected(_) => ConnectAction::RejectDuplicate(conn_id),
        }
    }

    pub fn on_disconnect(&mut self, conn_id: u16) {
        if let Self::Connected(active) = *self {
            if active == conn_id {
                *self = Self::Idle;
            }
            // Disconnect events for a `conn_id` other than the active
            // one are stale (e.g., the peer we rejected in
            // `on_connect` later disconnects naturally). Ignoring them
            // keeps the active connection alive.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_while_idle_accepts_and_stores_conn_id() {
        let mut state = BleConnectionState::default();
        assert_eq!(state.on_connect(7), ConnectAction::Accept);
        assert_eq!(state, BleConnectionState::Connected(7));
    }

    #[test]
    fn second_connect_while_busy_returns_reject_and_keeps_existing() {
        let mut state = BleConnectionState::Connected(7);
        let action = state.on_connect(8);
        assert_eq!(
            action,
            ConnectAction::RejectDuplicate(8),
            "the duplicate's conn_id must travel with the action so the C handler \
             knows which peer to disconnect",
        );
        assert_eq!(
            state,
            BleConnectionState::Connected(7),
            "the existing conn_id must be preserved — never silently overwritten",
        );
    }

    #[test]
    fn disconnect_clears_state_back_to_idle() {
        let mut state = BleConnectionState::Connected(7);
        state.on_disconnect(7);
        assert_eq!(state, BleConnectionState::Idle);
    }

    #[test]
    fn disconnect_with_mismatched_conn_id_is_noop() {
        let mut state = BleConnectionState::Connected(7);
        state.on_disconnect(8);
        assert_eq!(
            state,
            BleConnectionState::Connected(7),
            "stale disconnect events from a rejected duplicate must not knock the \
             active connection offline",
        );
    }

    #[test]
    fn disconnect_while_idle_is_noop() {
        let mut state = BleConnectionState::default();
        state.on_disconnect(42);
        assert_eq!(state, BleConnectionState::Idle);
    }
}

/// Bluedroid `adv_filter_policy` value the C side should program after
/// every state transition. While unbonded the device must accept any
/// scanner / connection request so the companion can find the QR's
/// peripheral; once bonded the allowlist is locked to the bonded peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvertisingFilterPolicy {
    /// `ADV_FILTER_ALLOW_SCAN_ANY_CON_ANY` — open advertising for the
    /// QR-OOB pairing window.
    AllowAny,
    /// `ADV_FILTER_ALLOW_SCAN_WLST_CON_WLST` — accepts scans and
    /// connections only from the bonded peer in the controller's
    /// whitelist (loaded by the C side via
    /// `esp_ble_gap_update_whitelist`).
    WhitelistOnly,
}

/// Pick the right adv-filter policy based on whether the device is
/// bonded. Single source of truth — the C handler reads this and
/// reprograms `s_adv_params.adv_filter_policy` after every pairing
/// transition.
pub fn advertising_filter_policy_for(is_bonded: bool) -> AdvertisingFilterPolicy {
    if is_bonded {
        AdvertisingFilterPolicy::WhitelistOnly
    } else {
        AdvertisingFilterPolicy::AllowAny
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn unbonded_advertising_accepts_any_scanner() {
        assert_eq!(
            advertising_filter_policy_for(false),
            AdvertisingFilterPolicy::AllowAny,
            "the QR-OOB pairing window has to be open or the companion \
             can't discover the peripheral on its first connect",
        );
    }

    #[test]
    fn bonded_advertising_locks_to_whitelist() {
        assert_eq!(
            advertising_filter_policy_for(true),
            AdvertisingFilterPolicy::WhitelistOnly,
            "once bonded, only the paired phone's BD_ADDR (in the \
             controller's whitelist) should be able to even scan us",
        );
    }
}
