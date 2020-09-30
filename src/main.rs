use chat_mio::Server;
use mio::net::TcpListener;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let addr = args
        .get(1)
        .unwrap_or(&"127.0.0.1:80".to_string())
        .parse()
        .unwrap();

    let listener = TcpListener::bind(&addr).unwrap();
    let mut server = Server::new(listener).unwrap();

    println!("Running chat server on {}. Press ctrl-c to exit...", addr);
    loop {
        match server.poll() {
            Ok(_) => {}
            Err(e) => println!("Error handling http request {:?}", e),
        }
    }
}
