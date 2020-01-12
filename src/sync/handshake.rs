use std::{convert, io, io::Read, io::Write};

// use log::debug;
use sodiumoxide::crypto::{auth, sign::ed25519};

use crate::handshake::{self, Handshake, HandshakeComplete};
use super::error::{Error,Result};

impl convert::From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl convert::From<handshake::Error> for Error {
    fn from(error: handshake::Error) -> Self {
        Self::Handshake(error)
    }
}

pub fn handshake_client<T: Read + Write>(
    mut stream: T,
    net_id: auth::Key,
    pk: ed25519::PublicKey,
    sk: ed25519::SecretKey,
    server_pk: ed25519::PublicKey,
) -> Result<HandshakeComplete> {
    let mut buf = [0; 128];
    let handshake = Handshake::new_client(net_id, pk, sk);

    let mut send_buf = &mut buf[..handshake.send_bytes()];
    let handshake = handshake.send_client_hello(&mut send_buf);
    stream.write_all(&send_buf)?;

    let mut recv_buf = &mut buf[..handshake.recv_bytes()];
    stream.read_exact(&mut recv_buf)?;
    let handshake = handshake.recv_server_hello(&recv_buf)?;

    let mut send_buf = &mut buf[..handshake.send_bytes()];
    let handshake = handshake.send_client_auth(&mut send_buf, server_pk)?;
    stream.write_all(&send_buf)?;

    let mut recv_buf = &mut buf[..handshake.recv_bytes()];
    stream.read_exact(&mut recv_buf)?;
    let handshake = handshake.recv_server_accept(&mut recv_buf)?;

    Ok(handshake.complete())
}

pub fn handshake_server<T: Read + Write>(
    mut stream: T,
    net_id: auth::Key,
    pk: ed25519::PublicKey,
    sk: ed25519::SecretKey,
) -> Result<HandshakeComplete> {
    let mut buf = [0; 128];
    let handshake = Handshake::new_server(net_id, pk, sk);

    let mut recv_buf = &mut buf[..handshake.recv_bytes()];
    stream.read_exact(&mut recv_buf)?;
    let handshake = handshake.recv_client_hello(&recv_buf)?;

    let mut send_buf = &mut buf[..handshake.send_bytes()];
    let handshake = handshake.send_server_hello(&mut send_buf);
    stream.write_all(&send_buf)?;

    let mut recv_buf = &mut buf[..handshake.recv_bytes()];
    stream.read_exact(&mut recv_buf)?;
    let handshake = handshake.recv_client_auth(&mut recv_buf)?;

    let mut send_buf = &mut buf[..handshake.send_bytes()];
    let handshake = handshake.send_server_accept(&mut send_buf);
    stream.write_all(&send_buf)?;

    Ok(handshake.complete())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::{Read, Write};

    use test_utils::net_sync::{net, net_fragment};

    use crossbeam::thread;

    const NET_ID_HEX: &str = "d4a1cb88a66f02f8db635ce26441cc5dac1b08420ceaac230839b755845a9ffb";
    const CLIENT_SEED_HEX: &str =
        "0000000000000000000000000000000000000000000000000000000000000000";
    const SERVER_SEED_HEX: &str =
        "0000000000000000000000000000000000000000000000000000000000000001";

    // Perform a handshake between two connected streams
    fn handshake_aux<T: Write + Read + Send>(stream_client: T, stream_server: T) {
        let net_id = auth::Key::from_slice(&hex::decode(NET_ID_HEX).unwrap()).unwrap();
        let (client_pk, client_sk) = ed25519::keypair_from_seed(
            &ed25519::Seed::from_slice(&hex::decode(CLIENT_SEED_HEX).unwrap()).unwrap(),
        );
        let (server_pk, server_sk) = ed25519::keypair_from_seed(
            &ed25519::Seed::from_slice(&hex::decode(SERVER_SEED_HEX).unwrap()).unwrap(),
        );

        let (client_handshake, server_handshake) = thread::scope(|s| {
            let net_id_cpy = net_id.clone();

            let handle_client = s.spawn(move |_| {
                handshake_client(stream_client, net_id, client_pk, client_sk, server_pk).unwrap()
            });

            let handle_server = s.spawn(move |_| {
                handshake_server(stream_server, net_id_cpy, server_pk, server_sk).unwrap()
            });

            (handle_client.join().unwrap(), handle_server.join().unwrap())
        })
        .unwrap();

        assert_eq!(client_handshake.net_id, server_handshake.net_id);
        assert_eq!(
            client_handshake.shared_secret,
            server_handshake.shared_secret
        );
        assert_eq!(client_handshake.pk, server_handshake.peer_pk);
        assert_eq!(
            client_handshake.ephemeral_pk,
            server_handshake.peer_ephemeral_pk
        );
        assert_eq!(client_handshake.peer_pk, server_handshake.pk);
        assert_eq!(
            client_handshake.peer_ephemeral_pk,
            server_handshake.ephemeral_pk
        );
    }

    #[test]
    fn test_handshake_sync() {
        net(|a, _, b, _| handshake_aux(a, b));
    }

    #[test]
    fn test_handshake_sync_fragment() {
        net_fragment(5, |a, _, b, _| handshake_aux(a, b));
    }
}
