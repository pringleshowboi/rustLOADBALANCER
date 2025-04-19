use hyper::{Body, Client, Request, Response, Server, Uri};
use hyper::service::{make_service_fn, service_fn};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let addr = ([0, 0, 0, 0], 8080).into();

    // List of backend servers
    let backends = Arc::new(Mutex::new(vec![
        "http://localhost:3001",
        "http://localhost:3002",
        "http://localhost:3003",
    ]));

    // Shared state for round-robin index
    let index = Arc::new(Mutex::new(0));

    let make_svc = make_service_fn(move |_conn| {
        let backends = Arc::clone(&backends);
        let index = Arc::clone(&index);

        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let backends = Arc::clone(&backends);
                let index = Arc::clone(&index);
                forward_request(req, backends, index)
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);

    println!("Load balancer running on http://{}", addr);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

async fn forward_request(
    req: Request<Body>,
    backends: Arc<Mutex<Vec<&str>>>,
    index: Arc<Mutex<usize>>,
) -> Result<Response<Body>, Infallible> {
    let client = Client::new();

    // Pick backend server
    let backend_url = {
        let mut idx = index.lock().unwrap();
        let servers = backends.lock().unwrap();
        let server = servers[*idx % servers.len()];
        *idx = (*idx + 1) % servers.len();
        server.to_string()
    };

    // Build new URI
    let uri_string = format!(
        "{}{}",
        backend_url,
        req.uri().path_and_query().map(|x| x.as_str()).unwrap_or("/")
    );

    match uri_string.parse::<Uri>() {
        Ok(uri) => {
            // Forward the request
            let mut new_req = Request::builder()
                .method(req.method())
                .uri(uri);

            // Copy headers
            for (key, value) in req.headers() {
                new_req = new_req.header(key, value);
            }

            match new_req.body(req.into_body()) {
                Ok(forward_req) => match client.request(forward_req).await {
                    Ok(response) => Ok(response),
                    Err(err) => Ok(Response::new(Body::from(format!("Backend Error: {}", err)))),
                },
                Err(err) => Ok(Response::new(Body::from(format!("Request Build Error: {}", err)))),
            }
        }
        Err(err) => Ok(Response::new(Body::from(format!("URI Parse Error: {}", err)))),
    }
}
