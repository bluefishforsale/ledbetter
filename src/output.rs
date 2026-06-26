//! Output transports. Art-Net and sACN are hand-rolled over UDP (the packets
//! are fixed-format and this avoids a dependency, consistent with M0). USB-DMX
//! and WLED are stubbed behind the same Transport enum — they need a serial
//! dongle / a WLED box to verify, so they do nothing until tested on hardware.

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};

/// A per-Controller send path (ADR-0001: transport is a Controller property).
pub enum Transport {
    ArtNet(ArtNet),
    Sacn(Sacn),
    /// ponytail: needs a real USB-DMX dongle + the rust_dmx dep to verify.
    /// Wire it up when hardware is on the bench; until then it is a no-op.
    UsbDmx,
    /// ponytail: needs a WLED box (HTTP/DDP) to verify. No-op until tested.
    Wled,
}

impl Transport {
    pub fn send(&mut self, universe: u16, dmx: &[u8]) -> std::io::Result<()> {
        match self {
            Transport::ArtNet(a) => a.send(universe, dmx),
            Transport::Sacn(s) => s.send(universe, dmx),
            Transport::UsbDmx | Transport::Wled => Ok(()),
        }
    }
}

// ---------------------------------------------------------------- Art-Net ----

pub struct ArtNet {
    sock: UdpSocket,
    target: String,
    seq: u8,
}

impl ArtNet {
    pub fn new(target: impl Into<String>) -> std::io::Result<Self> {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        Ok(ArtNet { sock, target: target.into(), seq: 1 })
    }

    pub fn send(&mut self, universe: u16, dmx: &[u8]) -> std::io::Result<()> {
        self.sock.send_to(&artdmx(universe, self.seq, dmx), &self.target)?;
        self.seq = self.seq.wrapping_add(1).max(1);
        Ok(())
    }
}

/// ArtDmx packet: 8-byte id + opcode 0x5000 + header + DMX data.
pub fn artdmx(universe: u16, seq: u8, data: &[u8]) -> Vec<u8> {
    let len = data.len();
    let mut p = Vec::with_capacity(18 + len);
    p.extend_from_slice(b"Art-Net\0");
    p.extend_from_slice(&[0x00, 0x50]);
    p.extend_from_slice(&[0x00, 0x0e]);
    p.push(seq);
    p.push(0);
    p.push((universe & 0xff) as u8);
    p.push(((universe >> 8) & 0x7f) as u8);
    p.extend_from_slice(&[(len >> 8) as u8, (len & 0xff) as u8]);
    p.extend_from_slice(data);
    p
}

// -------------------------------------------------------------------- sACN ----

pub struct Sacn {
    sock: UdpSocket,
    cid: [u8; 16],
    seq: u8,
    /// Test/unicast override; None routes to the per-universe multicast group.
    target: Option<SocketAddrV4>,
}

impl Sacn {
    pub fn new(cid: [u8; 16]) -> std::io::Result<Self> {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        Ok(Sacn { sock, cid, seq: 0, target: None })
    }

    // Used by the loopback test; also the hook for unicast sACN config later.
    #[allow(dead_code)]
    pub fn with_target(mut self, target: SocketAddrV4) -> Self {
        self.target = Some(target);
        self
    }

    pub fn send(&mut self, universe: u16, dmx: &[u8]) -> std::io::Result<()> {
        let pkt = e131(&self.cid, self.seq, universe, dmx);
        let addr = self.target.unwrap_or_else(|| sacn_multicast(universe));
        self.sock.send_to(&pkt, addr)?;
        self.seq = self.seq.wrapping_add(1);
        Ok(())
    }
}

/// E1.31 multicast group for a universe: 239.255.<hi>.<lo>:5568.
pub fn sacn_multicast(universe: u16) -> SocketAddrV4 {
    SocketAddrV4::new(Ipv4Addr::new(239, 255, (universe >> 8) as u8, (universe & 0xff) as u8), 5568)
}

/// Build an E1.31 (sACN) data packet. 638 bytes for 512 channels of DMX.
pub fn e131(cid: &[u8; 16], seq: u8, universe: u16, dmx: &[u8]) -> Vec<u8> {
    let prop_count = (dmx.len() + 1) as u16; // +1 start code
    // 125 header bytes before the property values (root 38 + framing 77 + DMP
    // header 10), then prop_count value bytes (start code + DMX).
    let total = 125 + prop_count as usize;
    let mut p = Vec::with_capacity(total);

    // --- Root layer ---
    p.extend_from_slice(&[0x00, 0x10]); // preamble size
    p.extend_from_slice(&[0x00, 0x00]); // postamble size
    p.extend_from_slice(b"ASC-E1.17\0\0\0"); // ACN packet identifier (12)
    push_flags_len(&mut p, total - 16); // root flags & length
    p.extend_from_slice(&[0x00, 0x00, 0x00, 0x04]); // VECTOR_ROOT_E131_DATA
    p.extend_from_slice(cid); // CID (16)

    // --- Framing layer ---
    push_flags_len(&mut p, total - 38);
    p.extend_from_slice(&[0x00, 0x00, 0x00, 0x02]); // VECTOR_E131_DATA_PACKET
    let mut name = [0u8; 64];
    name[..9].copy_from_slice(b"ledbetter");
    p.extend_from_slice(&name); // source name (64)
    p.push(100); // priority
    p.extend_from_slice(&[0x00, 0x00]); // sync address
    p.push(seq); // sequence number
    p.push(0x00); // options
    p.extend_from_slice(&universe.to_be_bytes()); // universe

    // --- DMP layer ---
    push_flags_len(&mut p, total - 115);
    p.push(0x02); // VECTOR_DMP_SET_PROPERTY
    p.push(0xa1); // address type & data type
    p.extend_from_slice(&[0x00, 0x00]); // first property address
    p.extend_from_slice(&[0x00, 0x01]); // address increment
    p.extend_from_slice(&prop_count.to_be_bytes()); // property value count
    p.push(0x00); // DMX start code
    p.extend_from_slice(dmx);
    p
}

/// PDU flags (0x7) + 12-bit length, big-endian.
fn push_flags_len(p: &mut Vec<u8>, len: usize) {
    let v = 0x7000u16 | (len as u16 & 0x0fff);
    p.extend_from_slice(&v.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artdmx_header_is_wellformed() {
        let p = artdmx(0x0103, 5, &[7u8; 512]);
        assert_eq!(&p[0..8], b"Art-Net\0");
        assert_eq!(&p[8..10], &[0x00, 0x50]);
        assert_eq!(p[14], 0x03);
        assert_eq!(p[15], 0x01);
        assert_eq!(&p[16..18], &[0x02, 0x00]);
    }

    #[test]
    fn e131_layout_and_length() {
        let cid = [0xAB; 16];
        let dmx = [9u8; 512];
        let p = e131(&cid, 3, 0x0102, &dmx);
        assert_eq!(p.len(), 638);
        assert_eq!(&p[0..2], &[0x00, 0x10]); // preamble
        assert_eq!(&p[4..16], b"ASC-E1.17\0\0\0");
        assert_eq!(&p[22..38], &cid); // CID after root flags+vector
        assert_eq!(&p[40..44], &[0x00, 0x00, 0x00, 0x02]); // framing vector
        assert_eq!(&p[44..53], b"ledbetter"); // source name
        assert_eq!(p[111], 3); // sequence
        assert_eq!(&p[113..115], &[0x01, 0x02]); // universe
        assert_eq!(p[125], 0x00); // DMX start code
        assert_eq!(&p[126..], &dmx); // 512 channels
        // flags+length on the root PDU
        assert_eq!(&p[16..18], &(0x7000u16 | (638 - 16)).to_be_bytes());
    }

    #[test]
    fn sacn_multicast_address() {
        assert_eq!(sacn_multicast(1), "239.255.0.1:5568".parse().unwrap());
        assert_eq!(sacn_multicast(0x0102), "239.255.1.2:5568".parse().unwrap());
    }

    #[test]
    fn e131_loopback() {
        let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        rx.set_read_timeout(Some(std::time::Duration::from_secs(1))).unwrap();
        let addr = match rx.local_addr().unwrap() {
            std::net::SocketAddr::V4(a) => a,
            _ => unreachable!(),
        };
        let mut s = Sacn::new([1u8; 16]).unwrap().with_target(addr);
        let mut dmx = [0u8; 512];
        dmx[0] = 255;
        s.send(7, &dmx).unwrap();

        let mut buf = [0u8; 700];
        let n = rx.recv(&mut buf).unwrap();
        assert_eq!(n, 638);
        assert_eq!(&buf[4..16], b"ASC-E1.17\0\0\0");
        assert_eq!(&buf[113..115], &[0x00, 0x07]); // universe 7
        assert_eq!(buf[126], 255); // first channel on the wire
    }
}
