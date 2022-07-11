
use clap::Parser;
use p2p::admin::admin_server::AdminServer;
use p2p::network::p2p_network::P2PNetwork;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, default_value = "")]
    description: String,
    #[clap(short, long, value_parser, default_value = "3000")]
    admin_port: u16
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    // parse arguments
    let args = Args::parse();

    // initialize the p2p network
    let mut p2p_network = P2PNetwork::new(args.description).await;

    let p2p_client = p2p_network.client.clone();

    // initialize admin server
    let admin_server = AdminServer::start_server(args.admin_port, p2p_client);

    // if let Err(e) = server.await {
    //   eprintln!("server error: {}", e);
    // }

    tokio::select! {
      () = admin_server => {
        eprintln!("server error");
      },
      () = p2p_network.listen() => {
        eprintln!("error");
      }
    }
}