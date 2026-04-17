pub mod graph;
pub mod nodes;
pub mod patterns;
pub mod types;

use std::thread;

use ringbuf::traits::{Producer, Split};
use ringbuf::HeapRb;

use crate::dmx_io::{SharedDmxState, SharedObjectStore};
use self::graph::EngineGraph;
use self::types::EngineCommand;

// ---------------------------------------------------------------------------
// Ring buffer types for lock-free command passing
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub type CommandRingBuffer = HeapRb<EngineCommand>;
pub type CommandProducer = ringbuf::HeapProd<EngineCommand>;
pub type CommandConsumer = ringbuf::HeapCons<EngineCommand>;

/// Size of the UI→engine ring buffer. Needs to accommodate the biggest burst
/// the UI can produce in a single frame — chiefly `apply_project` on
/// undo/redo, which sends a `RemoveAllNodes` + `AddNode`/`AddConnection`/
/// `LoadData` triple per node (plus recursive subgraph inner commands). 8k
/// entries is well above anything realistic; the engine drains the whole ring
/// on each 1 ms tick, so this is not about steady-state throughput but about
/// surviving burst loads.
const COMMAND_BUFFER_SIZE: usize = 8192;

// ---------------------------------------------------------------------------
// Engine handle — owned by the app, sends commands to the engine thread
// ---------------------------------------------------------------------------

pub struct EngineHandle {
    pub command_tx: CommandProducer,
    _thread: thread::JoinHandle<()>,
}

impl EngineHandle {
    pub fn start(dmx_shared: SharedDmxState, object_store: SharedObjectStore) -> Self {
        let rb = HeapRb::new(COMMAND_BUFFER_SIZE);
        let (command_tx, command_rx) = rb.split();

        let handle = thread::spawn(move || {
            let graph = EngineGraph::new(dmx_shared, object_store);
            graph.run(command_rx);
        });

        Self {
            command_tx,
            _thread: handle,
        }
    }

    /// Send a command to the engine thread. Retries briefly if the ring is
    /// full — the engine drains the whole buffer every 1 ms, so giving up the
    /// thread for a couple of scheduler slices recovers from transient bursts
    /// without dropping state-altering commands. Only logs if it still can't
    /// push after the retry budget (indicates the engine is stuck).
    pub fn send(&mut self, cmd: EngineCommand) {
        const MAX_SPIN: u32 = 200;
        let mut pending = cmd;
        for attempt in 0..MAX_SPIN {
            match self.command_tx.try_push(pending) {
                Ok(()) => return,
                Err(c) => {
                    pending = c;
                    if attempt < 20 {
                        std::thread::yield_now();
                    } else {
                        // Short sleep past the first few yields so we don't
                        // busy-spin if the engine is genuinely behind.
                        std::thread::sleep(std::time::Duration::from_micros(100));
                    }
                }
            }
        }
        eprintln!("Engine command buffer full after retry, dropping command");
    }
}
