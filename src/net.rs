//! Networking layer.
//!
//! Provides a [`Transport`] trait abstracting over real sockets and test
//! channels, protocol message types, and the iroh-based server/client.

use crate::game::{self, EntityID, EntityMap, GameAction, GameState};
use crate::fov::{PlayerFov, PlayerFovMap, DEFAULT_FOV_RADIUS, FOV_NETWORK_MARGIN};

use bitcode::{Decode, Encode};
use iroh::{
    Endpoint, EndpointAddr, EndpointId,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler, Router},
};
use n0_error::{Result, StdResultExt};
use rustc_hash::FxHashMap;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ALPN: &[u8] = b"iroh-example/echo/0";
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10 MB

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

/// Maps connected endpoints to the entity they control.
pub type EndpointMap = FxHashMap<EndpointId, EntityID>;

// ---------------------------------------------------------------------------
// Protocol messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerMessage {
    EntityMap(EntityMap),
    PlayerID(EntityID),
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum Message {
    Client(GameAction),
    Server(ServerMessage),
}

// ---------------------------------------------------------------------------
// Transport trait
// ---------------------------------------------------------------------------

/// Abstraction over a network transport so that game logic and tests can work
/// with both real sockets and in-memory channels.
pub trait Transport: Send + Sync + 'static {
    /// Send a message to the remote peer.
    fn send(&self, msg: Message) -> std::result::Result<(), Box<dyn std::error::Error + Send>>;
    /// Try to receive a message (non-blocking).
    fn try_recv(&mut self) -> Option<Message>;
}

/// In-memory transport for testing.
pub struct MockTransport {
    pub tx: mpsc::UnboundedSender<Message>,
    pub rx: mpsc::UnboundedReceiver<Message>,
}

impl Transport for MockTransport {
    fn send(&self, msg: Message) -> std::result::Result<(), Box<dyn std::error::Error + Send>> {
        self.tx
            .send(msg)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
    }

    fn try_recv(&mut self) -> Option<Message> {
        self.rx.try_recv().ok()
    }
}

/// Create a pair of connected [`MockTransport`]s for testing.
pub fn mock_transport_pair() -> (MockTransport, MockTransport) {
    let (tx_a, rx_b) = mpsc::unbounded_channel();
    let (tx_b, rx_a) = mpsc::unbounded_channel();
    (
        MockTransport { tx: tx_a, rx: rx_a },
        MockTransport { tx: tx_b, rx: rx_b },
    )
}

// ---------------------------------------------------------------------------
// Server state (game state + networking bookkeeping)
// ---------------------------------------------------------------------------

/// State owned by the server: the authoritative game state plus networking
/// metadata that does not belong in the pure game layer.
#[derive(Debug)]
pub struct ServerState {
    pub game: GameState,
    pub endpoints: EndpointMap,
    pub unique_server_messages: FxHashMap<EndpointId, Vec<ServerMessage>>,
    pub event_queue: Vec<(EntityID, GameAction)>,
    pub player_fovs: PlayerFovMap,
}

impl ServerState {
    pub fn new(game: GameState) -> Self {
        Self {
            game,
            endpoints: EndpointMap::default(),
            unique_server_messages: FxHashMap::default(),
            event_queue: Vec::new(),
            player_fovs: PlayerFovMap::default(),
        }
    }

    /// Drain the event queue and apply each action to the game state.
    ///
    /// After processing, recomputes FOV for any player that moved.
    pub fn process_events(&mut self) {
        let events: Vec<(EntityID, GameAction)> = self.event_queue.drain(..).collect();

        let mut moved_players = Vec::new();

        for (eid, action) in &events {
            match action {
                GameAction::Move(_) => {
                    game::apply(&mut self.game, *eid, action);
                    moved_players.push(*eid);
                }
                GameAction::SpawnPlayer(_) | GameAction::SpawnAs(_) => {
                    // Handled at connection time in the protocol handler.
                }
                GameAction::SaveWorld => {
                    let _ = game::save_to_file(&self.game);
                }
            }
        }

        // Recompute FOV for players that moved.
        for pid in moved_players {
            if let Some(entity) = self.game.entities.get(&pid) {
                let origin = entity.position;
                self.player_fovs
                    .entry(pid)
                    .or_insert_with(|| PlayerFov::new(DEFAULT_FOV_RADIUS))
                    .recompute(origin, &self.game.entities);
            }
        }
    }

    /// Initialize FOV for a newly spawned/connected player.
    pub fn init_player_fov(&mut self, pid: EntityID) {
        if let Some(entity) = self.game.entities.get(&pid) {
            let origin = entity.position;
            let mut pfov = PlayerFov::new(DEFAULT_FOV_RADIUS);
            pfov.recompute(origin, &self.game.entities);
            self.player_fovs.insert(pid, pfov);
        }
    }

    /// Return the subset of entities visible to the given player (FOV + margin).
    pub fn entities_for_player(&self, pid: EntityID) -> EntityMap {
        let Some(pfov) = self.player_fovs.get(&pid) else {
            // No FOV computed yet — fall back to sending everything.
            return self.game.entities.clone();
        };
        let Some(player) = self.game.entities.get(&pid) else {
            return EntityMap::default();
        };

        let radius = pfov.fov_radius + FOV_NETWORK_MARGIN;
        let px = player.position.x;
        let py = player.position.y;

        let mut filtered = EntityMap::default();
        #[expect(clippy::iter_over_hash_type, reason = "order not significant for filtering")]
        for (&eid, entity) in &self.game.entities {
            let ex = entity.position.x;
            let ey = entity.position.y;
            let in_fov = pfov.current_fov.contains(&(ex, ey));
            let in_margin = (ex - px).abs() <= radius && (ey - py).abs() <= radius;
            if in_fov || in_margin {
                filtered.insert(eid, entity.clone());
            }
        }
        filtered
    }
}

// ---------------------------------------------------------------------------
// Iroh helpers
// ---------------------------------------------------------------------------

/// Send one message on a new unidirectional stream.
async fn send_one_way(conn: &Connection, msg: &Message) -> Result<()> {
    let mut send = conn.open_uni().await.anyerr()?;
    let encoded = bitcode::encode(msg);
    send.write_all(&encoded).await.anyerr()?;
    send.finish().anyerr()?;
    Ok(())
}

/// Receive one message from a unidirectional stream.
async fn recv_one_way(mut recv: iroh::endpoint::RecvStream) -> Result<Message> {
    let bytes = recv.read_to_end(MAX_MESSAGE_SIZE).await.anyerr()?;
    let msg = bitcode::decode(&bytes).anyerr()?;
    Ok(msg)
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

pub async fn run_server_internal(game: GameState) -> Result<Router> {
    let endpoint = Endpoint::bind().await?;

    let router = Router::builder(endpoint)
        .accept(ALPN, Echo::new(game))
        .spawn();

    tokio::time::sleep(Duration::from_millis(2000)).await;
    Ok(router)
}

#[derive(Debug, Clone)]
struct Echo {
    state: Arc<Mutex<ServerState>>,
}

impl Echo {
    fn new(game: GameState) -> Self {
        Self {
            state: Arc::new(Mutex::new(ServerState::new(game))),
        }
    }
}

impl ProtocolHandler for Echo {
    async fn accept(&self, connection: Connection) -> std::result::Result<(), AcceptError> {
        let state = self.state.clone();

        let conn_clone = connection.clone();
        // Periodic update task (50 ms tick)
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(50));

            loop {
                interval.tick().await;
                let mut responses = Vec::new();

                let client_update = {
                    let mut guard = state.lock().await;
                    guard.process_events();

                    if let Some(x) = guard
                        .unique_server_messages
                        .get_mut(&conn_clone.remote_id())
                    {
                        while let Some(a) = x.pop() {
                            responses.push(Message::Server(a));
                        }
                    }

                    // Send only entities within this player's FOV + margin.
                    let pid = guard.endpoints.get(&conn_clone.remote_id()).copied();
                    match pid {
                        Some(pid) => guard.entities_for_player(pid),
                        None => guard.game.entities.clone(),
                    }
                };

                let response = Message::Server(ServerMessage::EntityMap(client_update));
                responses.push(response);

                for r in responses {
                    if let Err(e) = send_one_way(&conn_clone, &r).await {
                        eprintln!("Error sending periodic update to client: {e}");
                        break;
                    }
                }
            }
        });

        // Accept incoming streams
        loop {
            let endpoint_id = connection.remote_id();

            match connection.accept_uni().await {
                Ok(recv) => {
                    let state = self.state.clone();

                    tokio::spawn(async move {
                        match recv_one_way(recv).await {
                            Ok(Message::Client(action)) => {
                                let mut guard = state.lock().await;

                                match action {
                                    GameAction::SpawnPlayer(name) => {
                                        let pid = game::spawn_player(&mut guard.game, name);
                                        guard.endpoints.insert(endpoint_id, pid);
                                        guard.init_player_fov(pid);
                                        guard
                                            .unique_server_messages
                                            .entry(endpoint_id)
                                            .or_default()
                                            .push(ServerMessage::PlayerID(pid));
                                    }
                                    GameAction::SpawnAs(eid) => {
                                        guard.endpoints.insert(endpoint_id, eid);
                                        guard.init_player_fov(eid);
                                        guard
                                            .unique_server_messages
                                            .entry(endpoint_id)
                                            .or_default()
                                            .push(ServerMessage::PlayerID(eid));
                                    }
                                    other => {
                                        if let Some(pid) =
                                            guard.endpoints.get(&endpoint_id).copied()
                                        {
                                            guard.event_queue.push((pid, other));
                                        }
                                    }
                                }
                            }
                            Ok(Message::Server(_)) => {
                                eprintln!("Server received unexpected ServerMessage");
                            }
                            Err(e) => {
                                eprintln!("Error receiving message: {e}");
                            }
                        }
                    });
                }
                Err(_) => {
                    break;
                }
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

pub async fn run_client_internal(
    addr: impl Into<EndpointAddr>,
    tx: mpsc::UnboundedSender<Message>,
    mut rx: mpsc::UnboundedReceiver<GameAction>,
) -> Result<()> {
    let endpoint = Endpoint::bind().await?;
    let conn = endpoint.connect(addr, ALPN).await?;

    // Receive loop
    let conn_clone = conn.clone();
    tokio::spawn(async move {
        loop {
            match conn_clone.accept_uni().await {
                Ok(recv) => match recv_one_way(recv).await {
                    Ok(msg) => {
                        let _ = tx.send(msg);
                    }
                    Err(e) => {
                        eprintln!("Error receiving server message: {e}");
                    }
                },
                Err(_) => {
                    break;
                }
            }
        }
    });

    // Send loop
    loop {
        match rx.recv().await {
            Some(action) => {
                let msg = Message::Client(action);

                if let Err(e) = send_one_way(&conn, &msg).await {
                    eprintln!("Error sending message: {e}");
                    break;
                }
            }
            None => {
                break;
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_transport_pair_round_trips() {
        let (a, mut b) = mock_transport_pair();

        a.send(Message::Client(GameAction::SaveWorld))
            .expect("send should succeed");
        let received = b.try_recv();
        assert!(received.is_some());
    }

    #[test]
    fn protocol_message_encodes_and_decodes() {
        let original = Message::Server(ServerMessage::PlayerID(EntityID(42)));
        let bytes = bitcode::encode(&original);
        let decoded: Message = bitcode::decode(&bytes).expect("decode should succeed");

        match decoded {
            Message::Server(ServerMessage::PlayerID(id)) => assert_eq!(id, EntityID(42)),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn server_state_process_events_applies_moves() {
        let game = GameState::create_test_world("test".into());
        let mut server = ServerState::new(game);

        let pid = game::spawn_player(&mut server.game, "Alice".into());
        let start = server.game.entities[&pid].position;

        server
            .event_queue
            .push((pid, GameAction::Move(game::Direction::Right)));
        server.process_events();

        assert_eq!(
            server.game.entities[&pid].position,
            game::Point {
                x: start.x + 1,
                y: start.y
            }
        );
    }

    #[test]
    fn entities_for_player_excludes_far_entities() {
        let game = GameState::create_test_world("test".into());
        let mut server = ServerState::new(game);

        let pid = game::spawn_player(&mut server.game, "Alice".into());
        server.init_player_fov(pid);

        // Place a distant entity far outside FOV + margin.
        let far_id = server.game.entity_gen.next_id();
        server.game.entities.insert(
            far_id,
            game::Entity {
                name: None,
                position: game::Point { x: 500, y: 500 },
                entity_type: game::EntityType::Tree,
            },
        );

        let filtered = server.entities_for_player(pid);
        assert!(
            !filtered.contains_key(&far_id),
            "distant entity should not be in FOV-scoped update"
        );
        assert!(
            filtered.contains_key(&pid),
            "player should see themselves"
        );
    }
}
