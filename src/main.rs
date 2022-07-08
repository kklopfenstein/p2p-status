use std::collections::HashSet;

use clap::Parser;
use libp2p::{
    core::upgrade,
    floodsub::{Floodsub, Topic},
    futures::StreamExt,
    identity,
    mdns::{Mdns},
    mplex,
    noise::{Keypair, NoiseConfig, X25519Spec},
    PeerId,
    swarm::{Swarm, SwarmBuilder}, tcp::TokioTcpConfig, Transport,
};
use log::{error, info};
use once_cell::sync::Lazy;
use tokio::{io::AsyncBufReadExt, sync::mpsc};

use p2p::{behaviors::peer_status::PeerStatusBehaviour, requests::message_request::MessageRequest};
use p2p::requests::list_request::{PeerListMode, PeerListRequest};
use p2p::responses::list_response::ListEventType;


static KEYS: Lazy<identity::Keypair> = Lazy::new(|| identity::Keypair::generate_ed25519());
static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("recipes"));
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, default_value = "")]
    description: String
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

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

    let args = Args::parse();
    info!("Description: {}", args.description);

    let mut behaviour = PeerStatusBehaviour {
        floodsub: Floodsub::new(PEER_ID.clone()),
        mdns: Mdns::new(Default::default())
            .await
            .expect("can create mdns"),
        response_sender,
        description: args.description,
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

    loop {
        let evt = {
            tokio::select! {
                line = stdin.next_line() => Some(ListEventType::Input(line.expect("can get line").expect("can read line from stdin"))),
                response = response_rcv.recv() => Some(ListEventType::Response(response.expect("response exists"))),
                event = swarm.select_next_some() => {
                    info!("Unhandled Swarm Event: {:?}", event);
                    None
                },
            }
        };

        if let Some(event) = evt {
            match event {
                ListEventType::Response(resp) => {
                    info!("Received response!");
                    let json = serde_json::to_string(&resp).expect("can jsonify response");
                    swarm
                        .behaviour_mut()
                        .floodsub
                        .publish(TOPIC.clone(), json.as_bytes());
                }
                ListEventType::Input(line) => match line.as_str() {
                    "ls" => handle_list_peers(&mut swarm).await,
                    cmd if cmd.starts_with("ps") => handle_list_peer_statuses(cmd, &mut swarm).await,
                    cmd if cmd.starts_with("send") => handle_message_send(cmd, &mut swarm).await,
                    _ => error!("unknown command"),
                },
            }
        }
    }
}

async fn handle_list_peers(swarm: &mut Swarm<PeerStatusBehaviour>) {
    info!("Discovered Peers:");
    let nodes = swarm.behaviour().mdns.discovered_nodes();
    let mut unique_peers = HashSet::new();
    for peer in nodes {
        unique_peers.insert(peer);
    }
    unique_peers.iter().for_each(|p| println!("{}", p));
}

async fn handle_list_peer_statuses(cmd: &str, swarm: &mut Swarm<PeerStatusBehaviour>) {
    let rest = cmd.strip_prefix("ps ");
    match rest {
        Some("*") => {
            info!("making request for all");
            let req = PeerListRequest {
                mode: PeerListMode::ALL
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json.as_bytes());
        }
        Some(recipes_peer_id) => {
            let req = PeerListRequest {
                mode: PeerListMode::One(recipes_peer_id.to_owned()),
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json.as_bytes());
        }
        None => {
            info!("No peers found.");
        }
    };
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