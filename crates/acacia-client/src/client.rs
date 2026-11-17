use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use acacia_auth::LoginCredentials;
use acacia_nethernet::Identity as NetIdentity;
use acacia_session::blob_store::BlobStore;
use acacia_session::pack_store::PackStore;
use acacia_session::{DisconnectReason, LinkConfig, Session, SessionConfig};
use p384::ecdsa::SigningKey;
use tokio::sync::{mpsc, oneshot};

use crate::blob_cache::BlobCache;
use crate::carried::{CarriedLogin, Carrier};
use crate::driver::{Command, Driver, Wire};
use crate::filter::PacketFilter;
use crate::friend::FriendJoin;
use crate::lan::LanServer;
use crate::login::{Identity, build_login, client_data};
use crate::pack_cache;
use crate::packet_trace::PacketTrace;
use crate::route;
use crate::signaling::SignalingTarget;
use crate::socks5::Socks5Proxy;
use crate::transport::{resolve, with_port};
use crate::{Client, ClientSender, ConnectError};

/// Which transport to join over.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TransportKind {
    /// RakNet if the server answers a RakNet ping, else NetherNet if it answers `GET /v1/join`.
    /// Offline logins always use RakNet.
    #[default]
    Auto,
    RakNet,
    /// WebRTC direct connect. An offline login offers a self-signed identity, which BDS refuses
    /// (error 37) and a host without online auth may accept.
    NetherNet,
}

pub enum Login {
    /// For `online-mode=false` servers.
    Offline { name: String },
    /// `credentials` must have been obtained for `key` (acacia-auth binds tokens to the client key).
    Online {
        credentials: Box<LoginCredentials>,
        key: SigningKey,
    },
}

/// How a join reaches the world.
enum Via {
    Address,
    Signaling(SignalingTarget),
    Lan(LanServer),
}

/// Adjusts the Login's client data; see [`ClientBuilder::client_data`].
pub(crate) type ClientDataFn = Box<dyn FnOnce(&mut acacia_auth::ClientData) + Send>;

pub struct ClientBuilder {
    server: String,
    via: Via,
    /// The Login `Nonce` a friend's world handed out.
    nonce: Option<String>,
    skin: Option<acacia_auth::login::Skin>,
    mouse_input: bool,
    client_data: Option<ClientDataFn>,
    /// Carries the connection in place of a socket ([`ClientBuilder::carried`]).
    carrier: Option<Carrier>,
    /// Held by the connection; dropping it leaves the friend's Xbox session.
    friend_session: Option<oneshot::Sender<()>>,
    blob_cache: BlobCache,
    blob_payloads: bool,
    pack_cache_dir: Option<PathBuf>,
    pack_store: Option<Arc<dyn PackStore>>,
    login: Login,
    proxy: Option<Socks5Proxy>,
    transport: TransportKind,
    chunk_radius: i32,
    auto_respawn: bool,
    initialize_on_spawn: bool,
    strict: bool,
    filter: PacketFilter,
    event_capacity: usize,
    login_timeout: Duration,
    max_batch_bytes: usize,
}

impl ClientBuilder {
    pub fn new(server: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            via: Via::Address,
            nonce: None,
            skin: None,
            mouse_input: false,
            client_data: None,
            carrier: None,
            friend_session: None,
            blob_cache: BlobCache::Memory,
            blob_payloads: false,
            pack_cache_dir: None,
            pack_store: None,
            login: Login::Offline {
                name: "Player".into(),
            },
            proxy: None,
            transport: TransportKind::Auto,
            chunk_radius: 2,
            auto_respawn: false,
            initialize_on_spawn: true,
            strict: false,
            filter: PacketFilter::all(),
            event_capacity: 256,
            login_timeout: Duration::from_secs(30),
            max_batch_bytes: acacia_session::batch::DEFAULT_MAX_DECOMPRESSED,
        }
    }

    /// The server as given to [`ClientBuilder::new`].
    pub fn server(&self) -> &str {
        &self.server
    }

    pub fn offline(mut self, name: impl Into<String>) -> Self {
        self.login = Login::Offline { name: name.into() };
        self
    }

    /// Wears `skin` instead of the account's pool skin.
    pub fn skin(mut self, skin: acacia_auth::login::Skin) -> Self {
        self.skin = Some(skin);
        self
    }

    /// Logs in with a mouse as the current input mode (default: touch, as the Android device it claims).
    /// Should match the input mode `PlayerAuthInput` reports.
    pub fn mouse_input(mut self, on: bool) -> Self {
        self.mouse_input = on;
        self
    }

    /// Adjusts the Login's client data after Acacia fills in its defaults, which describe an Android
    /// phone with touch input. A client on another platform sets `device_os`, the input modes and
    /// the other device claims to what that platform sends. The skin from [`ClientBuilder::skin`] and
    /// a friend's world's nonce are applied after `adjust` and take precedence.
    pub fn client_data(
        mut self,
        adjust: impl FnOnce(&mut acacia_auth::ClientData) + Send + 'static,
    ) -> Self {
        self.client_data = Some(Box::new(adjust));
        self
    }

    /// Has `carrier` carry the connection: it owns the transport and the keys and logs in with
    /// what it is told ([`CarriedLogin`]); everything after the key exchange stays here. Offline
    /// logins only.
    pub fn carried(mut self, carrier: Carrier) -> Self {
        self.carrier = Some(carrier);
        self
    }

    pub fn online(mut self, credentials: LoginCredentials, key: SigningKey) -> Self {
        self.login = Login::Online {
            credentials: Box::new(credentials),
            key,
        };
        self
    }

    /// Joins through the signaling service (e.g. a NetherNet realm) instead of dialing `server`.
    pub fn signaling(mut self, target: SignalingTarget) -> Self {
        self.via = Via::Signaling(target);
        self
    }

    /// Joins a world found by [`crate::discover_lan`] instead of dialing `server`.
    pub fn lan(mut self, server: LanServer) -> Self {
        self.via = Via::Lan(server);
        self
    }

    /// Joins a friend's world (see `join_friend_world`) through the signaling service, with its nonce.
    /// The connection keeps the Xbox session membership until it ends.
    pub fn friend(mut self, join: FriendJoin) -> Self {
        self.via = Via::Signaling(join.target);
        self.nonce = join.nonce;
        self.friend_session = Some(join.session);
        self
    }

    pub fn proxy(mut self, proxy: Socks5Proxy) -> Self {
        self.proxy = Some(proxy);
        self
    }

    pub fn transport(mut self, kind: TransportKind) -> Self {
        self.transport = kind;
        self
    }

    /// Respawn automatically whenever the player dies after spawning.
    pub fn auto_respawn(mut self, enabled: bool) -> Self {
        self.auto_respawn = enabled;
        self
    }

    /// Send SetLocalPlayerAsInitialized on spawn (default). Off when the caller sends it at the end
    /// of its loading-screen sequence, as vanilla does.
    pub fn initialize_on_spawn(mut self, enabled: bool) -> Self {
        self.initialize_on_spawn = enabled;
        self
    }

    /// Strict mode: every server packet is fully decoded, and what a strict peer would reject
    /// arrives as [`crate::Event::Violation`] whatever the packet filter (docs/testing.md).
    pub fn strict(mut self, enabled: bool) -> Self {
        self.strict = enabled;
        self
    }

    /// Requested view distance in chunks; small values cut most of an idle bot's inbound traffic.
    pub fn chunk_radius(mut self, radius: i32) -> Self {
        self.chunk_radius = radius;
        self
    }

    /// Deliver only these packet IDs (default: all).
    pub fn subscribe(mut self, ids: impl IntoIterator<Item = u32>) -> Self {
        self.filter = ids.into_iter().collect();
        self
    }

    pub fn filter(mut self, filter: PacketFilter) -> Self {
        self.filter = filter;
        self
    }

    /// Max undelivered events before the connection stops reading (backpressure).
    pub fn event_capacity(mut self, capacity: usize) -> Self {
        self.event_capacity = capacity.max(1);
        self
    }

    /// Largest decompressed server batch accepted; a bigger one ends the session as a protocol error.
    /// Default [`acacia_session::batch::DEFAULT_MAX_DECOMPRESSED`].
    pub fn max_batch_bytes(mut self, limit: usize) -> Self {
        self.max_batch_bytes = limit;
        self
    }

    pub fn login_timeout(mut self, timeout: Duration) -> Self {
        self.login_timeout = timeout;
        self
    }

    /// Keep the blob cache in `dir`, one file per account, across joins (default: per connection).
    pub fn blob_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.blob_cache = BlobCache::Dir(dir.into());
        self
    }

    pub fn blob_store(mut self, store: Arc<dyn BlobStore>) -> Self {
        self.blob_cache = BlobCache::Store(store);
        self
    }

    /// Report the blob cache as disabled, as before vanilla's cache was copied (not vanilla).
    pub fn without_blob_cache(mut self) -> Self {
        self.blob_cache = BlobCache::Off;
        self
    }

    /// Remember downloaded resource packs in `dir`, one file per account, so later joins skip the
    /// download like a vanilla player's cache (default: every join downloads).
    pub fn pack_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.pack_cache_dir = Some(dir.into());
        self
    }

    /// Decide yourself which resource packs count as already downloaded; replaces
    /// [`pack_cache_dir`](Self::pack_cache_dir). A store that has every pack never downloads.
    pub fn pack_store(mut self, store: Arc<dyn PackStore>) -> Self {
        self.pack_store = Some(store);
        self
    }

    /// Keep blob bytes, not just hashes: needed to read terrain ([`Client::blob_store`]).
    pub fn keep_blob_payloads(mut self, keep: bool) -> Self {
        self.blob_payloads = keep;
        self
    }

    /// Connects, logs in and resolves once the player has spawned.
    pub async fn connect(mut self) -> Result<Client, ConnectError> {
        if let Some(carrier) = self.carrier.take() {
            return self.connect_carried(carrier).await;
        }
        // Made here, not with the Login: a NetherNet offer is signed by the same key.
        let offline_key = SigningKey::random(&mut rand_core::OsRng);
        let net_identity = match &self.login {
            Login::Online { credentials, key } => credentials
                .multiplayer_token
                .clone()
                .map(|token| NetIdentity::multiplayer(key.clone(), token)),
            Login::Offline { .. } => Some(NetIdentity::self_signed(offline_key.clone())),
        };
        let transport = match (&self.login, self.transport) {
            (Login::Offline { .. }, TransportKind::Auto) => TransportKind::RakNet,
            (_, kind) => kind,
        };
        let route = match &self.via {
            Via::Signaling(target) => {
                route::open_signaling(target, self.proxy.as_ref(), net_identity.as_ref()).await?
            }
            Via::Lan(server) => route::open_lan(server, net_identity.as_ref()).await?,
            Via::Address => {
                let addr = resolve(&self.server).await?;
                route::open(
                    &self.server,
                    addr,
                    self.proxy.as_ref(),
                    transport,
                    net_identity.as_ref(),
                )
                .await?
            }
        };
        let addr = route.remote;
        let account = match &self.login {
            Login::Offline { name } => format!("offline:{name}"),
            Login::Online { credentials, .. } => format!("xbox:{}", credentials.xuid),
        };
        let blob_store = self.blob_cache.open(&account, self.blob_payloads);
        let (key, login_request, identity) = build_login(
            self.login,
            offline_key,
            &route.server_address,
            self.nonce,
            self.skin,
            self.mouse_input,
            self.client_data,
        );
        let cfg = SessionConfig {
            link: route.link,
            key,
            login_request,
            chunk_radius: self.chunk_radius,
            auto_respawn: self.auto_respawn,
            initialize_on_spawn: self.initialize_on_spawn,
            blob_store: blob_store.clone(),
            pack_store: self
                .pack_store
                .unwrap_or_else(|| pack_cache::open(self.pack_cache_dir.as_deref(), &account)),
            strict: self.strict,
            max_batch_bytes: self.max_batch_bytes,
        };
        let session = Session::new(cfg, addr, Instant::now());
        let started = Started {
            session,
            wire: route.wire,
            transport: Some(route.transport),
            carried: None,
            signaling: route.keepalive,
            identity,
            blob_store,
        };
        spawn(
            started,
            self.filter,
            self.event_capacity,
            self.proxy,
            self.friend_session,
            self.login_timeout,
        )
        .await
    }

    /// [`ClientBuilder::connect`] over a [`Carrier`]: no socket is opened and nothing is signed
    /// here.
    async fn connect_carried(mut self, carrier: Carrier) -> Result<Client, ConnectError> {
        let Login::Offline { name } = &self.login else {
            return Err(ConnectError::Io(std::io::Error::other(
                "a carried connection logs in offline",
            )));
        };
        let name = name.clone();
        let account = format!("offline:{name}");
        let blob_store = self.blob_cache.open(&account, self.blob_payloads);
        let client = client_data(
            &name,
            &with_port(&self.server),
            self.nonce.take(),
            self.skin.take(),
            self.mouse_input,
            self.client_data.take(),
        );
        let login = CarriedLogin {
            protocol: acacia_session::proto::PROTOCOL_VERSION,
            identity: acacia_auth::login::offline_identity(&name),
            display_name: name.clone(),
            client_data: serde_json::to_string(&client).map_err(std::io::Error::other)?,
        };
        if carrier.login.send(login).is_err() {
            return Err(ConnectError::Disconnected(DisconnectReason::LocalClose));
        }
        let cfg = SessionConfig {
            link: LinkConfig::Carried,
            // The carrier holds the connection's key and has sent the login; neither is used.
            key: SigningKey::random(&mut rand_core::OsRng),
            login_request: Vec::new(),
            chunk_radius: self.chunk_radius,
            auto_respawn: self.auto_respawn,
            initialize_on_spawn: self.initialize_on_spawn,
            blob_store: blob_store.clone(),
            pack_store: self
                .pack_store
                .unwrap_or_else(|| pack_cache::open(self.pack_cache_dir.as_deref(), &account)),
            strict: self.strict,
            max_batch_bytes: self.max_batch_bytes,
        };
        // The address is the RakNet link's; a carried link has none.
        let nowhere = std::net::SocketAddr::from(([0, 0, 0, 0], 0));
        let started = Started {
            session: Session::new(cfg, nowhere, Instant::now()),
            wire: Wire::Carried(carrier.outbound),
            transport: None,
            carried: Some(carrier.inbound),
            signaling: None,
            identity: Identity {
                display_name: name,
                xuid: String::new(),
            },
            blob_store,
        };
        spawn(
            started,
            self.filter,
            self.event_capacity,
            self.proxy,
            self.friend_session,
            self.login_timeout,
        )
        .await
    }
}

/// A connection ready for its task.
struct Started {
    session: Session,
    wire: Wire,
    transport: Option<crate::transport::Transport>,
    carried: Option<mpsc::UnboundedReceiver<crate::carried::Carried>>,
    signaling: Option<crate::signaling::Keepalive>,
    identity: Identity,
    blob_store: Option<Arc<dyn BlobStore>>,
}

/// Runs the connection's task and waits for the player to spawn.
async fn spawn(
    started: Started,
    filter: PacketFilter,
    event_capacity: usize,
    proxy: Option<Socks5Proxy>,
    friend_session: Option<oneshot::Sender<()>>,
    login_timeout: Duration,
) -> Result<Client, ConnectError> {
    let Started {
        session,
        wire,
        transport,
        carried,
        signaling,
        identity,
        blob_store,
    } = started;
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (ev_tx, ev_rx) = crate::event_queue::channel(event_capacity);
    let (spawn_tx, spawn_rx) = oneshot::channel();
    let trace = PacketTrace::default();
    let driver = Driver {
        session,
        wire,
        transport,
        carried,
        commands: cmd_rx,
        events: ev_tx,
        spawned: Some(spawn_tx),
        filter,
        trace: trace.clone(),
        capacity: event_capacity,
        proxy,
        _signaling: signaling,
        _friend_session: friend_session,
    };
    tokio::spawn(driver.run());

    let result = tokio::time::timeout(login_timeout, spawn_rx).await;
    match result {
        Ok(Ok(Ok(runtime_entity_id))) => Ok(Client {
            sender: ClientSender {
                commands: cmd_tx,
                identity: Arc::new(identity),
            },
            events: ev_rx,
            runtime_entity_id,
            blob_store,
            trace,
        }),
        Ok(Ok(Err(reason))) => Err(ConnectError::Disconnected(reason)),
        Ok(Err(_)) => Err(ConnectError::Disconnected(DisconnectReason::LocalClose)),
        Err(_) => {
            let _ = cmd_tx.send(Command::Close);
            Err(ConnectError::Timeout)
        }
    }
}
