use libp2p::floodsub::{Floodsub, FloodsubEvent};
use libp2p::mdns::{Mdns, MdnsEvent};
use libp2p::swarm::NetworkBehaviourEventProcess;
use libp2p::NetworkBehaviour;
use log::{error, info};
use tokio::sync::mpsc;
use crate::models::peer::Peer;
use gethostname::gethostname;

use crate::requests::list_request::{PeerListMode, PeerListRequest};
use crate::responses::list_response::ListResponse;

#[derive(NetworkBehaviour)]
pub struct PeerStatusBehaviour {
    pub floodsub: Floodsub,
    pub mdns: Mdns,
    #[behaviour(ignore)]
    pub response_sender: mpsc::UnboundedSender<ListResponse>,
    #[behaviour(ignore)]
    pub description: String,
    #[behaviour(ignore)]
    pub peer_id: String
}

impl NetworkBehaviourEventProcess<FloodsubEvent> for PeerStatusBehaviour {
    fn inject_event(&mut self, event: FloodsubEvent) {
        match event {
            FloodsubEvent::Message(msg) => {
                info!("Receiving message.");
                if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                    info!("Got a ListResponse");
                    if resp.receiver == self.peer_id.to_string() {
                        info!("Response from {}:", msg.source);
                        info!("{:?}", resp.data);
                    } else {
                        info!("Wasn't our ListResponse. Our id is {} and the receiver was {}", self.peer_id.to_string(), resp.receiver);
                    }
                } else if let Ok(req) = serde_json::from_slice::<PeerListRequest>(&msg.data) {
                    info!("Got a PeerListRequest");
                    match req.mode {
                        PeerListMode::ALL => {
                            info!("Received ALL req: {:?} from {:?}", req, msg.source);
                            respond_with_peer_info(
                                self.response_sender.clone(),
                                msg.source.to_string(),
                                self.peer_id.clone(),
                                self.description.clone()
                            );
                        }
                        PeerListMode::One(ref peer_id) => {
                            if peer_id == &self.peer_id.to_string() {
                                info!("Received req: {:?} from {:?}", req, msg.source);
                                respond_with_peer_info(
                                    self.response_sender.clone(),
                                    msg.source.to_string(),
                                    self.peer_id.clone(),
                                    self.description.clone()
                                );
                            }
                        }
                    }
                }
            }
            _ => (),
        }
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for PeerStatusBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(discovered_list) => {
                for (peer, _addr) in discovered_list {
                    self.floodsub.add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(expired_list) => {
                for (peer, _addr) in expired_list {
                    if !self.mdns.has_node(&peer) {
                        self.floodsub.remove_node_from_partial_view(&peer);
                    }
                }
            }
        }
    }
}

fn respond_with_peer_info(sender: mpsc::UnboundedSender<ListResponse>, receiver: String, peer_id: String, description: String) {
    let resp = ListResponse {
        mode: PeerListMode::ALL,
        receiver,
        data: Peer {
            id: peer_id,
            hostname: gethostname().to_str().unwrap().to_string(),
            description
        },
    };
    if let Err(e) = sender.send(resp) {
        error!("error sending response via channel, {}", e);
    }
}
