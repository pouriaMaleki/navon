//! QR-OOB pairing state machine.
//!
//! Pure Rust, no esp-idf deps — host-testable. The C-side GATT handler
//! drives this via `firmware/src/app.rs`: each frame the runtime calls
//! `tick(now)` and drains the inbound `pairing_confirm` queue; matching
//! a secret transitions from `Pairing` to `Operational`.
//!
//! Companion-side complement:
//!
//! 1. Device boots unpaired → renders the current QR payload on the
//!    panel via `pairing_overlay::render_pairing_qr`.
//! 2. Companion scans the QR, parses out `(peripheral_address,
//!    ephemeral_secret)`.
//! 3. Companion connects (BLE Just Works pairing kicks in
//!    automatically when a write hits the encrypted characteristic),
//!    then writes the 32-byte secret to the `pairing_confirm`
//!    characteristic.
//! 4. Device receives the write → `on_pairing_confirm` matches the
//!    secret → state transitions to `Operational(peer_identity)` and
//!    the bond is persisted to NVS.
//!
//! Anti-replay: the secret rotates every `ROTATION_PERIOD` while the
//! device is in pairing mode, so a stale photo of the QR can't be
//! replayed later.

use std::time::Duration;

/// How often the QR's ephemeral secret rotates while we're in pairing
/// mode. 60s balances anti-replay against giving the user enough time
/// to fumble with the camera.
pub const ROTATION_PERIOD: Duration = Duration::from_secs(60);

/// Length of the ephemeral pairing secret carried in the QR payload
/// and written back over `pairing_confirm`.
pub const SECRET_LEN: usize = 32;

/// Length of the peripheral's BLE address (BD_ADDR).
pub const PERIPHERAL_ADDRESS_LEN: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub mode: Mode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// No bond stored. The device shows a QR and accepts only the
    /// `pairing_confirm` characteristic; route writes are dropped.
    Pairing {
        secret: [u8; SECRET_LEN],
        rotated_at: Duration,
    },
    /// A bond has been established. Route writes are accepted again;
    /// `pairing_confirm` is rejected (already bonded — single-bond
    /// policy).
    Operational {
        peer_identity: [u8; 16],
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    /// No state change.
    None,
    /// `on_pairing_confirm` matched: bond established, persist
    /// `peer_identity` to NVS, route writes start being accepted.
    Bonded { peer_identity: [u8; 16] },
    /// `on_pairing_confirm` mismatch. State unchanged; companion
    /// should re-prompt the user (probably stale QR scan).
    Reject,
    /// Auth failure from Bluedroid (the bonded phone forgot us, or the
    /// IRK didn't match). Drop the bond and return to pairing mode.
    ClearBondAndPair,
    /// Single-bond policy: an unbonded peer connected while the device
    /// is already `Operational`. The C handler should call
    /// `esp_ble_gap_disconnect` immediately so the peer can't sit on
    /// the connection probing the encrypted characteristics.
    DropConnection,
}

#[derive(Debug, Clone)]
pub struct PairingStateMachine {
    state: State,
    peripheral_address: [u8; PERIPHERAL_ADDRESS_LEN],
}

impl PairingStateMachine {
    /// Build an unpaired machine with a fresh secret. Caller supplies
    /// `now` (typically `App::monotonic_clock`) so all internal
    /// timestamps share one clock source.
    pub fn new_unpaired(
        peripheral_address: [u8; PERIPHERAL_ADDRESS_LEN],
        initial_secret: [u8; SECRET_LEN],
        now: Duration,
    ) -> Self {
        Self {
            state: State {
                mode: Mode::Pairing {
                    secret: initial_secret,
                    rotated_at: now,
                },
            },
            peripheral_address,
        }
    }

    /// Build a state machine seeded from a persisted bond.
    pub fn new_paired(
        peripheral_address: [u8; PERIPHERAL_ADDRESS_LEN],
        peer_identity: [u8; 16],
    ) -> Self {
        Self {
            state: State {
                mode: Mode::Operational { peer_identity },
            },
            peripheral_address,
        }
    }

    pub fn mode(&self) -> Mode {
        self.state.mode
    }

    pub fn is_paired(&self) -> bool {
        matches!(self.state.mode, Mode::Operational { .. })
    }

    /// Advance the clock. While in `Pairing`, rotate the secret if
    /// `ROTATION_PERIOD` has elapsed since the last rotation. The
    /// caller supplies a `next_secret` factory so tests can inject
    /// deterministic bytes; production wires it to a CSPRNG.
    pub fn tick<R>(&mut self, now: Duration, mut next_secret: R)
    where
        R: FnMut() -> [u8; SECRET_LEN],
    {
        if let Mode::Pairing { rotated_at, .. } = self.state.mode {
            if now.saturating_sub(rotated_at) >= ROTATION_PERIOD {
                self.state.mode = Mode::Pairing {
                    secret: next_secret(),
                    rotated_at: now,
                };
            }
        }
    }

    /// Bytes encoded into the QR shown on the panel:
    /// `peripheral_address(6) || ephemeral_secret(32)` = 38 bytes.
    /// `None` when the device is already bonded.
    pub fn current_qr_payload(&self) -> Option<Vec<u8>> {
        match self.state.mode {
            Mode::Pairing { secret, .. } => {
                let mut buf =
                    Vec::with_capacity(PERIPHERAL_ADDRESS_LEN + SECRET_LEN);
                buf.extend_from_slice(&self.peripheral_address);
                buf.extend_from_slice(&secret);
                Some(buf)
            }
            Mode::Operational { .. } => None,
        }
    }

    /// Companion wrote the ephemeral secret to the `pairing_confirm`
    /// characteristic. Match it against the current QR's secret.
    /// `peer_identity_factory` is invoked only on success — typically
    /// returns the SHA-256-truncated digest of the connecting peer's
    /// BD_ADDR + IRK.
    pub fn on_pairing_confirm<I>(
        &mut self,
        payload: &[u8],
        peer_identity: I,
    ) -> Transition
    where
        I: FnOnce() -> [u8; 16],
    {
        let Mode::Pairing { secret, .. } = self.state.mode else {
            // Already paired — reject. Single-bond policy: the user
            // has to Forget the existing bond first.
            return Transition::Reject;
        };
        if payload.len() != SECRET_LEN || payload != secret {
            return Transition::Reject;
        }
        let identity = peer_identity();
        self.state.mode = Mode::Operational {
            peer_identity: identity,
        };
        Transition::Bonded {
            peer_identity: identity,
        }
    }

    /// A peer attempted to connect without going through the bond
    /// flow. While unpaired, this is the normal pairing path (the
    /// caller still has to `on_pairing_confirm` to land a bond), so
    /// we return `None` here. Once `Operational`, the policy is
    /// single-bond: drop the connection immediately so the peer can't
    /// camp on the encrypted characteristics waiting for the SMP
    /// timeout.
    pub fn on_unbonded_connect_attempt(&self) -> Transition {
        if matches!(self.state.mode, Mode::Operational { .. }) {
            Transition::DropConnection
        } else {
            Transition::None
        }
    }

    /// Auth failed (bonded peer forgot us, IRK mismatch, etc.). Drop
    /// the bond and re-enter pairing mode. Caller is expected to wipe
    /// NVS + re-init the QR with a fresh secret on `tick`'s next call
    /// (the secret is forced to rotate by setting `rotated_at` to a
    /// time that's already past the rotation period).
    pub fn on_auth_failure(&mut self) -> Transition {
        if matches!(self.state.mode, Mode::Operational { .. }) {
            self.state.mode = Mode::Pairing {
                secret: [0u8; SECRET_LEN],
                // Force the next `tick` to rotate immediately.
                rotated_at: Duration::ZERO,
            };
            return Transition::ClearBondAndPair;
        }
        Transition::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_addr() -> [u8; 6] {
        [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]
    }

    fn first_secret() -> [u8; 32] {
        [0x01; 32]
    }

    fn second_secret() -> [u8; 32] {
        [0x02; 32]
    }

    #[test]
    fn current_qr_payload_concatenates_address_and_secret() {
        let machine = PairingStateMachine::new_unpaired(
            fixed_addr(),
            first_secret(),
            Duration::ZERO,
        );
        let payload = machine.current_qr_payload().expect("pairing mode");
        assert_eq!(payload.len(), PERIPHERAL_ADDRESS_LEN + SECRET_LEN);
        assert_eq!(&payload[..6], &fixed_addr());
        assert_eq!(&payload[6..], &first_secret());
    }

    #[test]
    fn current_qr_payload_is_none_when_paired() {
        let machine = PairingStateMachine::new_paired(fixed_addr(), [0x9A; 16]);
        assert!(machine.current_qr_payload().is_none());
    }

    #[test]
    fn secret_rotates_after_rotation_period_in_pairing() {
        let mut machine = PairingStateMachine::new_unpaired(
            fixed_addr(),
            first_secret(),
            Duration::from_secs(0),
        );
        let before = machine.current_qr_payload().expect("pairing");
        // Just under the rotation window — must NOT rotate.
        machine.tick(ROTATION_PERIOD - Duration::from_secs(1), || second_secret());
        let inside = machine.current_qr_payload().expect("still pairing");
        assert_eq!(inside, before, "must not rotate before period elapses");
        // Past the rotation window — MUST rotate.
        machine.tick(ROTATION_PERIOD + Duration::from_secs(1), || second_secret());
        let after = machine.current_qr_payload().expect("still pairing");
        assert_ne!(after, before, "secret must change once the window elapses");
        assert_eq!(&after[6..], &second_secret());
    }

    #[test]
    fn matching_secret_transitions_to_bonded() {
        let mut machine = PairingStateMachine::new_unpaired(
            fixed_addr(),
            first_secret(),
            Duration::ZERO,
        );
        let identity = [0x42; 16];
        let transition = machine.on_pairing_confirm(&first_secret(), || identity);
        assert_eq!(transition, Transition::Bonded { peer_identity: identity });
        assert!(machine.is_paired());
        assert!(machine.current_qr_payload().is_none());
    }

    #[test]
    fn wrong_secret_keeps_state_pairing_and_rejects() {
        let mut machine = PairingStateMachine::new_unpaired(
            fixed_addr(),
            first_secret(),
            Duration::ZERO,
        );
        let transition = machine.on_pairing_confirm(&second_secret(), || [0; 16]);
        assert_eq!(transition, Transition::Reject);
        assert!(!machine.is_paired());
        // The secret is unchanged so a re-scan of the same QR still
        // works on the next try (the user fixed their camera).
        let payload = machine.current_qr_payload().unwrap();
        assert_eq!(&payload[6..], &first_secret());
    }

    #[test]
    fn pairing_confirm_while_already_operational_rejects() {
        // Single-bond policy: the user must Forget first.
        let mut machine = PairingStateMachine::new_paired(fixed_addr(), [0x42; 16]);
        let transition = machine.on_pairing_confirm(&[0u8; SECRET_LEN], || [0; 16]);
        assert_eq!(transition, Transition::Reject);
        assert!(machine.is_paired());
    }

    #[test]
    fn auth_failure_clears_bond_and_returns_to_pairing() {
        let mut machine = PairingStateMachine::new_paired(fixed_addr(), [0x42; 16]);
        let transition = machine.on_auth_failure();
        assert_eq!(transition, Transition::ClearBondAndPair);
        assert!(!machine.is_paired());
        // The next tick rotates the secret immediately (rotated_at
        // is `Duration::ZERO`). Confirms the device is ready to show
        // a fresh QR even without waiting for a full rotation period.
        machine.tick(Duration::from_secs(60), || second_secret());
        let payload = machine.current_qr_payload().expect("pairing mode");
        assert_eq!(&payload[6..], &second_secret());
    }

    #[test]
    fn on_unbonded_connect_attempt_while_operational_drops_connection() {
        let machine = PairingStateMachine::new_paired(fixed_addr(), [0xAB; 16]);
        assert_eq!(
            machine.on_unbonded_connect_attempt(),
            Transition::DropConnection,
            "single-bond policy: the bonded peer is the only one allowed; \
             everyone else gets dropped at the connection layer",
        );
    }

    #[test]
    fn on_unbonded_connect_attempt_while_pairing_is_noop() {
        let machine = PairingStateMachine::new_unpaired(
            fixed_addr(),
            first_secret(),
            Duration::ZERO,
        );
        assert_eq!(
            machine.on_unbonded_connect_attempt(),
            Transition::None,
            "during the pairing window any connection is welcome — the \
             companion is expected to write `pairing_confirm` next",
        );
    }

    #[test]
    fn auth_failure_while_unpaired_is_noop() {
        let mut machine = PairingStateMachine::new_unpaired(
            fixed_addr(),
            first_secret(),
            Duration::ZERO,
        );
        let transition = machine.on_auth_failure();
        assert_eq!(transition, Transition::None);
    }
}
