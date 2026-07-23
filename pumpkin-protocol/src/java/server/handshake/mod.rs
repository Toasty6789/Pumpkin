use std::io::Read;

use crate::{
    ClientPacket, ConnectionState, ReadingError, ServerPacket, VarInt, ser::NetworkReadExt,
    ser::NetworkWriteExt,
};
use pumpkin_data::packet::serverbound::HANDSHAKE_INTENTION;
use pumpkin_macros::java_packet;
use pumpkin_util::version::JavaMinecraftVersion;

/// The very first packet sent by the client to initiate a connection
///
/// It determines whether the client wants to check the server status (SLP)
/// or actually login to play.
#[java_packet(HANDSHAKE_INTENTION)]
pub struct SHandShake {
    /// The protocol version of the client (e.g., 767 for 1.21).
    pub protocol_version: VarInt,
    /// The hostname or IP used by the client to connect
    pub server_address: Box<str>,
    /// The port number used by the client to connect
    pub server_port: u16,
    /// The state the client wants to transition to (1 for Status, 2 for Login)
    pub next_state: ConnectionState,
}

impl ServerPacket for SHandShake {
    fn read(mut read: impl Read, _version: &JavaMinecraftVersion) -> Result<Self, ReadingError> {
        Ok(Self {
            protocol_version: read.get_var_int()?,
            // Use unbounded get_str() to accept proxy-forwarded addresses
            // (e.g. BungeeCord appends `\0ip\0uuid\0properties` > 255 chars).
            server_address: read.get_str()?,
            server_port: read.get_u16_be()?,
            next_state: read
                .get_var_int()?
                .try_into()
                .map_err(|_| ReadingError::Message("Invalid status".to_string()))?,
        })
    }
}

impl ClientPacket for SHandShake {
    fn write_packet_data(
        &self,
        mut write: impl std::io::Write,
        _version: &JavaMinecraftVersion,
    ) -> Result<(), crate::ser::WritingError> {
        write.write_var_int(&self.protocol_version)?;
        write.write_string(&self.server_address)?;
        write.write_u16_be(self.server_port)?;
        write.write_var_int(&VarInt(self.next_state as i32))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::java::server::handshake::SHandShake;
    use crate::ser::{NetworkReadExt, NetworkWriteExt};
    use crate::{ClientPacket, ConnectionState, JavaMinecraftVersion, ServerPacket, VarInt};
    use std::io::Cursor;

    /// Regression test for #1293: the handshake `server_address` field must accept
    /// strings longer than 255 characters so that `BungeeCord` proxy forwarding
    /// data (`host\0ip\0uuid\0properties`) does not cause a decode error.
    #[test]
    fn handshake_accepts_long_server_address() {
        let version = JavaMinecraftVersion::from_protocol(767);

        // Build a handshake packet with a 300-character server_address
        // (longer than the old 255-byte bound).
        let long_host = "a".repeat(300);
        let packet = SHandShake {
            protocol_version: VarInt(767),
            server_address: long_host.clone().into_boxed_str(),
            server_port: 25565,
            next_state: ConnectionState::Login,
        };

        // Serialize: prepend VarInt(0x00) packet ID, then write the packet data
        let mut buf = Vec::new();
        buf.write_var_int(&VarInt(0)).expect("write packet id");
        packet
            .write_packet_data(&mut buf, &version)
            .expect("write packet");

        let mut cursor = Cursor::new(&buf);
        // Skip the VarInt packet ID (0x00)
        cursor.get_var_int().expect("read packet id");

        let decoded = SHandShake::read(&mut cursor, &version).expect("read handshake");
        assert_eq!(decoded.server_address.as_ref(), long_host.as_str());
        assert_eq!(decoded.protocol_version.0, 767);
        assert_eq!(decoded.server_port, 25565);
    }
}
