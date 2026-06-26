//! Art-Net output. Hand-rolled ArtDmx over UDP (the packet is trivial and this
//! sidesteps a dependency). Per-Controller transports (ADR-0001) arrive at M4;
//! M1 keeps the single Art-Net sender.

use std::net::UdpSocket;

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
        let pkt = artdmx(universe, self.seq, dmx);
        self.sock.send_to(&pkt, &self.target)?;
        self.seq = self.seq.wrapping_add(1).max(1);
        Ok(())
    }
}

/// Build an ArtDmx packet: 8-byte id + opcode 0x5000 + header + DMX data.
pub fn artdmx(universe: u16, seq: u8, data: &[u8]) -> Vec<u8> {
    let len = data.len();
    let mut p = Vec::with_capacity(18 + len);
    p.extend_from_slice(b"Art-Net\0");
    p.extend_from_slice(&[0x00, 0x50]); // ArtDmx opcode 0x5000 (little-endian)
    p.extend_from_slice(&[0x00, 0x0e]); // protocol version 14 (big-endian)
    p.push(seq);
    p.push(0); // physical
    p.push((universe & 0xff) as u8); // SubUni
    p.push(((universe >> 8) & 0x7f) as u8); // Net
    p.extend_from_slice(&[(len >> 8) as u8, (len & 0xff) as u8]); // length (big-endian)
    p.extend_from_slice(data);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_wellformed() {
        let data = [7u8; 512];
        let p = artdmx(0x0103, 5, &data);
        assert_eq!(&p[0..8], b"Art-Net\0");
        assert_eq!(&p[8..10], &[0x00, 0x50]);
        assert_eq!(&p[10..12], &[0x00, 0x0e]);
        assert_eq!(p[12], 5);
        assert_eq!(p[14], 0x03);
        assert_eq!(p[15], 0x01);
        assert_eq!(&p[16..18], &[0x02, 0x00]);
        assert_eq!(&p[18..], &data);
    }
}
