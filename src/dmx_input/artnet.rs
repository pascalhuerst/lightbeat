//! Art-Net reception and announcement.
//!
//! LightBeat listens on UDP 6454 and, so that a desk can find it, answers
//! ArtPoll the way a node does: as one that *outputs* the universes of its
//! DMX inputs (to Art-Net a receiver is an output). It also announces itself
//! when the setup changes.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use artnet_protocol::{ArtCommand, PollReply};
use socket2::{Domain, Protocol, Socket, Type};

use super::{ARTNET_PORT, DmxInputConfig, DmxInputHub, split_address};

/// Universes one ArtPollReply can describe.
const PORTS_PER_REPLY: usize = 4;
/// PortTypes: bit 7 is "can output data from the Art-Net network", the rest DMX512.
const PORT_TYPE_OUTPUT_DMX: u8 = 0x80;
/// GoodOutput: bit 7 is "data is being transmitted".
const GOOD_OUTPUT_DATA: u8 = 0x80;
/// How long to wait before trying to open the socket again.
const RETRY: Duration = Duration::from_secs(2);
/// The manufacturer code range ESTA keeps for prototypes and experiments.
const ESTA_PROTOTYPE: u16 = 0x7FF0;

#[derive(Debug, PartialEq)]
pub enum Packet {
    Dmx { universe: u16, data: Vec<u8> },
    Poll,
    Other,
}

pub fn parse(bytes: &[u8]) -> Packet {
    match ArtCommand::from_buffer(bytes) {
        Ok(ArtCommand::Output(o)) => Packet::Dmx { universe: o.port_address.into(), data: o.data.as_ref().clone() },
        Ok(ArtCommand::Poll(_)) => Packet::Poll,
        _ => Packet::Other,
    }
}

/// A local IPv4 interface that Art-Net can be announced on.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalInterface {
    pub name: String,
    pub ip: Ipv4Addr,
    pub broadcast: Ipv4Addr,
    pub netmask: Ipv4Addr,
}

impl LocalInterface {
    fn same_subnet(&self, other: Ipv4Addr) -> bool {
        let mask = u32::from(self.netmask);
        u32::from(self.ip) & mask == u32::from(other) & mask
    }
}

pub fn local_interfaces() -> Vec<LocalInterface> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) if !v4.ip.is_loopback() => {
                let netmask = v4.netmask;
                let broadcast = v4.broadcast.unwrap_or_else(|| Ipv4Addr::from(u32::from(v4.ip) | !u32::from(netmask)));
                Some(LocalInterface { name: i.name, ip: v4.ip, broadcast, netmask })
            }
            _ => None,
        })
        .collect()
}

/// A zero-padded fixed-size text field.
fn text<const N: usize>(s: &str) -> [u8; N] {
    let mut out = [0u8; N];
    for (slot, b) in out.iter_mut().zip(s.bytes().filter(u8::is_ascii).take(N - 1)) {
        *slot = b;
    }
    out
}

/// The ArtPollReply packets that describe `inputs` as seen on `local`: one per
/// group of up to four universes sharing a net and sub-net, each with its own
/// bind index so they read as the ports of one device.
pub fn poll_replies(inputs: &[DmxInputConfig], local: Ipv4Addr, report_number: u32, live: &dyn Fn(u16) -> bool) -> Vec<PollReply> {
    let mut replies = Vec::new();
    let mut bind_index = 0u8;
    for input in inputs.iter().filter(|i| i.enabled) {
        let mut universes: Vec<u16> = input.universes().to_vec();
        universes.sort_unstable();
        universes.dedup();
        let mut groups: Vec<((u8, u8), Vec<u16>)> = Vec::new();
        for u in universes {
            let (net, sub, _) = split_address(u);
            match groups.iter_mut().find(|(key, _)| *key == (net, sub)) {
                Some((_, list)) => list.push(u),
                None => groups.push(((net, sub), vec![u])),
            }
        }
        for ((net, sub), list) in groups {
            for chunk in list.chunks(PORTS_PER_REPLY) {
                bind_index = bind_index.saturating_add(1);
                let mut reply = PollReply {
                    address: local,
                    version: [0, 1],
                    port_address: [net, sub],
                    oem: [0x00, 0xFF],
                    status_1: 0xD0,
                    esta_code: ESTA_PROTOTYPE,
                    short_name: text(&input.name),
                    long_name: text(&format!("LightBeat DMX input '{}'", input.name)),
                    node_report: text(&format!("#0001 [{report_number:04}] LightBeat receiving")),
                    num_ports: [0, chunk.len() as u8],
                    // 15-bit port addresses are supported.
                    status_2: 0x08,
                    bind_ip: local.octets(),
                    bind_index,
                    ..PollReply::default()
                };
                for (slot, universe) in chunk.iter().enumerate() {
                    reply.port_types[slot] = PORT_TYPE_OUTPUT_DMX;
                    reply.swout[slot] = (universe & 0x0F) as u8;
                    reply.good_output[slot] = if live(*universe) { GOOD_OUTPUT_DATA } else { 0 };
                }
                replies.push(reply);
            }
        }
    }
    replies
}

fn bind(port: u16) -> std::io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    // Other Art-Net software on this machine listens on the same port.
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    socket.bind(&SocketAddr::from(([0, 0, 0, 0], port)).into())?;
    socket.set_read_timeout(Some(Duration::from_millis(50)))?;
    Ok(socket.into())
}

/// Send the poll replies: broadcast on every interface the inputs are
/// announced on, and straight to whoever asked.
fn announce(hub: &DmxInputHub, socket: &UdpSocket, asked_by: Option<SocketAddrV4>) {
    let (inputs, number) = {
        let mut s = hub.lock();
        (s.inputs.clone(), s.next_report_number())
    };
    let live = |u: u16| hub.universe_live(u);
    let interfaces = local_interfaces();
    let send = |reply: PollReply, to: SocketAddrV4| {
        if let Ok(bytes) = ArtCommand::PollReply(Box::new(reply)).write_to_buffer() {
            let _ = socket.send_to(&bytes, to);
        }
    };

    for iface in &interfaces {
        // An interface chosen on another computer is not here: announce
        // everywhere rather than nowhere.
        let here: Vec<DmxInputConfig> = inputs
            .iter()
            .filter(|i| {
                i.interface().is_empty()
                    || i.interface() == iface.ip.to_string()
                    || !interfaces.iter().any(|x| x.ip.to_string() == i.interface())
            })
            .cloned()
            .collect();
        for reply in poll_replies(&here, iface.ip, number, &live) {
            send(reply, SocketAddrV4::new(iface.broadcast, ARTNET_PORT));
        }
    }

    if let Some(to) = asked_by {
        // The address to give is the one on the asker's network.
        let local = interfaces
            .iter()
            .find(|i| i.same_subnet(*to.ip()))
            .or(interfaces.first())
            .map_or(Ipv4Addr::LOCALHOST, |i| i.ip);
        for reply in poll_replies(&inputs, local, number, &live) {
            send(reply, to);
        }
    }
}

fn handle_packet(hub: &DmxInputHub, socket: &UdpSocket, bytes: &[u8], from: SocketAddrV4) {
    match parse(bytes) {
        Packet::Dmx { universe, data } => hub.apply_dmx(universe, &data, *from.ip()),
        Packet::Poll => announce(hub, socket, Some(from)),
        Packet::Other => {}
    }
}

/// The worker. The socket is only open while some enabled input listens to a
/// universe, so a project that does not use DMX input never touches the network.
pub fn run(hub: DmxInputHub, port: u16) {
    let mut socket: Option<UdpSocket> = None;
    let mut last_try: Option<Instant> = None;
    let mut last_second = Instant::now();
    let mut buf = [0u8; 2048];

    loop {
        if !hub.wants_socket() {
            if socket.take().is_some() {
                hub.set_listening(None, None);
            }
        } else if socket.is_none() && last_try.is_none_or(|t| t.elapsed() >= RETRY) {
            last_try = Some(Instant::now());
            match bind(port) {
                Ok(s) => {
                    let actual = s.local_addr().map_or(port, |a| a.port());
                    hub.set_listening(Some(actual), None);
                    hub.request_announce();
                    socket = Some(s);
                }
                Err(e) => hub.set_listening(None, Some(format!("cannot listen on UDP port {port}: {e}"))),
            }
        }

        match &socket {
            Some(s) => match s.recv_from(&mut buf) {
                Ok((n, SocketAddr::V4(from))) => handle_packet(&hub, s, &buf[..n], from),
                Ok(_) => {}
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(e) => {
                    hub.set_listening(None, Some(format!("receiving failed: {e}")));
                    socket = None;
                }
            },
            None => std::thread::sleep(Duration::from_millis(100)),
        }

        if last_second.elapsed() >= Duration::from_secs(1) {
            hub.tick_rates();
            last_second = Instant::now();
        }
        if let Some(s) = &socket
            && hub.take_announce()
        {
            announce(&hub, s, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmx_input::{DmxInputKind, join_address};
    use artnet_protocol::{Output, Poll};

    fn input(name: &str, universes: &[u16]) -> DmxInputConfig {
        let mut i = DmxInputConfig::new_art_net(1, name);
        i.kind = DmxInputKind::ArtNet { universes: universes.to_vec(), interface: String::new() };
        i
    }

    #[test]
    fn dmx_and_poll_packets_are_recognised_and_others_are_ignored() {
        let dmx = ArtCommand::Output(Output {
            port_address: 5u8.into(),
            data: vec![1, 2, 3, 4].into(),
            ..Output::default()
        });
        assert_eq!(parse(&dmx.write_to_buffer().unwrap()), Packet::Dmx { universe: 5, data: vec![1, 2, 3, 4] });
        assert_eq!(parse(&ArtCommand::Poll(Poll::default()).write_to_buffer().unwrap()), Packet::Poll);
        assert_eq!(parse(b"not art-net at all"), Packet::Other);
        assert_eq!(parse(&[]), Packet::Other);
    }

    #[test]
    fn an_input_is_announced_as_a_node_with_output_ports_for_its_universes() {
        let desk = input("Chamsys", &[0, 1, 2, 3, 4, join_address(1, 0, 5)]);
        let replies = poll_replies(&[desk], Ipv4Addr::new(2, 0, 0, 9), 7, &|u| u == 1);
        // Net 0 sub 0 holds five universes (a reply takes four), net 1 one more.
        assert_eq!(replies.len(), 3);
        assert_eq!(replies.iter().map(|r| r.bind_index).collect::<Vec<_>>(), vec![1, 2, 3]);
        let first = &replies[0];
        assert_eq!(first.address, Ipv4Addr::new(2, 0, 0, 9));
        assert_eq!(first.port, 6454);
        assert_eq!(first.num_ports, [0, 4]);
        assert_eq!(first.port_types, [0x80; 4], "to Art-Net a receiver is an output");
        assert_eq!(first.swout, [0, 1, 2, 3]);
        assert_eq!(first.good_output, [0, 0x80, 0, 0], "only universe 1 has data");
        assert_eq!(first.port_address, [0, 0]);
        assert_eq!(&first.short_name[..7], b"Chamsys");
        assert_eq!(first.short_name[7], 0);
        assert_eq!((replies[1].num_ports, replies[1].swout[0]), ([0, 1], 4));
        assert_eq!((replies[2].port_address, replies[2].swout[0]), ([1, 0], 5));
        assert!(String::from_utf8_lossy(&first.node_report).contains("[0007]"));
        // And it goes out as a valid packet.
        let bytes = ArtCommand::PollReply(Box::new(replies.into_iter().next().unwrap())).write_to_buffer().unwrap();
        assert!(bytes.starts_with(b"Art-Net\0"));
        assert_eq!(bytes.len(), 239, "an ArtPollReply is 239 bytes");
    }

    #[test]
    fn disabled_inputs_and_long_names_are_handled() {
        let mut off = input("Off", &[0]);
        off.enabled = false;
        assert!(poll_replies(&[off], Ipv4Addr::LOCALHOST, 1, &|_| false).is_empty());
        let long = input("A very long name that does not fit", &[0]);
        let r = &poll_replies(&[long], Ipv4Addr::LOCALHOST, 1, &|_| false)[0];
        assert_eq!(r.short_name[17], 0, "17 characters and the terminator");
        assert_eq!(&r.short_name[..17], b"A very long name ");
    }

    fn wait_for<T>(mut f: impl FnMut() -> Option<T>) -> Option<T> {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if let Some(v) = f() {
                return Some(v);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    #[test]
    fn a_real_socket_receives_dmx_and_answers_a_poll() {
        let hub = DmxInputHub::start_on(0);
        hub.set_inputs(&[input("Desk", &[2])]);
        let port = wait_for(|| hub.listening_port()).expect("the socket opened");

        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let target = SocketAddr::from(([127, 0, 0, 1], port));
        let frame = |data: Vec<u8>| {
            ArtCommand::Output(Output { port_address: 2u8.into(), data: data.into(), ..Output::default() })
                .write_to_buffer()
                .unwrap()
        };
        sender.send_to(&frame(vec![0, 0, 128, 255]), target).unwrap();
        let mut out = Vec::new();
        let got = wait_for(|| {
            hub.read_channels(&[(2, 0, 4)], &mut out);
            (out == vec![0, 0, 128, 255]).then_some(())
        });
        assert!(got.is_some(), "the frame arrived: {out:?}");
        assert!(hub.input_live(1));

        // A poll from the sender gets a reply from us, as a poll reply, to the sender.
        sender.send_to(&ArtCommand::Poll(Poll::default()).write_to_buffer().unwrap(), target).unwrap();
        let mut buf = [0u8; 1024];
        let (n, _) = sender.recv_from(&mut buf).expect("a reply");
        match ArtCommand::from_buffer(&buf[..n]).unwrap() {
            ArtCommand::PollReply(r) => {
                assert_eq!(&r.short_name[..4], b"Desk");
                assert_eq!(r.swout[0], 2);
            }
            other => panic!("expected a poll reply, got {other:?}"),
        }
    }
}
