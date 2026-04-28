//! Rust binding for the `hosted_ble` C component.
//!
//! On the ESP32-P4 the BLE controller runs on an on-board ESP32-C6 reached
//! over SDIO via `espressif/esp_hosted`. The Rust types in `esp_idf_ble.rs`
//! can't be used here because `esp-idf-svc::bt::BtDriver` requires a
//! `Modem` peripheral that doesn't exist on the P4. Instead the GATT
//! server lives in `components/hosted_ble/hosted_ble_route_sync.c` and
//! this module just plumbs:
//!
//!   * an inbound chunk callback → a Mutex<VecDeque> the runtime drains
//!     each frame in `RouteSyncIo::poll_chunk`;
//!   * `RouteSyncIo::publish_messages` → `hosted_ble_route_sync_notify`,
//!     skipping silently when no companion is connected.
//!
//! Wire format (BLE packet envelope, route-sync canonical messages, chunk
//! reassembly) is shared with the host build through `route_sync_ble.rs`
//! and `route_sync.rs`, so the same tests cover both paths.
//!
//! ## Graceful degradation when the C6 isn't ready
//!
//! `hosted_ble_init` will fail (most commonly with `ESP_FAIL` or an RPC
//! timeout) when the on-board ESP32-C6 is running stock factory firmware
//! instead of the matching `esp_hosted` slave image. We treat that as a
//! soft failure: the device boots and runs the rendering / GPS / touch
//! stack as usual, BLE stays offline, and the runtime sees a no-op
//! `RouteSyncIo`. This keeps the rest of the device usable while the user
//! flashes the C6 — see `docs/ble-route-sync-contract.md` for the slave
//! firmware bring-up steps.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use runtime_core::api::RouteSyncMessage;

use crate::hosted_ble_queue::{enqueue_chunk, enqueue_pairing_secret, QueueFullError};
use crate::pairing::SECRET_LEN;
use crate::platform::{AuthCmplOutcome, RouteSyncIo, RouteSyncIoError};
use crate::route_sync::RouteTransferChunk;
use crate::route_sync_ble::{BleRouteSyncPacket, decode_ble_packet, encode_ble_packet};

static INBOUND: OnceLock<Mutex<VecDeque<RouteTransferChunk>>> = OnceLock::new();
static PAIRING_INBOUND: OnceLock<Mutex<VecDeque<[u8; SECRET_LEN]>>> = OnceLock::new();
/// Auth-completion events the GAP handler pushes from the BT host
/// task. The runtime task drains them once per frame and forwards
/// the outcome to `App::ingest_auth_cmpl`.
static AUTH_CMPL_INBOUND: OnceLock<Mutex<VecDeque<AuthCmplEvent>>> = OnceLock::new();

/// Mirror of the C `hosted_ble_auth_cmpl_cb_t` payload. Carried
/// across the BT host → runtime task boundary via a queue so the App
/// layer doesn't run inside the Bluedroid callback.
#[derive(Debug, Clone, Copy)]
pub struct AuthCmplEvent {
    pub success: bool,
    pub fail_reason: u8,
    pub peer_addr: [u8; 6],
    pub addr_type: u8,
}

/// Set by the C `pairing_request` write trampoline; consumed by
/// `App::step_frame` to open the QR-display window. Lock-free atomic
/// because the producer runs in the BT host task and the consumer runs
/// in the runtime task.
static PAIRING_REQUEST_PENDING: AtomicBool = AtomicBool::new(false);

/// Whether the device is currently in pairing mode. Read from the C
/// pairing-confirm write path (under the BT host task) and updated by
/// the runtime when the pairing state machine transitions; using an
/// atomic dodges the lock-ordering footgun of grabbing the runtime
/// state mutex from inside a Bluedroid callback.
static PAIRING_MODE: AtomicBool = AtomicBool::new(true);

pub fn set_pairing_mode(value: bool) {
    PAIRING_MODE.store(value, Ordering::SeqCst);
}

pub fn pairing_mode() -> bool {
    PAIRING_MODE.load(Ordering::SeqCst)
}

fn inbound() -> &'static Mutex<VecDeque<RouteTransferChunk>> {
    INBOUND.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn pairing_inbound() -> &'static Mutex<VecDeque<[u8; SECRET_LEN]>> {
    PAIRING_INBOUND.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn auth_cmpl_inbound() -> &'static Mutex<VecDeque<AuthCmplEvent>> {
    AUTH_CMPL_INBOUND.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Drain one queued pairing-confirm secret if any are available. Called
/// once per frame from `App::step_frame` while the device is in pairing
/// mode; the secret is then matched against the current QR's secret by
/// `PairingStateMachine::on_pairing_confirm`.
pub fn drain_pairing_secret() -> Option<[u8; SECRET_LEN]> {
    pairing_inbound().lock().ok().and_then(|mut q| q.pop_front())
}

/// Atomically clear and return whether a pairing-request write was
/// pending since the last drain. The platform layer calls this once
/// per frame and forwards the signal to `App::request_qr_display`.
pub fn drain_pairing_request() -> bool {
    PAIRING_REQUEST_PENDING.swap(false, Ordering::SeqCst)
}

/// Pull the next queued SMP auth-completion event. The platform layer
/// forwards each one to `App::ingest_auth_cmpl` so the bond is
/// persisted on success / dropped on failure.
pub fn drain_auth_cmpl() -> Option<AuthCmplEvent> {
    auth_cmpl_inbound().lock().ok().and_then(|mut q| q.pop_front())
}

/// Lock the controller's advertising-filter policy to the bonded peer
/// (whitelist-only) or open it back up to any scanner. Wraps
/// `hosted_ble_route_sync_set_adv_filter` from the C side.
pub fn set_adv_filter_policy(whitelist_only: bool, peer_addr: &[u8; 6], addr_type: u8) {
    let err = unsafe {
        esp_idf_svc::sys::hosted_ble_route_sync_set_adv_filter(
            whitelist_only,
            peer_addr.as_ptr(),
            addr_type,
        )
    };
    if err != esp_idf_svc::sys::ESP_OK as i32 {
        log::warn!(
            "hosted_ble_route_sync_set_adv_filter(whitelist_only={whitelist_only}) -> err={err:#x}"
        );
    } else {
        log::info!(
            "advertising filter set: whitelist_only={whitelist_only} peer={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            peer_addr[0],
            peer_addr[1],
            peer_addr[2],
            peer_addr[3],
            peer_addr[4],
            peer_addr[5],
        );
    }
}

/// Route-sync IO backed by the hosted-BLE GATT server in C.
///
/// Two modes:
///
/// * `Active` — `hosted_ble_route_sync_start` returned `ESP_OK`; the GATT
///   service is advertising and chunk writes flow through.
/// * `Inactive` — bring-up failed (typically because the on-board C6
///   isn't running matching `esp_hosted` slave firmware). The runtime
///   sees a no-op transport so the rest of the device keeps working.
///
/// Drop is intentionally a no-op — when active, the BLE stack stays up
/// for the rest of the process lifetime, matching how the runtime loop
/// is structured (the device entrypoint never returns).
#[derive(Debug)]
pub enum HostedBleRouteSyncIo {
    Active,
    Inactive,
}

impl HostedBleRouteSyncIo {
    /// Try to start the hosted BLE GATT server. Always returns a usable
    /// `RouteSyncIo`: on failure the runtime gets a no-op transport and a
    /// warning log line is emitted instead of bubbling the error up to
    /// the device entrypoint (which would put the firmware in a panic
    /// loop).
    pub fn start_or_fallback() -> Self {
        // Touch the queues so the C callback can lock without racing
        // on OnceLock initialisation under the BT host task.
        let _ = inbound();
        let _ = pairing_inbound();
        let _ = auth_cmpl_inbound();

        let cbs = esp_idf_svc::sys::hosted_ble_route_sync_callbacks_t {
            on_chunk: Some(on_chunk_trampoline),
            on_pairing_confirm: Some(on_pairing_confirm_trampoline),
            on_pairing_request: Some(on_pairing_request_trampoline),
            is_pairing_mode: Some(is_pairing_mode_trampoline),
            on_auth_cmpl: Some(on_auth_cmpl_trampoline),
            ctx: std::ptr::null_mut(),
        };

        let err = unsafe { esp_idf_svc::sys::hosted_ble_route_sync_start(&cbs) };
        if err == esp_idf_svc::sys::ESP_OK as i32 {
            log::info!("hosted-ble: route-sync GATT server online");
            Self::Active
        } else {
            log::warn!(
                "hosted-ble: bring-up failed (err={err:#x}); device runs without BLE. \
                 Most common cause: the on-board ESP32-C6 isn't running matching \
                 esp_hosted slave firmware. Flash it from \
                 `<build>/managed_components/espressif__esp_hosted/slave/` — see \
                 docs/ble-route-sync-contract.md for steps."
            );
            Self::Inactive
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

unsafe extern "C" fn on_chunk_trampoline(data: *const u8, len: usize, _ctx: *mut c_void) {
    if data.is_null() || len == 0 {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let Ok(packet) = decode_ble_packet(bytes) else {
        log::warn!("hosted-ble: dropped malformed packet ({len} bytes)");
        return;
    };
    let BleRouteSyncPacket::Chunk(chunk) = packet else {
        // The contract expects only chunk packets on the write characteristic.
        log::warn!("hosted-ble: dropped non-chunk packet on chunk characteristic");
        return;
    };
    if let Ok(mut queue) = inbound().lock() {
        if let Err(QueueFullError) = enqueue_chunk(&mut queue, chunk) {
            log::warn!(
                "hosted-ble: inbound queue full at cap {} — dropping chunk; \
                 companion will retry after ack timeout",
                crate::hosted_ble_queue::MAX_INBOUND_QUEUE,
            );
        }
    }
}

unsafe extern "C" fn on_pairing_confirm_trampoline(
    data: *const u8,
    len: usize,
    _ctx: *mut c_void,
) {
    if data.is_null() || len != SECRET_LEN {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let mut secret = [0u8; SECRET_LEN];
    secret.copy_from_slice(bytes);
    if let Ok(mut queue) = pairing_inbound().lock() {
        if let Err(QueueFullError) = enqueue_pairing_secret(&mut queue, secret) {
            log::warn!(
                "hosted-ble: pairing-confirm queue full at cap {} — dropping secret",
                crate::hosted_ble_queue::MAX_INBOUND_QUEUE_PAIRING,
            );
        }
    }
}

unsafe extern "C" fn is_pairing_mode_trampoline(_ctx: *mut c_void) -> bool {
    pairing_mode()
}

unsafe extern "C" fn on_pairing_request_trampoline(_ctx: *mut c_void) {
    log::info!("hosted-ble: pairing-request flag set from BT host task");
    PAIRING_REQUEST_PENDING.store(true, Ordering::SeqCst);
}

unsafe extern "C" fn on_auth_cmpl_trampoline(
    success: bool,
    fail_reason: u8,
    peer_addr: *const u8,
    addr_type: u8,
    _ctx: *mut c_void,
) {
    if peer_addr.is_null() {
        log::warn!("hosted-ble: auth_cmpl trampoline received null peer_addr");
        return;
    }
    let mut bd = [0u8; 6];
    let slice = unsafe { std::slice::from_raw_parts(peer_addr, 6) };
    bd.copy_from_slice(slice);
    let event = AuthCmplEvent {
        success,
        fail_reason,
        peer_addr: bd,
        addr_type,
    };
    log::info!(
        "hosted-ble: auth_cmpl trampoline — success={success} reason=0x{fail_reason:02x} \
         bd={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} addr_type={addr_type}",
        bd[0], bd[1], bd[2], bd[3], bd[4], bd[5],
    );
    if let Ok(mut queue) = auth_cmpl_inbound().lock() {
        queue.push_back(event);
    }
}

impl RouteSyncIo for HostedBleRouteSyncIo {
    fn poll_chunk(&mut self) -> Result<Option<RouteTransferChunk>, RouteSyncIoError> {
        match self {
            Self::Active => {
                let mut queue = inbound().lock().map_err(|e| {
                    RouteSyncIoError::Transport(format!("inbound queue poisoned: {e}"))
                })?;
                Ok(queue.pop_front())
            }
            Self::Inactive => Ok(None),
        }
    }

    fn publish_messages(&mut self, messages: &[RouteSyncMessage]) -> Result<(), RouteSyncIoError> {
        if matches!(self, Self::Inactive) {
            return Ok(());
        }
        for message in messages {
            let bytes = encode_ble_packet(&BleRouteSyncPacket::SyncMessage(message.clone()));
            let err = unsafe {
                esp_idf_svc::sys::hosted_ble_route_sync_notify(bytes.as_ptr(), bytes.len())
            };
            // No connected companion → drop silently. The runtime calls
            // this every frame; bubbling an error here would put the
            // platform loop in a permanent failure state when the bike
            // is just out of range.
            if err == esp_idf_svc::sys::ESP_ERR_INVALID_STATE as i32 {
                continue;
            }
            if err != esp_idf_svc::sys::ESP_OK as i32 {
                return Err(RouteSyncIoError::Transport(format!(
                    "hosted_ble_route_sync_notify -> err={err:#x}"
                )));
            }
        }
        Ok(())
    }

    fn poll_pairing_request(&mut self) -> Result<bool, RouteSyncIoError> {
        if matches!(self, Self::Inactive) {
            return Ok(false);
        }
        Ok(drain_pairing_request())
    }

    fn poll_pairing_secret(&mut self) -> Result<Option<[u8; SECRET_LEN]>, RouteSyncIoError> {
        if matches!(self, Self::Inactive) {
            return Ok(None);
        }
        Ok(drain_pairing_secret())
    }

    fn poll_auth_cmpl(&mut self) -> Result<Option<AuthCmplOutcome>, RouteSyncIoError> {
        if matches!(self, Self::Inactive) {
            return Ok(None);
        }
        Ok(drain_auth_cmpl().map(|event| AuthCmplOutcome {
            success: event.success,
            fail_reason: event.fail_reason,
            peer_addr: event.peer_addr,
            addr_type: event.addr_type,
        }))
    }

    fn set_advertising_allowlist(
        &mut self,
        peer_addr: Option<([u8; 6], u8)>,
    ) -> Result<(), RouteSyncIoError> {
        if matches!(self, Self::Inactive) {
            return Ok(());
        }
        match peer_addr {
            Some((bd, addr_type)) => set_adv_filter_policy(true, &bd, addr_type),
            // Open the controller back up. Pass a zero address — the
            // C side ignores it when whitelist_only=false.
            None => set_adv_filter_policy(false, &[0u8; 6], 0),
        }
        Ok(())
    }
}
