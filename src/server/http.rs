use std::net::SocketAddr;

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use url::{Host, Url};

use crate::address::Address;
use crate::util::{BufIoExt, DuplexCopy};
use crate::{connector::Connector, ProxyError};

pub struct HttpHandle {
    //
}

impl Default for HttpHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpHandle {
    pub fn new() -> Self {
        HttpHandle {}
    }

    pub async fn handle<T, C>(&self, connector: &C, mut io: BufReader<T>) -> Result<(), ProxyError>
    where
        T: AsyncRead + AsyncWrite + Unpin,
        C: Connector,
        <C as Connector>::Transport: Unpin,
    {
        let head_line = io
            .read_until_bytes(b"\r\n")
            .await
            .map_err(|e| io_fail!(e, "read http head line"))?;
        let request = Request::parse(&head_line)?;
        let mut remote = connector
            .connect_tcp(&request.addr)
            .await
            .map_err(|e| connect_remote_fail!(request.addr.clone(), "{}", e))?;

        if request.method == "CONNECT" {
            while io.read_until_bytes(b"\r\n").await?.len() > 2 {}
            io.write_all(b"HTTP/1.1 200 Ok\r\n\r\n").await?;
        } else {
            let line = request
                .build_http_line()
                .ok_or_else(|| format_err!("missing http url"))?;
            remote.write_all(line.as_bytes()).await?;
        }
        let buffer = io.buffer();
        if !buffer.is_empty() {
            remote.write_all(&Bytes::copy_from_slice(buffer)).await?;
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

struct Request<'a> {
    addr: Address,
    method: &'a str,
    url: Option<Url>,
    protocol: &'a str,
}

impl<'a> Request<'a> {
    fn parse(line: &'a Bytes) -> Result<Self, ProxyError> {
        let line =
            std::str::from_utf8(line).map_err(|_| format_err!("invalid HTTP data: {:?}", line))?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 3 {
            bail!("invalid http data: {:?}", line);
        }
        if parts[0] == "CONNECT" {
            let hp: Vec<&str> = parts[1].split(':').collect();
            if hp.len() != 2 {
                return Err(invalid_data!("invalid CONNECT address: {}", parts[1]));
            }
            let port: u16 = hp[1]
                .parse()
                .map_err(|_| format_err!("invalid CONNECT address: {}", parts[1]))?;
            return Ok(Request {
                addr: Address::Domain(hp[0].to_string(), port),
                method: parts[0],
                url: None,
                protocol: parts[2],
            });
        }
        let url = Url::parse(parts[1]).map_err(|_| format_err!("invalid url: {}", parts[1]))?;
        let port = match url.port_or_known_default() {
            Some(p) => p,
            None => {
                return Err(invalid_data!("invalid proxy url: {}", parts[1]));
            }
        };
        let addr = match url.host() {
            Some(Host::Domain(domain)) => Address::Domain(domain.to_string(), port),
            Some(Host::Ipv4(ip)) => Address::Sock(SocketAddr::new(ip.into(), port)),
            Some(Host::Ipv6(ip)) => Address::Sock(SocketAddr::new(ip.into(), port)),
            None => {
                return Err(invalid_data!("invalid proxy url: {}", parts[1]));
            }
        };
        Ok(Request {
            addr,
            method: parts[0],
            url: Some(url),
            protocol: parts[2],
        })
    }

    fn build_http_line(&self) -> Option<String> {
        self.url.as_ref().map(|url| {
            let mut new_line = String::with_capacity(64);
            new_line.push_str(self.method);
            new_line.push(' ');
            new_line.push_str(url.path());
            if let Some(query) = url.query() {
                new_line.push('?');
                new_line.push_str(query);
            }
            if let Some(fragment) = url.fragment() {
                new_line.push('#');
                new_line.push_str(fragment);
            }
            new_line.push(' ');
            new_line.push_str(self.protocol);
            new_line.push_str("\r\n");
            new_line
        })
    }
}
