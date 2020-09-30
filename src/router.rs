use path_tree::PathTree;
use std::collections::HashMap;
use std::sync::Arc;

use qstring::QString;

use super::chat_service::ChatService;

pub type HttpHandler = Box<
    dyn Fn(
            &mut ChatService,
            HashMap<&str, &str>,
            Option<QString>,
            http::Request<&str>,
        ) -> http::Response<String>
        + Send
        + Sync
        + 'static,
>;

pub fn handler_fn<F>(f: F) -> HttpHandler
where
    F: Fn(
            &mut ChatService,
            HashMap<&str, &str>,
            Option<QString>,
            http::Request<&str>,
        ) -> http::Response<String>
        + Send
        + Sync
        + 'static,
{
    Box::new(f) as HttpHandler
}

pub struct Route {
    pub method: http::Method,
    pub handler: HttpHandler,
}

impl Route {
    pub fn new(method: http::Method, handler: HttpHandler) -> Self {
        Route { method, handler }
    }
}

pub struct RouterBuilder {
    trees: HashMap<http::Method, PathTree<Route>>,
    service: ChatService,
}

impl RouterBuilder {
    /// Call register on the builder to add a route
    pub fn register<F>(mut self, route: &str, method: http::Method, handler: F) -> Self
    where
        F: Fn(
                &mut ChatService,
                HashMap<&str, &str>,
                Option<QString>,
                http::Request<&str>,
            ) -> http::Response<String>
            + Send
            + Sync
            + 'static,
    {
        self.trees
            .get_mut(&method)
            .unwrap()
            .insert(route, Route::new(method, handler_fn(handler)));
        self
    }

    /// creates a materialized router for the given builder
    pub fn build(self) -> Router {
        Router {
            trees: Arc::new(self.trees),
            service: self.service,
        }
    }
}

pub struct Router {
    trees: Arc<HashMap<http::Method, PathTree<Route>>>,
    service: ChatService,
}

impl Router {
    pub fn builder(service: ChatService) -> RouterBuilder {
        let trees: HashMap<http::Method, PathTree<Route>> = {
            let mut tree = HashMap::new();
            tree.insert(http::Method::POST, PathTree::new());
            tree.insert(http::Method::GET, PathTree::new());
            tree
        };
        RouterBuilder { trees, service }
    }

    pub fn route(&mut self, req: http::Request<&str>) -> http::Response<String> {
        let trees = self.trees.clone();
        let path = req.uri().path().to_owned();
        let query = req.uri().query().to_owned();
        let query = query.map(QString::from);
        match trees.get(req.method()).unwrap().find(&path) {
            Some((route, params)) => {
                let handler = &route.handler;
                let res = handler(
                    &mut self.service,
                    params.into_iter().collect::<HashMap<_, _>>(),
                    query,
                    req,
                );
                if res.status() != http::StatusCode::OK {
                    println!("response: {:?}", res);
                }
                res
            }
            _ => {
                eprintln!("unknown route {}, method {:?}", path, req.method());
                not_found()
            }
        }
    }
}

pub fn not_found() -> http::Response<String> {
    status_code_msg(http::StatusCode::NOT_FOUND, "Not found.", "text/plain")
}

pub fn status_ok() -> http::Response<String> {
    status_code_msg(http::StatusCode::OK, String::new(), "text/plain")
}

pub fn ok_json<T: Into<String>>(body: T) -> http::Response<String> {
    status_code_msg(http::StatusCode::OK, body, "application/json")
}

pub fn error500(error_msg: &str) -> http::Response<String> {
    eprintln!("ERROR 500 : {}", error_msg);
    super::router::status_code_msg(
        http::StatusCode::INTERNAL_SERVER_ERROR,
        error_msg,
        "text/plain",
    )
}

pub fn status_code_msg<T: Into<String>>(
    code: http::StatusCode,
    msg: T,
    content_type: &str,
) -> http::Response<String> {
    http::Response::builder()
        .status(code)
        .header("Content-Type", content_type)
        .body(msg.into())
        .expect("unable to create response")
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_route_with_params_and_response() {
        let chat = ChatService::default();
        let mut router = Router::builder(chat)
            .register(
                "/home/:id/:answer",
                http::Method::GET,
                |_, params, _, req| {
                    assert_eq!(params.len(), 2, "Unexpected params len");
                    assert_eq!(params["id"], "42");
                    assert_eq!(params["answer"], "everything");
                    assert_eq!(*req.body(), "req body");
                    status_code_msg(http::StatusCode::OK, "body here", "text/plain")
                },
            )
            .build();

        let mut req = http::Request::builder();
        req.uri("/home/42/everything");
        let res = router.route(req.body("req body".into()).unwrap());
        assert_eq!(res.body(), "body here");
        assert_eq!(
            res.headers().get("Content-type"),
            Some(&http::HeaderValue::from_str("text/plain").unwrap())
        );
    }
}
