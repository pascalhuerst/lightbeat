use std::convert::TryInto;
use std::net::{ToSocketAddrs, UdpSocket};

use artnet_protocol::{ArtCommand, Output};

use crate::objects::DmxUniverse;
use super::DmxOutput;

/// Art-Net DMX output. Sends DMX universes over UDP using the Art-Net protocol.
pub struct ArtNetOutput {
    socket: UdpSocket,
    target: std::net::SocketAddr,
}

impl ArtNetOutput {
    /// Create a new Art-Net output targeting the given host and port.
    ///
    /// `host`: IP address or hostname of the Art-Net node (use "255.255.255.255" for broadcast).
    /// `port`: Art-Net port (standard: 6454).
    pub fn new(host: &str, port: u16) -> Result<Self, String> {
        let target = format!("{}:{}", host, port)
            .to_socket_addrs()
            .map_err(|e| format!("Invalid Art-Net target address: {}", e))?
            .next()
            .ok_or_else(|| "Could not resolve Art-Net target address".to_string())?;

        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| format!("Failed to bind UDP socket: {}", e))?;

        // Enable broadcast if targeting broadcast address.
        if host == "255.255.255.255" || host.ends_with(".255") {
            socket
                .set_broadcast(true)
                .map_err(|e| format!("Failed to enable broadcast: {}", e))?;
        }

        Ok(Self { socket, target })
    }
}

impl DmxOutput for ArtNetOutput {
    fn send_universe(&mut self, universe: &DmxUniverse) -> Result<(), String> {
        // Art-Net universe addressing: port_address = (net << 8) | (subnet << 4) | universe
        let port_address = ((universe.net as u16) << 8)
            | ((universe.subnet as u16) << 4)
            | (universe.universe as u16);

        let command = ArtCommand::Output(Output {
            port_address: port_address
                .try_into()
                .map_err(|e| format!("Invalid port address: {}", e))?,
            data: universe.channels.as_ref().try_into()
                .map_err(|e| format!("Invalid DMX data: {}", e))?,
            ..Output::default()
        });

        let buf = command
            .write_to_buffer()
            .map_err(|e| format!("Failed to serialize Art-Net packet: {}", e))?;

        self.socket
            .send_to(&buf, self.target)
            .map_err(|e| format!("Failed to send Art-Net packet: {}", e))?;

        Ok(())
    }

    fn name(&self) -> &str {
        "Art-Net"
    }
}
