//! Bounded queue helpers for the inbound chunks the hosted-BLE C
//! callback drops on the runtime's doorstep.
//!
//! Lives in its own module so the cap is host-testable without a
//! Bluedroid bring-up: `hosted_ble.rs` itself is gated to `esp32p4`
//! builds, but the queue-management logic is pure Rust over standard
//! library types and runs on every target.

use std::collections::VecDeque;

use crate::pairing::SECRET_LEN;
use crate::route_sync::RouteTransferChunk;

/// Upper bound on the number of unconsumed `RouteTransferChunk`s the
/// inbound chunk queue will hold before it starts dropping. Defends
/// against a peer pumping chunks faster than the runtime drains them
/// (one per frame). At ~256-byte chunks × 64 = ~16 KiB the worst-case
/// memory footprint is bounded; combined with `MAX_TOTAL_CHUNKS` and
/// `MAX_PAYLOAD_BYTES` over in `route_sync.rs` no single transfer can
/// exhaust internal RAM.
pub const MAX_INBOUND_QUEUE: usize = 64;

/// Upper bound on the number of unconsumed pairing secrets the inbound
/// pairing-confirm queue will hold. Pairing-confirm writes are rare
/// (one per pairing handshake) and tiny (32 bytes), so the cap is much
/// lower than the chunk queue's. The rule is the same: drop the *new*
/// item on overflow so the in-flight handshake can finish.
pub const MAX_INBOUND_QUEUE_PAIRING: usize = 8;

/// Returned when an `enqueue_*` call hits the cap. The caller is
/// expected to drop the *new* item and log a warning — keeping the
/// already-queued items intact lets the active transfer/handshake
/// finish, and the companion will retry after its ack timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFullError;

/// Push an item onto a bounded inbound queue, refusing once the cap is
/// hit. Pure Rust; no esp-idf dependencies. Generic so the same rule
/// covers chunks (cap 64) and pairing-confirm secrets (cap 8) without
/// duplicating the logic.
pub fn enqueue_bounded<T>(
    queue: &mut VecDeque<T>,
    item: T,
    cap: usize,
) -> Result<(), QueueFullError> {
    if queue.len() >= cap {
        return Err(QueueFullError);
    }
    queue.push_back(item);
    Ok(())
}

/// Convenience wrapper around `enqueue_bounded` for the chunk queue,
/// hardcoding `MAX_INBOUND_QUEUE`.
pub fn enqueue_chunk(
    queue: &mut VecDeque<RouteTransferChunk>,
    chunk: RouteTransferChunk,
) -> Result<(), QueueFullError> {
    enqueue_bounded(queue, chunk, MAX_INBOUND_QUEUE)
}

/// Convenience wrapper around `enqueue_bounded` for the pairing-confirm
/// queue, hardcoding `MAX_INBOUND_QUEUE_PAIRING`.
pub fn enqueue_pairing_secret(
    queue: &mut VecDeque<[u8; SECRET_LEN]>,
    secret: [u8; SECRET_LEN],
) -> Result<(), QueueFullError> {
    enqueue_bounded(queue, secret, MAX_INBOUND_QUEUE_PAIRING)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_chunk(index: u32) -> RouteTransferChunk {
        RouteTransferChunk {
            transfer_id: format!("t-{index}"),
            chunk_index: index,
            total_chunks: 1024,
            checksum_hex: "deadbeef".to_owned(),
            payload_fragment: vec![0xEE_u8; 4],
        }
    }

    #[test]
    fn enqueue_chunk_succeeds_until_capacity() {
        let mut queue = VecDeque::new();
        for index in 0..MAX_INBOUND_QUEUE as u32 {
            assert_eq!(
                enqueue_chunk(&mut queue, tiny_chunk(index)),
                Ok(()),
                "chunk {index} should fit while under cap"
            );
        }
        assert_eq!(queue.len(), MAX_INBOUND_QUEUE);
    }

    #[test]
    fn enqueue_chunk_rejects_at_capacity_with_queue_full_error() {
        let mut queue = VecDeque::new();
        for index in 0..MAX_INBOUND_QUEUE as u32 {
            enqueue_chunk(&mut queue, tiny_chunk(index)).expect("under cap");
        }
        let attempt = enqueue_chunk(&mut queue, tiny_chunk(999));
        assert_eq!(
            attempt,
            Err(QueueFullError),
            "the (cap+1)th chunk must be rejected"
        );
        // Rejection happens before the push, so existing entries stay
        // intact and the queue length doesn't grow past the cap.
        assert_eq!(queue.len(), MAX_INBOUND_QUEUE);
        let first = queue.front().expect("queue not empty");
        assert_eq!(
            first.chunk_index, 0,
            "head chunk must still be the first one we enqueued — \
             the rejection must not silently drop earlier chunks",
        );
    }

    #[test]
    fn enqueue_chunk_recovers_after_pop() {
        let mut queue = VecDeque::new();
        for index in 0..MAX_INBOUND_QUEUE as u32 {
            enqueue_chunk(&mut queue, tiny_chunk(index)).expect("fill");
        }
        queue.pop_front().expect("non-empty");
        // After a pop there's room again.
        let attempt = enqueue_chunk(&mut queue, tiny_chunk(MAX_INBOUND_QUEUE as u32));
        assert_eq!(attempt, Ok(()));
        assert_eq!(queue.len(), MAX_INBOUND_QUEUE);
    }

    #[test]
    fn enqueue_pairing_secret_succeeds_until_capacity() {
        let mut queue: VecDeque<[u8; SECRET_LEN]> = VecDeque::new();
        for i in 0..MAX_INBOUND_QUEUE_PAIRING as u8 {
            assert_eq!(
                enqueue_pairing_secret(&mut queue, [i; SECRET_LEN]),
                Ok(()),
                "pairing secret {i} should fit under cap"
            );
        }
        assert_eq!(queue.len(), MAX_INBOUND_QUEUE_PAIRING);
    }

    #[test]
    fn enqueue_pairing_secret_rejects_at_capacity() {
        let mut queue: VecDeque<[u8; SECRET_LEN]> = VecDeque::new();
        for i in 0..MAX_INBOUND_QUEUE_PAIRING as u8 {
            enqueue_pairing_secret(&mut queue, [i; SECRET_LEN]).expect("under cap");
        }
        let attempt = enqueue_pairing_secret(&mut queue, [0xFF; SECRET_LEN]);
        assert_eq!(attempt, Err(QueueFullError));
        assert_eq!(queue.len(), MAX_INBOUND_QUEUE_PAIRING);
        // First-in entries are still intact at the head — the cap drops
        // the *new* secret so an in-flight handshake can finish.
        assert_eq!(queue.front().copied().unwrap(), [0; SECRET_LEN]);
    }
}
