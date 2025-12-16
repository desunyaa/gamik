use crate::structs::*;

use bincode::{Decode, Encode};
use iroh::{
    Endpoint, EndpointAddr, EndpointId,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler, Router},
};
use n0_error::{Result, StdResultExt};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

use tokio::sync::mpsc;

const ALPN: &[u8] = b"iroh-example/echo/0";
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10MB limit

#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientMessage {
    GameMessage(GameCommand),
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerMessage {
    EntityMap(EntityVec),
    PlayerID(EntityID),
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

pub async fn run_server_internal(world: GameWorld) -> Result<Router> {
    let endpoint = Endpoint::bind().await?;

    let router = Router::builder(endpoint)
        .accept(ALPN, Echo::new(world))
        .spawn();

    // router.endpoint().online().await;
    println!("Server started at {:#?}", router.endpoint().addr());
    tokio::time::sleep(Duration::from_millis(2000)).await;
    Ok(router)
}

#[derive(Debug, Clone)]
struct Echo {
    net_world: Arc<Mutex<GameWorld>>,
}

impl Echo {
    fn new(world: GameWorld) -> Self {
        Self {
            net_world: Arc::new(Mutex::new(world)),
        }
    }
}
// In network.rs - Update run_client_internal to accept a receiver channel
pub async fn run_client_internal(
    addr: impl Into<EndpointAddr>,
    tx: mpsc::UnboundedSender<Message>,
    mut rx: mpsc::UnboundedReceiver<GameCommand>, // New parameter
) -> Result<()> {
    let endpoint = Endpoint::bind().await?;
    //    endpoint.online().await;
    println!("client endpoint created: {:#?}", endpoint.addr());
    let conn = endpoint.connect(addr, ALPN).await?;
    println!("CLIENT CONNECTED");
    // Spawn a task to receive messages from server

    let conn_clone = conn.clone();
    tokio::spawn(async move {
        loop {
            match conn_clone.accept_uni().await {
                Ok(recv) => match recv_one_way(recv).await {
                    Ok(msg) => {
                        let _ = tx.send(msg.clone());
                        println!("Client received server message: {:#?}", msg);
                    }
                    Err(e) => {
                        eprintln!("Error receiving server message: {}", e);
                    }
                },
                Err(_) => {
                    println!("Server connection closed");
                    break;
                }
            }
        }
    });

    // Send messages from the UI to the server
    loop {
        // Wait for a message from the UI
        match rx.recv().await {
            Some(game_event) => {
                let client_msg = ClientMessage::GameMessage(game_event);
                let msg = Message::ClientMessage(client_msg);

                match send_one_way(&conn, &msg).await {
                    Ok(_) => {
                        println!("Message sent successfully: {:#?}", msg);
                    }
                    Err(e) => {
                        eprintln!("Error sending message: {}", e);
                        break;
                    }
                }
            }
            None => {
                println!("UI channel closed");
                break;
            }
        }
    }

    Ok(())
}

impl ProtocolHandler for Echo {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let endpoint_id = connection.remote_id();
        println!("Accepted connection from {}", endpoint_id);

        let world = self.net_world.clone();

        let conn_clone = connection.clone();
        // Spawn a task for periodic updates every 50ms
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(50));

            loop {
                interval.tick().await;
                let mut responses = Vec::new();
                // Lock the world, process any pending events, and generate update
                let client_update = {
                    let mut world_guard = world.lock().await;
                    world_guard.process_events();

                    if let Some(x) = world_guard
                        .unique_server_messages
                        .get_mut(&conn_clone.remote_id())
                    {
                        while let Some(a) = x.pop() {
                            responses.push(Message::ServerMessage(a));
                        }
                    }

                    world_guard.gen_client_info()
                };

                // Send the periodic update
                let response = Message::ServerMessage(ServerMessage::EntityMap(client_update));
                responses.push(response);

                for r in responses {
                    if let Err(e) = send_one_way(&conn_clone, &r).await {
                        eprintln!("Error sending periodic update to client: {}", e);
                        break; // Exit if connection is broken
                    }
                }
            }
        });

        // Accept unidirectional streams in a loop (for incoming messages)
        loop {
            let endpoint_id = connection.remote_id();

            match connection.accept_uni().await {
                Ok(recv) => {
                    let world = self.net_world.clone();

                    // Spawn a task to handle each stream independently
                    tokio::spawn(async move {
                        match recv_one_way(recv).await {
                            Ok(Message::ClientMessage(msg)) => {
                                match msg {
                                    ClientMessage::GameMessage(gmsg) => {
                                        // Lock only when needed, and add event to queue
                                        let mut world_guard = world.lock().await;

                                        match gmsg {
                                            GameCommand::SpawnPlayer(name) => {
                                                let pid = world_guard.spawn_player(name);
                                                world_guard.endpoints.insert(endpoint_id, pid);

                                                world_guard
                                                    .unique_server_messages
                                                    .entry(endpoint_id.clone())
                                                    .or_insert_with(Vec::new)
                                                    .push(ServerMessage::PlayerID(pid));
                                            }
                                            GameCommand::SpawnAs(eid) => {
                                                world_guard.endpoints.insert(endpoint_id, eid);

                                                world_guard
                                                    .unique_server_messages
                                                    .entry(endpoint_id.clone())
                                                    .or_insert_with(Vec::new)
                                                    .push(ServerMessage::PlayerID(eid));
                                            }

                                            _ => {
                                                if let Some(pid) =
                                                    world_guard.endpoints.get(&endpoint_id).cloned()
                                                {
                                                    world_guard.event_queue.push((pid, gmsg));
                                                }
                                            }
                                        }

                                        // Don't process events here - let the periodic task handle it
                                    }
                                }
                            }
                            Ok(Message::ServerMessage(_)) => {
                                eprintln!("Server received unexpected ServerMessage");
                            }
                            Ok(Message::Blank) => {
                                eprintln!("Server received unexpected Blank message");
                            }
                            Err(e) => {
                                eprintln!("Error receiving message: {}", e);
                            }
                        }
                    });
                }
                Err(_) => {
                    // Connection closed
                    println!("Connection closed");
                    break;
                }
            }
        }

        Ok(())
    }
}
