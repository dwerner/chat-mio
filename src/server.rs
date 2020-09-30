use std::collections::HashMap;
use std::error::Error;
use std::io::{Read, Write};

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Poll, PollOpt, Ready, Token};

use super::chat_service::ChatService;
use super::messages::Message;
use super::router::{error500, not_found, ok_json, status_code_msg, status_ok, Router};

const MAX_BUF_SIZE: usize = 8192;

pub struct Client<T>
where
    T: Read + Write,
{
    socket: T,
    buffer: [u8; MAX_BUF_SIZE],
}

impl<T> Client<T>
where
    T: Read + Write,
{
    pub fn new(socket: T) -> Self {
        Client {
            socket,
            buffer: [0; MAX_BUF_SIZE],
        }
    }

    pub fn read(&mut self) -> std::io::Result<usize> {
        self.socket.read(&mut self.buffer)
    }
}

pub struct Server {
    token: mio::Token,
    next_token: u64,
    listener: TcpListener,
    events: Events,
    connections: HashMap<mio::Token, Client<TcpStream>>,
    poll: Poll,
    router: Router,
}

fn response_to_string(res: http::Response<String>) -> String {
    let body = res.body();
    let headers = res.headers();
    let headers = headers
        .iter()
        .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap()))
        .collect::<Vec<_>>()
        .join("\r\n");

    format!(
        "HTTP/1.1 {}\r\n{}\r\nContent-Length: {}\r\n\r\n{}",
        res.status(),
        headers,
        body.len(),
        body
    )
}

impl Server {
    pub fn new(listener: TcpListener) -> Result<Self, Box<dyn Error>> {
        let events = Events::with_capacity(64);
        let connections = HashMap::new();
        let token = Token(0);
        let next_token = 1;
        let poll = Poll::new()?;
        let chat_service = ChatService::default();

        let router = Router::builder(chat_service)
            // Creates a chat between users
            .register("/chats", http::Method::POST, |svc, _, _, req| {
                println!("POST /chats {}", req.body());
                let chat = match serde_json::from_str::<super::messages::Chat>(req.body()) {
                    Ok(chat) => chat,
                    Err(e) => return error500(&format!("unable to parse json: {:?}", e)),
                };
                match svc.add_chat(chat) {
                    Ok(()) => status_ok(),
                    Err(e) => status_code_msg(
                        http::StatusCode::BAD_REQUEST,
                        format!("unable to add chat {:?}", e),
                        "text/plain",
                    ), //error500(&format!("unable to add chat: {:?}", e)),
                }
            })
            // Adds a message to a chat
            .register(
                "/chats/:chatId/messages",
                http::Method::POST,
                |svc, params, _, req| {
                    println!("POST, /chats/:chatId/messages {}", req.body());
                    let chat_id = params.get("chatId").unwrap();
                    let chat_id = match chat_id.parse::<u64>() {
                        Ok(chat_id) => chat_id,
                        Err(e) => return error500(&format!("unable to parse chat id: {:?}", e)),
                    };
                    let message = match serde_json::from_str::<Message>(req.body()) {
                        Ok(message) => message,
                        Err(e) => return error500(&format!("unable to parse json: {:?}", e)),
                    };
                    match svc.send_message(chat_id, message) {
                        Ok(()) => status_ok(),
                        Err(_) => not_found(),
                    }
                },
            )
            // Lists a user's current chats (query param userId required)
            .register("/chats", http::Method::GET, |svc, _, query, _| {
                println!("GET /chats");
                let query = match query {
                    Some(query) => query,
                    None => return super::router::not_found(),
                };
                let user_id = match query.get("userId") {
                    Some(user_id) => match user_id.parse::<u64>() {
                        Ok(user_id) => user_id,
                        Err(e) => {
                            return error500(&format!(
                                "unable to parse user_id from query string: {:?}",
                                e
                            ))
                        }
                    },
                    None => return not_found(),
                };
                let chats = svc.get_user_chats(user_id);
                match serde_json::to_string(&chats) {
                    Ok(json) => ok_json(json),
                    Err(e) => error500(&format!("unable to parse json: {:?}", e)),
                }
            })
            // Lists a chat's messages
            .register(
                "/chats/:chatId/messages",
                http::Method::GET,
                |svc, params, _, _| {
                    println!("GET /chats {:?}", params);
                    let chat_id = params.get("chatId").unwrap();
                    let chat_id = match chat_id.parse::<u64>() {
                        Ok(chat_id) => chat_id,
                        Err(e) => return error500(&format!("unable to parse json: {:?}", e)),
                    };
                    let messages = match svc.get_messages(chat_id) {
                        Ok(messages) => messages,
                        Err(e) => return error500(&format!("unable to get messages {:?}", e)),
                    };
                    match serde_json::to_string(&messages) {
                        Ok(json) => ok_json(json),
                        Err(e) => error500(&format!("unable to serialize messages: {:?}", e)),
                    }
                },
            )
            .build();

        poll.register(&listener, token, Ready::readable(), PollOpt::edge())?;
        Ok(Server {
            token,
            next_token,
            listener,
            events,
            connections,
            poll,
            router,
        })
    }

    pub fn poll(&mut self) -> Result<(), Box<dyn Error>> {
        self.poll.poll(&mut self.events, None)?;
        for event in self.events.iter() {
            match event.token() {
                token if token == self.token => loop {
                    match self.listener.accept() {
                        Ok((socket, _)) => {
                            let client_token = Token(self.next_token as usize);
                            self.next_token += 1;
                            self.poll.register(
                                &socket,
                                client_token,
                                Ready::readable(),
                                PollOpt::edge(),
                            )?;
                            self.connections.insert(client_token, Client::new(socket));
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            break;
                        }
                        _ => unreachable!(),
                    }
                },
                client_token => loop {
                    let client = self.connections.get_mut(&client_token).unwrap();
                    match client.read() {
                        Ok(bytes_read) => {
                            if bytes_read == 0 {
                                // socket closed
                                eprintln!("client socket closed {:?}", client_token);
                                self.connections.remove(&client_token);
                                break;
                            }
                            let requests = {
                                match crate::parse::parse_buffer(&client.buffer[0..bytes_read]) {
                                    Ok(requests) => requests,
                                    Err(e) => {
                                        eprintln!("error parsing buffer {:?}", e);
                                        break;
                                    }
                                }
                            };
                            for request in requests {
                                let response = self.router.route(request);
                                let response = response_to_string(response);
                                client.socket.write_all(&response[..].as_bytes()).unwrap();
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            break;
                        }
                        _ => unreachable!(),
                    }
                },
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_server() {
        let addr = "127.0.0.1:8080".parse().unwrap();
        let listener = TcpListener::bind(&addr).unwrap();
        let mut server = Server::new(listener).unwrap();
        let sock = TcpStream::connect(&addr).unwrap();
        server.poll().unwrap();
    }
}
