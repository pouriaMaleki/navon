#[cfg(all(
    target_os = "espidf",
    not(any(esp32s2, esp32p4)),
    esp_idf_bt_enabled,
    esp_idf_bt_bluedroid_enabled
))]
use std::collections::VecDeque;
#[cfg(all(
    target_os = "espidf",
    not(any(esp32s2, esp32p4)),
    esp_idf_bt_enabled,
    esp_idf_bt_bluedroid_enabled
))]
use std::sync::{Arc, Mutex};

use runtime_core::api::RouteSyncMessage;

use crate::platform::{RouteSyncIo, RouteSyncIoError};
use crate::route_sync::RouteTransferChunk;
#[cfg(all(
    target_os = "espidf",
    not(any(esp32s2, esp32p4)),
    esp_idf_bt_enabled,
    esp_idf_bt_bluedroid_enabled
))]
use crate::route_sync_ble::{BleRouteSyncPacket, decode_ble_packet, encode_ble_packet};
use crate::route_sync_ble::{
    ROUTE_SYNC_CHUNK_WRITE_UUID, ROUTE_SYNC_EVENT_NOTIFY_UUID, ROUTE_SYNC_SERVICE_UUID,
};

#[cfg(all(
    target_os = "espidf",
    not(any(esp32s2, esp32p4)),
    esp_idf_bt_enabled,
    esp_idf_bt_bluedroid_enabled
))]
mod imp {
    use super::*;
    use enumset::EnumSet;
    use esp_idf_svc::bt::ble::gap::{AdvConfiguration, AppearanceCategory, EspBleGap};
    use esp_idf_svc::bt::ble::gatt::server::{EspGatts, GattsEvent};
    use esp_idf_svc::bt::ble::gatt::{
        AutoResponse, GattCharacteristic, GattId, GattServiceId, GattStatus, Permission, Property,
    };
    use esp_idf_svc::bt::{BleEnabled, BtDriver, BtUuid};
    use esp_idf_svc::sys::EspError;
    use std::borrow::Borrow;
    use std::marker::PhantomData;

    const APP_ID: u16 = 0x4553;
    const DEVICE_NAME: &str = "ESP32 Bike Minimap";
    const SERVICE_UUID128: u128 = 0x8d0f3f307b4d4f7c8b242f8e7e4e1001;
    const CHUNK_UUID128: u128 = 0x8d0f3f307b4d4f7c8b242f8e7e4e1002;
    const EVENT_UUID128: u128 = 0x8d0f3f307b4d4f7c8b242f8e7e4e1003;
    const MAX_PACKET_LEN: usize = 512;

    #[derive(Debug, Default)]
    struct SharedState {
        inbound_chunks: VecDeque<RouteTransferChunk>,
        pending_notifications: VecDeque<Vec<u8>>,
        gatt_if: Option<u8>,
        conn_id: Option<u16>,
        service_handle: Option<u16>,
        chunk_char_handle: Option<u16>,
        event_char_handle: Option<u16>,
    }

    pub struct EspIdfBleRouteSyncIo<'d, M, T>
    where
        T: Borrow<BtDriver<'d, M>> + Clone + Send + 'd,
        M: BleEnabled,
    {
        _gap: EspBleGap<'d, M, T>,
        _gatts: EspGatts<'d, M, T>,
        shared: Arc<Mutex<SharedState>>,
        _phantom: PhantomData<&'d ()>,
    }

    impl<'d, M, T> EspIdfBleRouteSyncIo<'d, M, T>
    where
        T: Borrow<BtDriver<'d, M>> + Clone + Send + 'd,
        M: BleEnabled,
    {
        pub fn new(driver: T) -> Result<Self, EspError> {
            let gap = EspBleGap::new(driver.clone())?;
            let gatts = EspGatts::new(driver)?;
            let shared = Arc::new(Mutex::new(SharedState::default()));
            gap.set_device_name(DEVICE_NAME)?;
            gap.set_adv_conf(&AdvConfiguration {
                include_name: true,
                appearance: AppearanceCategory::Cycling,
                service_uuid: Some(BtUuid::uuid128(SERVICE_UUID128)),
                ..Default::default()
            })?;

            unsafe {
                let gap_ref = &gap;
                let gatts_ref = &gatts;
                let shared_ref = shared.clone();
                gatts.subscribe_nonstatic(move |(gatt_if, event)| {
                    handle_gatts_event(gap_ref, gatts_ref, &shared_ref, gatt_if, event);
                })?;
            }
            gatts.register_app(APP_ID)?;

            Ok(Self {
                _gap: gap,
                _gatts: gatts,
                shared,
                _phantom: PhantomData,
            })
        }
    }

    impl<'d, M, T> RouteSyncIo for EspIdfBleRouteSyncIo<'d, M, T>
    where
        T: Borrow<BtDriver<'d, M>> + Clone + Send + 'd,
        M: BleEnabled,
    {
        fn poll_chunk(&mut self) -> Result<Option<RouteTransferChunk>, RouteSyncIoError> {
            Ok(self
                .shared
                .lock()
                .expect("ble shared state")
                .inbound_chunks
                .pop_front())
        }

        fn publish_messages(
            &mut self,
            messages: &[RouteSyncMessage],
        ) -> Result<(), RouteSyncIoError> {
            let mut shared = self.shared.lock().expect("ble shared state");
            for message in messages {
                shared.pending_notifications.push_back(encode_ble_packet(
                    &BleRouteSyncPacket::SyncMessage(message.clone()),
                ));
            }
            Ok(())
        }
    }

    fn handle_gatts_event<'d, M, T>(
        gap: &EspBleGap<'d, M, T>,
        gatts: &EspGatts<'d, M, T>,
        shared: &Arc<Mutex<SharedState>>,
        gatt_if: u8,
        event: GattsEvent<'_>,
    ) where
        T: Borrow<BtDriver<'d, M>> + Clone + Send + 'd,
        M: BleEnabled,
    {
        match event {
            GattsEvent::ServiceRegistered { status, .. } if status == GattStatus::Ok => {
                let service_id = GattServiceId {
                    id: GattId {
                        uuid: BtUuid::uuid128(SERVICE_UUID128),
                        inst_id: 0,
                    },
                    is_primary: true,
                };
                let _ = gatts.create_service(gatt_if, &service_id, 8);
                shared.lock().expect("ble shared state").gatt_if = Some(gatt_if);
            }
            GattsEvent::ServiceCreated {
                status,
                service_handle,
                ..
            } if status == GattStatus::Ok => {
                let chunk_characteristic = GattCharacteristic::new(
                    BtUuid::uuid128(CHUNK_UUID128),
                    EnumSet::only(Permission::Write),
                    EnumSet::from_iter([Property::Write, Property::WriteNoResponse]),
                    MAX_PACKET_LEN,
                    AutoResponse::ByApp,
                );
                let event_characteristic = GattCharacteristic::new(
                    BtUuid::uuid128(EVENT_UUID128),
                    EnumSet::only(Permission::Read),
                    EnumSet::only(Property::Notify),
                    MAX_PACKET_LEN,
                    AutoResponse::ByGatt,
                );
                let _ = gatts.add_characteristic(service_handle, &chunk_characteristic, &[]);
                let _ = gatts.add_characteristic(service_handle, &event_characteristic, &[]);
                shared.lock().expect("ble shared state").service_handle = Some(service_handle);
            }
            GattsEvent::CharacteristicAdded {
                status,
                attr_handle,
                char_uuid,
                ..
            } if status == GattStatus::Ok => {
                let mut shared = shared.lock().expect("ble shared state");
                if char_uuid == BtUuid::uuid128(CHUNK_UUID128) {
                    shared.chunk_char_handle = Some(attr_handle);
                }
                if char_uuid == BtUuid::uuid128(EVENT_UUID128) {
                    shared.event_char_handle = Some(attr_handle);
                }
                if let (Some(service_handle), Some(_), Some(_)) = (
                    shared.service_handle,
                    shared.chunk_char_handle,
                    shared.event_char_handle,
                ) {
                    let _ = gatts.start_service(service_handle);
                    let _ = gap.start_advertising();
                }
            }
            GattsEvent::PeerConnected { conn_id, .. } => {
                let mut shared = shared.lock().expect("ble shared state");
                shared.conn_id = Some(conn_id);
            }
            GattsEvent::PeerDisconnected { .. } => {
                let mut shared = shared.lock().expect("ble shared state");
                shared.conn_id = None;
                let _ = gap.start_advertising();
            }
            GattsEvent::Write {
                conn_id,
                trans_id,
                handle,
                need_rsp,
                value,
                ..
            } => {
                let maybe_chunk = {
                    let shared = shared.lock().expect("ble shared state");
                    if shared.chunk_char_handle == Some(handle) {
                        decode_ble_packet(value)
                            .ok()
                            .and_then(|packet| match packet {
                                BleRouteSyncPacket::Chunk(chunk) => Some(chunk),
                                BleRouteSyncPacket::SyncMessage(_) => None,
                            })
                    } else {
                        None
                    }
                };
                if let Some(chunk) = maybe_chunk {
                    shared
                        .lock()
                        .expect("ble shared state")
                        .inbound_chunks
                        .push_back(chunk);
                }
                if need_rsp {
                    let _ = gatts.send_response(gatt_if, conn_id, trans_id, GattStatus::Ok, None);
                }
            }
            GattsEvent::Congest {
                congested: false, ..
            }
            | GattsEvent::ResponseComplete {
                status: GattStatus::Ok,
                ..
            }
            | GattsEvent::ServiceStarted {
                status: GattStatus::Ok,
                ..
            } => {
                drain_notifications(gatts, shared, gatt_if);
            }
            _ => {}
        }
    }

    fn drain_notifications<'d, M, T>(
        gatts: &EspGatts<'d, M, T>,
        shared: &Arc<Mutex<SharedState>>,
        gatt_if: u8,
    ) where
        T: Borrow<BtDriver<'d, M>> + Clone + Send + 'd,
        M: BleEnabled,
    {
        let (conn_id, event_handle, packets) = {
            let mut shared = shared.lock().expect("ble shared state");
            let conn_id = shared.conn_id;
            let event_handle = shared.event_char_handle;
            let packets = shared.pending_notifications.drain(..).collect::<Vec<_>>();
            (conn_id, event_handle, packets)
        };

        if let (Some(conn_id), Some(event_handle)) = (conn_id, event_handle) {
            for packet in packets {
                let _ = gatts.notify(gatt_if, conn_id, event_handle, &packet);
            }
        }
    }
}

#[cfg(all(
    target_os = "espidf",
    not(any(esp32s2, esp32p4)),
    esp_idf_bt_enabled,
    esp_idf_bt_bluedroid_enabled
))]
pub use imp::EspIdfBleRouteSyncIo;

#[cfg(not(all(
    target_os = "espidf",
    not(any(esp32s2, esp32p4)),
    esp_idf_bt_enabled,
    esp_idf_bt_bluedroid_enabled
)))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EspIdfBleRouteSyncIo;

#[cfg(not(all(
    target_os = "espidf",
    not(any(esp32s2, esp32p4)),
    esp_idf_bt_enabled,
    esp_idf_bt_bluedroid_enabled
)))]
impl RouteSyncIo for EspIdfBleRouteSyncIo {
    fn poll_chunk(&mut self) -> Result<Option<RouteTransferChunk>, RouteSyncIoError> {
        Ok(None)
    }

    fn publish_messages(&mut self, _messages: &[RouteSyncMessage]) -> Result<(), RouteSyncIoError> {
        Ok(())
    }
}

pub fn gatt_service_summary() -> (&'static str, &'static str, &'static str) {
    (
        ROUTE_SYNC_SERVICE_UUID,
        ROUTE_SYNC_CHUNK_WRITE_UUID,
        ROUTE_SYNC_EVENT_NOTIFY_UUID,
    )
}
