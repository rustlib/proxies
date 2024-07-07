extern crate proxies;

use proxies::connector::DirectConnector;
use proxies::server::ProxyServer;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt::init();
    let server = ProxyServer::bind(DirectConnector, "127.0.0.1:9000")
        .await
        .expect("bind fail");
    server.run().await.expect("run server fail");
}
