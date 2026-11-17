use std::net::SocketAddr;
use std::time::{Duration, Instant};

use acacia_nethernet::{Connection, Event as NetEvent};
use acacia_session::{DisconnectReason, Event as SessionEvent, PackFetch, Session};

use crate::net_wire::NetherNetWire;
use crate::signaling::Keepalive;
use bytes::Bytes;
use tokio::sync::{mpsc, oneshot};

use crate::Event;
use crate::carried::Carried;
use crate::event_queue::EventSender;
use crate::filter::PacketFilter;
use crate::pack_fetch;
use crate::packet_trace::PacketTrace;
use crate::socks5::Socks5Proxy;
use crate::transport::Transport;

/// Largest datagram we accept; above the biggest RakNet MTU and the WebRTC (1200-byte) packets.
const RECV_BUFFER: usize = 2048;
/// How long the loop sleeps when neither the session nor the wire has a deadline.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) enum Command {
    Send(Bytes),
    Respawn,
    RespawnDone,
    Filter(PacketFilter),
    Close,
}

pub(crate) type SpawnResult = Result<u64, DisconnectReason>;

/// What sits between the session and the socket.
pub(crate) enum Wire {
    /// The session speaks RakNet datagrams directly.
    RakNet,
    /// The session's game messages travel over a WebRTC data channel.
    NetherNet(Box<NetherNetWire>),
    /// The session's batches go to a carrier that owns the connection (carried.rs).
    Carried(mpsc::UnboundedSender<Bytes>),
}

pub(crate) struct Driver {
    pub session: Session,
    pub wire: Wire,
    /// The socket; a carried connection has none.
    pub transport: Option<Transport>,
    /// What a carrier received, for [`Wire::Carried`].
    pub carried: Option<mpsc::UnboundedReceiver<Carried>>,
    pub commands: mpsc::UnboundedReceiver<Command>,
    pub events: EventSender,
    pub spawned: Option<oneshot::Sender<SpawnResult>>,
    pub filter: PacketFilter,
    pub trace: PacketTrace,
    pub capacity: usize,
    /// The connection's proxy, for pack downloads too.
    pub proxy: Option<Socks5Proxy>,
    /// Held for the connection's life; dropping it closes the signaling socket.
    pub _signaling: Option<Keepalive>,
    /// Held for the connection's life; dropping it leaves a friend's Xbox session.
    pub _friend_session: Option<oneshot::Sender<()>>,
}

impl Driver {
    /// Runs the connection until it closes; one task per connection.
    ///
    /// Never awaits the event channel directly: nobody reads it until login completes, and the
    /// login burst can exceed its capacity. Events queue locally instead; once spawned, socket
    /// reads pause while that queue is over capacity (backpressure without deadlock).
    pub async fn run(mut self) {
        let mut buf = vec![0u8; RECV_BUFFER];
        let mut pending: Vec<Event> = Vec::new();
        let mut seen: Vec<u32> = Vec::new();
        let (fetched_tx, mut fetched) = mpsc::unbounded_channel();
        // Downloads not yet reported; the channel is polled only while there are some.
        let mut fetching = 0usize;
        // One timer for the task's life: a fresh sleep each pass registered and cleared a wheel entry per datagram.
        let sleep = tokio::time::sleep(IDLE_TIMEOUT);
        tokio::pin!(sleep);
        loop {
            if let Err(e) = self.flush().await {
                return self
                    .finish(pending, DisconnectReason::Io(e.to_string()))
                    .await;
            }
            if let Some(reason) = self.wire_closed() {
                return self.finish(pending, reason).await;
            }
            let now = Instant::now();
            while let Some(event) = self.session.poll_event() {
                match event {
                    SessionEvent::Spawned { runtime_entity_id } => {
                        if let Some(tx) = self.spawned.take() {
                            let _ = tx.send(Ok(runtime_entity_id));
                        }
                    }
                    SessionEvent::Packet(p) => {
                        seen.push(p.id);
                        if self.filter.allows(p.id) {
                            pending.push(Event::Packet(p));
                        }
                    }
                    SessionEvent::Violation(v) => pending.push(Event::Violation(v)),
                    SessionEvent::Disconnected(reason) => {
                        self.trace.record(&seen, now);
                        let _ = self.flush().await;
                        self.close_wire();
                        let _ = self.flush().await;
                        return self.finish(pending, reason).await;
                    }
                }
            }
            if !seen.is_empty() {
                self.trace.record(&seen, now);
                seen.clear();
            }
            while let Some(pack) = self.session.poll_pack_fetch() {
                fetching += 1;
                tokio::spawn(fetch_pack(pack, self.proxy.clone(), fetched_tx.clone()));
            }
            // A pass's events go out as one message; the select below only waits for room in a full queue.
            if !pending.is_empty() && self.events.try_send(&mut pending).is_err() {
                self.session.close(Instant::now());
            }
            let can_read = self.spawned.is_some() || pending.len() < self.capacity;
            match self.poll_timeout() {
                Some(deadline) if sleep.deadline() != deadline.into() => {
                    sleep.as_mut().reset(deadline.into())
                }
                // Nothing due: an earlier deadline still armed only wakes handle_timeout early, which is harmless.
                None if sleep.is_elapsed() => {
                    sleep.as_mut().reset((Instant::now() + IDLE_TIMEOUT).into())
                }
                _ => {}
            }
            let failed = tokio::select! {
                sent = self.events.send(&mut pending), if !pending.is_empty() => {
                    if sent.is_err() {
                        self.session.close(Instant::now());
                    }
                    None
                }
                r = recv_from(self.transport.as_ref(), &mut buf), if can_read => match r {
                    Ok((range, source)) => {
                        tracing::trace!(len = range.len(), flags = buf[range.start], "datagram");
                        handle_datagram(&mut self.session, &mut self.wire, source, &buf[range]);
                        None
                    }
                    Err(e) => Some(DisconnectReason::Io(e.to_string())),
                },
                batch = next_carried(self.carried.as_mut()), if can_read => match batch {
                    Some(Carried::Batch(batch)) => {
                        self.session.handle_message(Instant::now(), batch);
                        None
                    }
                    Some(Carried::Closed(why)) => Some(DisconnectReason::Io(why)),
                    None => Some(DisconnectReason::Io("the carrier has gone".into())),
                },
                cmd = self.commands.recv() => {
                    match cmd {
                        Some(Command::Send(packet)) => self.session.send_raw(packet),
                        Some(Command::Respawn) => self.session.respawn(),
                        Some(Command::RespawnDone) => self.session.send_respawn_done(),
                        Some(Command::Filter(filter)) => self.filter = filter,
                        Some(Command::Close) | None => self.session.close(Instant::now()),
                    }
                    None
                }
                Some((id, ok)) = fetched.recv(), if fetching > 0 => {
                    fetching -= 1;
                    self.session.pack_fetched(Instant::now(), &id, ok);
                    None
                }
                _ = &mut sleep => {
                    handle_timeout(&mut self.session, &mut self.wire, Instant::now());
                    None
                }
            };
            if let Some(reason) = failed {
                return self.finish(pending, reason).await;
            }
        }
    }

    fn poll_timeout(&self) -> Option<Instant> {
        let session = self.session.poll_timeout();
        match &self.wire {
            Wire::RakNet | Wire::Carried(_) => session,
            Wire::NetherNet(conn) => session.into_iter().chain(conn.poll_timeout()).min(),
        }
    }

    /// The disconnect reason once the NetherNet connection has failed or closed.
    fn wire_closed(&mut self) -> Option<DisconnectReason> {
        let Wire::NetherNet(conn) = &mut self.wire else {
            return None;
        };
        while let Some(event) = conn.conn.poll_event() {
            if let NetEvent::Closed(reason) = event {
                return Some(DisconnectReason::NetherNet(reason));
            }
        }
        None
    }

    fn close_wire(&mut self) {
        if let Wire::NetherNet(conn) = &mut self.wire {
            conn.close(Instant::now());
        }
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        let now = Instant::now();
        match &mut self.wire {
            Wire::RakNet => {
                while let Some(datagram) = self.session.poll_transmit(now) {
                    if let Some(transport) = &self.transport {
                        transport.send(&datagram).await?;
                    }
                }
            }
            Wire::Carried(outbound) => {
                while let Some(batch) = self.session.poll_transmit(now) {
                    // Fails only if the carrier has gone, which its inbound end reports.
                    let _ = outbound.send(batch);
                }
            }
            Wire::NetherNet(conn) => {
                while let Some(msg) = self.session.poll_transmit(now) {
                    tracing::trace!(
                        len = msg.len(),
                        open = conn.conn.is_open(),
                        "game message out"
                    );
                    conn.conn.send(msg, now);
                }
                while let Some((target, datagram)) = conn.poll_datagram(now) {
                    // A server lists candidates we may not be able to reach (other families, LANs).
                    if let Some(transport) = &self.transport
                        && let Err(e) = transport.send_to(&datagram, target).await
                    {
                        tracing::trace!(%target, "send failed: {e}");
                    }
                }
            }
        }
        Ok(())
    }

    /// Delivers queued packets, then the disconnect (or fails the pending login).
    async fn finish(mut self, pending: Vec<Event>, reason: DisconnectReason) {
        if let Some(tx) = self.spawned.take() {
            let _ = tx.send(Err(reason));
            return;
        }
        let _ = self.events.send_final(
            pending
                .into_iter()
                .chain([Event::Disconnected(reason)])
                .collect(),
        );
    }
}

async fn fetch_pack(
    pack: PackFetch,
    proxy: Option<Socks5Proxy>,
    done: mpsc::UnboundedSender<(String, bool)>,
) {
    let result = tokio::time::timeout(
        pack_fetch::FETCH_TIMEOUT,
        pack_fetch::fetch(&pack.url, proxy.as_ref()),
    )
    .await;
    let ok = match result {
        Ok(Ok(size)) => {
            tracing::debug!(pack = pack.id, size, "resource pack downloaded");
            true
        }
        Ok(Err(e)) => {
            tracing::warn!(
                pack = pack.id,
                url = pack.url,
                "resource pack download: {e}"
            );
            false
        }
        Err(_) => {
            tracing::warn!(
                pack = pack.id,
                url = pack.url,
                "resource pack download timed out"
            );
            false
        }
    };
    let _ = done.send((pack.id, ok));
}

// Free functions: `select!` holds `transport` borrowed while these run.
fn handle_datagram(session: &mut Session, wire: &mut Wire, source: SocketAddr, data: &[u8]) {
    let now = Instant::now();
    match wire {
        Wire::RakNet => session.handle_datagram(now, Bytes::copy_from_slice(data)),
        // A carried connection has no socket to read datagrams from.
        Wire::Carried(_) => {}
        Wire::NetherNet(conn) => {
            conn.handle_datagram(now, source, data);
            deliver_messages(session, &mut conn.conn, now);
        }
    }
}

/// The socket's next datagram; never, without a socket.
async fn recv_from(
    transport: Option<&Transport>,
    buf: &mut [u8],
) -> std::io::Result<(std::ops::Range<usize>, SocketAddr)> {
    match transport {
        Some(transport) => transport.recv_from(buf).await,
        None => std::future::pending().await,
    }
}

/// What the carrier received next; never, without a carrier.
async fn next_carried(carried: Option<&mut mpsc::UnboundedReceiver<Carried>>) -> Option<Carried> {
    match carried {
        Some(carried) => carried.recv().await,
        None => std::future::pending().await,
    }
}

fn handle_timeout(session: &mut Session, wire: &mut Wire, now: Instant) {
    if let Wire::NetherNet(conn) = wire {
        conn.handle_timeout(now);
        deliver_messages(session, &mut conn.conn, now);
    }
    session.handle_timeout(now);
}

fn deliver_messages(session: &mut Session, conn: &mut Connection, now: Instant) {
    while let Some(msg) = conn.poll_message() {
        tracing::trace!(len = msg.len(), "game message in");
        session.handle_message(now, msg);
    }
}
