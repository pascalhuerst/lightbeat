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

const COMMAND_BUFFER_SIZE: usize = 256;

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

    /// Send a command to the engine thread (non-blocking).
    pub fn send(&mut self, cmd: EngineCommand) {
        if self.command_tx.try_push(cmd).is_err() {
            eprintln!("Engine command buffer full, dropping command");
        }
    }
}
