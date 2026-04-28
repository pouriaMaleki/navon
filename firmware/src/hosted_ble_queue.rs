//! Bounded queue helpers for the inbound chunks the hosted-BLE C
//! callback drops on the runtime's doorstep.
//!
//! Lives in its own module so the cap is host-testable without a
//! Bluedroid bring-up: `hosted_ble.rs` itself is gated to `esp32p4`
//! builds, but the queue-management logic is pure Rust over standard
//! library types and runs on every target.

use std::collections::VecDeque;

use crate::route_sync::RouteTransferChunk;

/// Upper bound on the number of unconsumed `RouteTransferChunk`s the
/// inbound queue will hold before it starts dropping. Defends against a
/// peer pumping chunks faster than the runtime drains them (one per
/// frame). At ~256-byte chunks × 64 = ~16 KiB the worst-case memory
/// footprint is bounded; combined with `MAX_TOTAL_CHUNKS` and
/// `MAX_PAYLOAD_BYTES` over in `route_sync.rs` no single transfer can
/// exhaust internal RAM.
pub const MAX_INBOUND_QUEUE: usize = 64;

/// Returned when an `enqueue_chunk` call hits the cap. The caller is
/// expected to drop the *new* chunk and log a warning — keeping the
/// already-queued chunks intact lets the active transfer finish, and
/// the companion will retry the dropped chunk after its ack timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFullError;

/// Push a chunk onto the inbound queue, refusing once the cap is hit.
/// Pure Rust; no esp-idf dependencies.
pub fn enqueue_chunk(
    queue: &mut VecDeque<RouteTransferChunk>,
    chunk: RouteTransferChunk,
) -> Result<(), QueueFullError> {
    if queue.len() >= MAX_INBOUND_QUEUE {
        return Err(QueueFullError);
    }
    queue.push_back(chunk);
    Ok(())
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
}
