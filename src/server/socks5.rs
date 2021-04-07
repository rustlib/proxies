use crate::connector::Connector;
use crate::error::ProxyError;
use crate::util::DuplexCopy;
use crate::{address::Address, util::BufIoExt};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

const SOCKVER: u8 = 0x05;

pub struct Socks5Handle {
    //
}

impl Socks5Handle {
    pub fn new() -> Self {
        Socks5Handle {}
    }

    pub async fn handle<T, C>(&self, connector: &C, mut io: BufReader<T>) -> Result<(), ProxyError>
    where
        T: AsyncRead + AsyncWrite + Unpin,
        C: Connector,
        <C as Connector>::Transport: Unpin,
    {
        Auth::auth(&mut io).await?;
        let request = ProxyRequest::parse(&mut io).await?;
        let mut remote = match connector.connect_tcp(&request.addr).await {
            Ok(x) => x,
            Err(e) => {
                let _ = io
                    .write_all(b"\x05\x01\x00\x01\x00\x00\x00\x00\x00\x00")
                    .await;
                return Err(connect_remote_fail!(request.addr, "{}", e));
            }
        };
        io.write_all(b"\x05\x00\x00\x01\x00\x00\x00\x00\x00\x00")
            .await?;
        let buffer = io.buffer();
        if !buffer.is_empty() {
            remote.write_all(buffer).await?;
        }
        let _ = DuplexCopy::with_pending(
            format!("local(to {})", request.addr),
            io.into_inner(),
            false,
            format!("remote({})", request.addr),
            remote,
            true,
        )
        .await?;
        Ok(())
    }
}

struct Auth;

impl Auth {
    async fn auth<T>(io: &mut T) -> Result<(), ProxyError>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let mut data: [u8; 2] = [0, 0];
        io.read_exact(&mut data).await?;
        if data[0] != SOCKVER {
            return Err(protocol_fail!("invalid socks version: {}", data[0]));
        }
        let mut methods = vec![0; data[1] as usize];
        io.read_exact(&mut methods).await?;
        io.write_all(b"\x05\x00").await?;
        Ok(())
    }
}

struct ProxyRequest {
    addr: Address,
}

impl ProxyRequest {
    async fn parse<T>(io: &mut T) -> Result<Self, ProxyError>
    where
        T: AsyncBufRead + Unpin,
    {
        let mut data: [u8; 4] = [0; 4];
        io.read_exact(&mut data).await?;
        if data[1] != 0x01 {
            return Err(protocol_fail!("unsupported cmd: {}", data[1]));
        }
        let addr = match data[3] {
            0x01 => {
                let mut ip: [u8; 4] = [0; 4];
                let mut port: [u8; 2] = [0, 0];
                io.read_exact(&mut ip).await?;
                io.read_exact(&mut port).await?;
                Address::Sock(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::from(ip)),
                    u16::from_be_bytes(port),
                ))
            }
            0x03 => match io.try_read_byte().await? {
                Some(size) => {
                    let mut addr = vec![0; (size as usize) + 2];
                    io.read_exact(&mut addr).await?;
                    let port = u16::from_be_bytes([addr[size as usize], addr[size as usize + 1]]);
                    addr.resize(size as usize, 0);
                    match String::from_utf8(addr) {
                        Ok(domain) => Address::Domain(domain, port),
                        Err(_) => {
                            return Err(invalid_data!("invalid domain"));
                        }
                    }
                }
                None => {
                    return Err(protocol_fail!("unexpected EOF"));
                }
            },
            0x04 => {
                let mut ip: [u8; 16] = [0; 16];
                let mut port: [u8; 2] = [0, 0];
                io.read_exact(&mut ip).await?;
                io.read_exact(&mut port).await?;
                Address::Sock(SocketAddr::new(
                    IpAddr::V6(Ipv6Addr::from(ip)),
                    u16::from_be_bytes(port),
                ))
            }
            other => {
                return Err(protocol_fail!("unknown ATYP type: {}", other));
            }
        };
        Ok(ProxyRequest { addr })
    }
}
