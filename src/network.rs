use crate::structs::*;

use bincode::{Decode, Encode};
use iroh::{
    Endpoint, EndpointAddr,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler, Router},
};
use n0_error::{Result, StdResultExt};
use std::sync::Arc;
use tokio::sync::Mutex;

use egui::{FontId, RichText};
use tokio::sync::mpsc;

const ALPN: &[u8] = b"iroh-example/echo/0";
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10MB limit

#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientMessage {
    GameMessage(GameEvent),
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerMessage {
    EntityMap(EntityMap),
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum Message {
    ClientMessage(ClientMessage),
    ServerMessage(ServerMessage),
    Blank,
}

// ====================
// Unidirectional Stream Solution
// ====================

/// Send one message on a new unidirectional stream
async fn send_one_way(conn: &Connection, msg: &Message) -> Result<()> {
    let mut send = conn.open_uni().await.anyerr()?;

    let encoded = bincode::encode_to_vec(msg, bincode::config::standard())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    send.write_all(&encoded).await.anyerr()?;
    send.finish().anyerr()?;

    Ok(())
}

/// Receive one message from a unidirectional stream
async fn recv_one_way(mut recv: iroh::endpoint::RecvStream) -> Result<Message> {
    let bytes = recv.read_to_end(MAX_MESSAGE_SIZE).await.anyerr()?;

    let (msg, _) = bincode::decode_from_slice(&bytes, bincode::config::standard())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    Ok(msg)
}

async fn run_server_internal() -> Result<Router> {
    let endpoint = Endpoint::bind().await?;
    let router = Router::builder(endpoint).accept(ALPN, Echo::new()).spawn();
    println!("Server started at {:#?}", router.endpoint().addr());
    Ok(router)
}

async fn run_client_internal(addr: EndpointAddr, tx: mpsc::UnboundedSender<Message>) -> Result<()> {
    let endpoint = Endpoint::bind().await?;
    let conn = endpoint.connect(addr, ALPN).await?;

    // Spawn a task to receive messages from server
    let conn_clone = conn.clone();
    tokio::spawn(async move {
        loop {
            match conn_clone.accept_uni().await {
                Ok(recv) => {
                    match recv_one_way(recv).await {
                        Ok(msg) => {
                            // Send the server's count to the UI
                            let _ = tx.send(msg.clone());
                            println!("Client received server count: {:#?}", msg);
                        }
                        Err(e) => {
                            eprintln!("Error receiving server message: {}", e);
                        }
                    }
                }
                Err(_) => {
                    println!("Server connection closed");
                    break;
                }
            }
        }
    });

    // Send messages to server
    let mut message_count = 0u64;
    let client_msg = ClientMessage::GameMessage(GameEvent::Move {
        entity: EntityID(5),
        direction: Direction::Up,
    });

    loop {
        let msg = Message::ClientMessage(client_msg.clone());

        match send_one_way(&conn, &msg).await {
            Ok(_) => {
                message_count += 1;

                if message_count % 10 == 0 {
                    println!("Client sent {} messages", message_count);
                }
            }
            Err(e) => {
                eprintln!("Error sending message: {}", e);
                break;
            }
        }

        // Uncomment to slow down message sending
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct Echo {
    net_world: Arc<Mutex<GameWorld>>,
}

impl Echo {
    fn new() -> Self {
        Self {
            net_world: Arc::new(Mutex::new(GameWorld::create_test_world())),
        }
    }
}

impl ProtocolHandler for Echo {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let endpoint_id = connection.remote_id();
        println!("Accepted connection from {}", endpoint_id);

        // Accept unidirectional streams in a loop
        loop {
            match connection.accept_uni().await {
                Ok(recv) => {
                    let world = self.net_world.clone();

                    let conn_clone = connection.clone();

                    // Spawn a task to handle each stream independently
                    tokio::spawn(async move {
                        match recv_one_way(recv).await {
                            Ok(Message::ClientMessage(msg)) => {
                                match msg {
                                    ClientMessage::GameMessage(gmsg) => {
                                        // Lock only when needed, and drop the guard quickly
                                        let mut world_guard = world.lock().await;
                                        world_guard.event_queue.push(gmsg);
                                        world_guard.process_events();
                                        let client_update =
                                            world_guard.gen_client_info(EntityID(5));
                                        // Guard is dropped here when it goes out of scope

                                        // Send the response after releasing the lock
                                        let response = Message::ServerMessage(
                                            ServerMessage::EntityMap(client_update),
                                        );
                                        if let Err(e) = send_one_way(&conn_clone, &response).await {
                                            eprintln!("Error sending response to client: {}", e);
                                        }
                                    }
                                }
                            }
                            Ok(Message::ServerMessage(_)) => {
                                eprintln!("Server received unexpected ServerMessage");
                            }
                            Ok(Message::Blank) => {
                                eprintln!("Server received unexpected ServerMessage");
                            }
                            Err(e) => {
                                eprintln!("Error receiving message: {}", e);
                            }
                        }
                    });
                }
                Err(_) => {
                    // Connection closed
                    println!("Connection closed. Total messages received: ",);
                    break;
                }
            }
        }

        Ok(())
    }
}
pub async fn run_singleplayer_internal(tx: mpsc::UnboundedSender<Message>) -> Result<()> {
    let router = run_server_internal().await?;
    router.endpoint().online().await;
    let server_addr = router.endpoint().addr();

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Run client (will run infinitely)
    run_client_internal(server_addr, tx).await?;

    Ok(())
}
