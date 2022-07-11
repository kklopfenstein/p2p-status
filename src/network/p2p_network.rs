use libp2p::{
    core::{upgrade, either::EitherOutput},
    floodsub::{Floodsub, Topic, FloodsubRpc, protocol::FloodsubProtocol},
    futures::{StreamExt},
    identity,
    mdns::{Mdns},
    mplex,
    noise::{Keypair, NoiseConfig, X25519Spec},
    PeerId,
    swarm::{Swarm, SwarmBuilder, ExpandedSwarm, IntoProtocolsHandlerSelect, protocols_handler::DummyProtocolsHandler, OneShotHandler}, tcp::TokioTcpConfig, Transport,
};
use tokio::{io::AsyncBufReadExt, sync::{mpsc::{self, UnboundedReceiver, Receiver, Sender}, oneshot}};
use once_cell::sync::Lazy;
use std::{collections::{HashSet, HashMap}, error::Error};

use clap::Parser;
use log::{error, info};

use crate::{behaviors::peer_status::PeerStatusBehaviour, requests::message_request::MessageRequest, admin::admin_server::AdminServer, network, responses::list_response::ListResponse, models::peer::Peer};
use crate::requests::list_request::{PeerListMode, PeerListRequest};
use crate::responses::list_response::ListEventType;

pub struct P2PNetwork {
    swarm: Swarm<PeerStatusBehaviour>,
    receiver: UnboundedReceiver<ListResponse>,
    command_receiver: Receiver<Command>,
    pub client: Client,
    peer_info_responses: HashMap<String, Peer>
}

static KEYS: Lazy<identity::Keypair> = Lazy::new(|| identity::Keypair::generate_ed25519());
static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("recipes"));

impl P2PNetwork {
    pub async fn new(description: String) -> P2PNetwork {
        info!("Peer Id: {}", PEER_ID.clone());

        let (response_sender, mut response_rcv) = mpsc::unbounded_channel();

        let auth_keys = Keypair::<X25519Spec>::new()
            .into_authentic(&KEYS)
            .expect("can create auth keys");

        let transp = TokioTcpConfig::new()
            .upgrade(upgrade::Version::V1)
            .authenticate(NoiseConfig::xx(auth_keys).into_authenticated()) // XX Handshake pattern, IX exists as well and IK - only XX currently provides interop with other libp2p impls
            .multiplex(mplex::MplexConfig::new())
            .boxed();

        info!("Description: {}", description);

        let mut behaviour = PeerStatusBehaviour {
            floodsub: Floodsub::new(PEER_ID.clone()),
            mdns: Mdns::new(Default::default())
                .await
                .expect("can create mdns"),
            response_sender,
            description: description,
            peer_id: format!("{}", *PEER_ID)
        };

        behaviour.floodsub.subscribe(TOPIC.clone());

        let mut swarm = SwarmBuilder::new(transp, behaviour, PEER_ID.clone())
            .executor(Box::new(|fut| {
                tokio::spawn(fut);
            }))
            .build();

        let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

        Swarm::listen_on(
            &mut swarm,
            "/ip4/0.0.0.0/tcp/0"
                .parse()
                .expect("can get a local socket"),
        )
            .expect("swarm can be started");

        let (command_sender, command_receiver) = mpsc::channel(1);

        Self {
            swarm: swarm,
            receiver: response_rcv,
            command_receiver: command_receiver,
            client: Client {
                sender: command_sender
            },
            peer_info_responses: HashMap::new()
        }

    }

    pub async fn listen(&mut self) {
        loop {
            let evt = {
                tokio::select! {
                    response = self.receiver.recv() => match response {
                        Some(e) => self.handle_event(e).await,
                        None => return,
                    },
                    event = self.swarm.select_next_some() => {
                        info!("Unhandled Swarm Event: {:?}", event);
                        ()
                    },
                    command = self.command_receiver.recv() => match command {
                        Some(c) => self.handle_command(c).await,
                        None => return,
                    },
                }
            };
        }
    }

    pub async fn handle_list_peers(&self) -> HashSet<PeerId> {
        let nodes = self.swarm.behaviour().mdns.discovered_nodes();
        let mut unique_peers = HashSet::new();
        for peer in nodes {
            unique_peers.insert(peer.clone());
        }
        unique_peers
    }

    async fn handle_command(&mut self, command: Command) {
        info!("handling command");
        match command {
            Command::GetAllPeerIds { sender } => {
                info!("sending handle_list_peers()");
                let peers = self.handle_list_peers().await;
                let peer_vec = peers.into_iter().map(|peer| peer.to_string()).collect();
                sender.send(peer_vec).expect("GetAllPeerIds to send");
            },
            Command::SendPeerInfoRequest { sender } => {
                info!("sending GetPeerInfo command");
                self.send_peer_list_request().await;
                sender.send(Ok(())).expect("command to send");
                self.peer_info_responses.clear();
            },
            Command::GetPeerInfoResponses { sender } => {
                info!("getting peer info responses");
                sender.send(self.peer_info_responses.clone()).expect("result to be present");
            }
        }
    }

    async fn handle_event(&mut self, event: ListResponse) {
        info!("Received response!");
        let json = serde_json::to_string(&event).expect("can jsonify response");
        self.swarm
            .behaviour_mut()
            .floodsub
            .publish(TOPIC.clone(), json.as_bytes());
        self.peer_info_responses.insert(event.data.id.clone(), event.data);
    }

    async fn send_peer_list_request(&mut self) {
        let req = PeerListRequest {
            mode: PeerListMode::ALL
        };
        let json = serde_json::to_string(&req).expect("can jsonify request");
        self.swarm
            .behaviour_mut()
            .floodsub
            .publish(TOPIC.clone(), json.as_bytes());
    }
}



async fn handle_message_send(cmd: &str, swarm: &mut Swarm<PeerStatusBehaviour>) {
    let rest = cmd.strip_prefix("send ");
    match rest {
      Some(message_content) => {
          let req = MessageRequest {
              message: String::from(message_content),
              hostname: gethostname::gethostname().to_str().unwrap().to_string()
          };
          let json = serde_json::to_string(&req).expect("can jsonify request");
          swarm
              .behaviour_mut()
              .floodsub
              .publish(TOPIC.clone(), json.as_bytes());
      }
      None => {
          info!("Message required.");
      }
  };
}

#[derive(Debug)]
enum Command {
    GetAllPeerIds {
        sender: oneshot::Sender<Vec<String>>
    },
    SendPeerInfoRequest {
        sender: oneshot::Sender<Result<(), ()>>
    },
    GetPeerInfoResponses {
        sender: oneshot::Sender<HashMap<String, Peer>>
    }
}

#[derive(Clone)]
pub struct Client {
    sender: Sender<Command>
}

impl Client {
    pub async fn get_peer_info(&mut self) -> Vec<String> {
        info!("Sending GetPeerInfo command");
        let (sender, receiver) = oneshot::channel();
        self.sender.send(Command::GetAllPeerIds { sender }).await.expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    pub async fn send_peer_info_request(&mut self) -> Result<(), ()> {
        info!("Sending peer info request");
        let (sender, receiver) = oneshot::channel();
        self.sender.send(Command::SendPeerInfoRequest { sender }).await.expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    pub async fn get_events(&mut self) -> HashMap<String, Peer> {
        info!("getting events");
        let (sender, receiver) = oneshot::channel();
        self.sender.send(Command::GetPeerInfoResponses { sender }).await.expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }
}