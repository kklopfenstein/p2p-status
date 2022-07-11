use std::borrow::BorrowMut;
use std::cell::{Cell, RefCell, Ref};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use hyper::{Body, Request, Response, Server, StatusCode, Method, header};
use hyper::service::{make_service_fn, service_fn};
use log::info;
use rust_embed::RustEmbed;
use serde_json::json;
use tokio::join;

use crate::network::p2p_network::{P2PNetwork, Client};

static NOTFOUND: &[u8] = b"Not found";
static INDEX: &str = "index.html";
type GenericError = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, GenericError>;
pub struct AdminServer;

#[derive(RustEmbed)]
#[folder = "public/"]
struct Asset;

impl AdminServer {
  pub async fn start_server(port: u16, p2p_client: Client) {
    let addr = SocketAddr::from(([127, 0 , 0, 1], port.clone()));
    
    // let shared_network = Rc::new(RefCell::new(p2p_client));


    let make_svc = make_service_fn(move |_conn| {
      let admin_port = port.clone();
      // let local_shared_network = shared_network.clone();
      let local_client = p2p_client.clone();
      info!("Called service fn");
      async move {
        Ok::<_, Infallible>(
          service_fn(move |req: Request<Body>| {
            admin_service(req, admin_port.to_owned(), local_client.clone())
        }))
      }
    });

    let server = Server::bind(&addr).serve(make_svc);
    if let Err(e) = server.await {
      eprintln!("server error: {}", e);
    }

    // tokio::select! {
    //   Err(e) = server => {
    //     eprintln!("server error: {}", e);
    //   },
    //   () = network.listen() => {
    //     eprintln!("error");
    //   }
    // }
  }

}

  async fn admin_service(req: Request<Body>, port: u16, network: Client) -> Result< Response<Body>> {
    match (req.method(), req.uri().path()) {
      (&Method::GET, "/") | (&Method::GET, "/index.html") => simple_file_send(INDEX, port).await,
      (&Method::POST, "/api/send_ps") => api_send_ps(network).await,
      (&Method::GET, "/api/events") => get_events(network).await,
      _ => Ok(not_found()),
    }
  }


  async fn api_send_ps(mut network: Client) -> Result<Response<Body>> {
    let response = network.send_peer_info_request().await;
    let data: serde_json::Value = serde_json::Value::from("test_value");
    let json = serde_json::to_string(&data)?;
    let api_response = match response {
      Ok(_) => {
        Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "application/json")
          .body(Body::from(json))?
      },
      Err(_) => {
        Response::builder()
          .status(StatusCode::INTERNAL_SERVER_ERROR)
          .header(header::CONTENT_TYPE, "application/json")
          .body(Body::from(json))?
      }
    };
    Ok(api_response)
  }

  async fn get_events(mut network: Client) -> Result<Response<Body>> {
    let peers = network.get_events().await;
    let data = serde_json::Value::Array(peers.into_iter().map(|(_, peer)| { 
      json!({
        "id": peer.id,
        "hostname": peer.hostname,
        "description": peer.description
      })
    }).collect());
    let json = serde_json::to_string(&data)?;
    let response = Response::builder()
      .status(StatusCode::OK)
      .header(header::CONTENT_TYPE, "application/json")
      .body(Body::from(json))?;
    Ok(response)
  }

  fn not_found() -> Response<Body> {
    Response::builder()
      .status(StatusCode::NOT_FOUND)
      .body(NOTFOUND.into())
      .unwrap()
  }

  async fn simple_file_send(filename: &str, port: u16) -> Result<Response<Body>> {
      let file_content = Asset::get(filename).unwrap();
      let mut file_data = std::str::from_utf8(file_content.data.as_ref()).unwrap().to_string();
      file_data = file_data.replace("$ADMIN_PORT", port.to_string().as_str());

      let body = Body::from(file_data);
      return Ok(Response::new(body));
  }

// Since the Server needs to spawn some background tasks, we needed
// to configure an Executor that can spawn !Send futures...
#[derive(Clone, Copy, Debug)]
struct LocalExec;

impl<F> hyper::rt::Executor<F> for LocalExec
where
    F: std::future::Future + 'static, // not requiring `Send`
{
    fn execute(&self, fut: F) {
        // This will spawn into the currently running `LocalSet`.
        tokio::task::spawn_local(fut);
    }
}
